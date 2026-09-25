use crate::{sites::ActiveSite, storage::clickhouse::ClickHouse, ApiError, AppState};
use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;
const LEASE_MINUTES: i64 = 5;
pub(crate) const ACTIVE_ROW_PREDICATE: &str =
    "(retention_active_until IS NULL OR retention_active_until > now64(3))";
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetentionStamp {
    pub active_until: Option<DateTime<Utc>>,
    pub policy_version: u16,
    pub purge_after: Option<DateTime<Utc>>,
    pub tier: &'static str,
}
pub(crate) async fn stamp_for_ingest(
    _state: &AppState,
    _site: &ActiveSite,
    _received: DateTime<Utc>,
) -> Result<RetentionStamp, ApiError> {
    Ok(RetentionStamp {
        active_until: None,
        purge_after: None,
        policy_version: 1,
        tier: "self_hosted",
    })
}
fn truncate_error(error: &str) -> String {
    error.chars().take(500).collect()
}
pub(crate) async fn execute_site_erasure(
    clickhouse: &ClickHouse,
    pool: &SqlitePool,
    site: &ActiveSite,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let job_id = Uuid::new_v4().to_string();
    let dedupe_key = format!("erasure:{}", site.id);
    let now_text = now.to_rfc3339();
    let lock_expires_at = (now + Duration::minutes(LEASE_MINUTES)).to_rfc3339();
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "UPDATE retention_jobs SET status = 'cancelled', locked_by = NULL, lock_expires_at = NULL, updated_at = ? WHERE site_id = ? AND kind != 'erasure' AND status IN ('pending', 'retry', 'running')",
    )
    .bind(&now_text)
    .bind(&site.id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO retention_jobs (
            id, site_id, site_tracking_id, kind, dedupe_key, priority,
            status, next_attempt_at, submitted_at, locked_by, lock_expires_at
        ) VALUES (?, ?, ?, 'erasure', ?, 100, 'running', ?, ?, 'synchronous_site_delete', ?)
        ON CONFLICT(dedupe_key) DO UPDATE SET
            status = 'running', priority = 100,
            locked_by = excluded.locked_by,
            lock_expires_at = excluded.lock_expires_at,
            submitted_at = excluded.submitted_at,
            updated_at = excluded.submitted_at
        "#,
    )
    .bind(&job_id)
    .bind(&site.id)
    .bind(&site.tracking_id)
    .bind(&dedupe_key)
    .bind(&now_text)
    .bind(&now_text)
    .bind(&lock_expires_at)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE sites SET erasure_pending_at = COALESCE(erasure_pending_at, ?), updated_at = ? WHERE id = ? AND archived_at IS NULL AND deleted_at IS NULL",
    )
    .bind(&now_text)
    .bind(&now_text)
    .bind(&site.id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    let persisted_job_id: String =
        sqlx::query_scalar("SELECT id FROM retention_jobs WHERE dedupe_key = ?")
            .bind(&dedupe_key)
            .fetch_one(pool)
            .await?;
    match clickhouse
        .delete_site_events(&site.tracking_id, &persisted_job_id)
        .await
    {
        Ok(()) => {
            crate::uptime::erase_site_configuration(pool, &site.id).await?;
            let completed_at = Utc::now();
            let digest =
                blake3::hash(format!("site-erasure:{}:{now_text}", site.tracking_id).as_bytes())
                    .to_hex()
                    .to_string();
            sqlx::query(
                "UPDATE retention_jobs SET status = 'completed', mutation_digest = ?, completed_at = ?, locked_by = NULL, lock_expires_at = NULL, last_error = NULL, updated_at = ? WHERE dedupe_key = ?",
            )
            .bind(digest)
            .bind(completed_at.to_rfc3339())
            .bind(completed_at.to_rfc3339())
            .bind(dedupe_key)
            .execute(pool)
            .await?;
            Ok(())
        }
        Err(error) => {
            let failed_at = Utc::now();
            let mut transaction = pool.begin().await?;
            sqlx::query(
                "UPDATE retention_jobs SET status = 'retry', attempt_count = attempt_count + 1, next_attempt_at = ?, locked_by = NULL, lock_expires_at = NULL, last_error = ?, updated_at = ? WHERE dedupe_key = ?",
            )
            .bind((failed_at + Duration::minutes(1)).to_rfc3339())
            .bind(truncate_error(&error.to_string()))
            .bind(failed_at.to_rfc3339())
            .bind(dedupe_key)
            .execute(&mut *transaction)
            .await?;
            // Keep ingestion fenced: a durable anonymous snapshot must cover
            // exactly the rows deleted by every retry of this same job.
            transaction.commit().await?;
            Err(error)
        }
    }
}

/// Resume interrupted explicit erasures. No automatic analytics expiry runs.
pub(crate) async fn run_erasure_worker(
    state: AppState,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        tokio::select! {
            _=interval.tick()=>{
                let result=async {
                    let sites=sqlx::query_as::<_,ActiveSite>("SELECT s.id,s.tracking_id,s.organization_id,s.created_by_user_id,s.team_id FROM sites s JOIN retention_jobs j ON j.site_id=s.id AND j.kind='erasure' WHERE s.erasure_pending_at IS NOT NULL AND s.deleted_at IS NULL AND (j.status='completed' OR (j.status IN ('pending','retry') AND unixepoch(j.next_attempt_at)<=unixepoch()) OR (j.status='running' AND unixepoch(j.lock_expires_at)<=unixepoch())) LIMIT 20").fetch_all(&state.sqlite).await?;
                    for site in sites {
                        if let Err(error)=crate::sites::submit_site_deletion(&state.clickhouse,&state.sqlite,&site,&Utc::now().to_rfc3339()).await { tracing::warn!(site_id=%site.id, error_kind=?std::mem::discriminant(&error),"site erasure will retry"); }
                    }
                    Ok::<_,sqlx::Error>(())
                }.await;
                if result.is_err(){tracing::warn!("Could not scan pending site erasures");}
            }
            changed=shutdown.changed()=>{if changed.is_err() || *shutdown.borrow(){return;}}
        }
    }
}
