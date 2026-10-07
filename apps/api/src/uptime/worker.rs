use super::{
    allowance, eligible_ids,
    probe::{self, Observation},
    Monitor,
};
use crate::{ApiError, AppState};
use chrono::Utc;
use serde_json::json;
use sqlx::SqlitePool;
use std::time::Duration;
use tokio::{sync::watch, task::JoinSet};
use uuid::Uuid;

pub(crate) async fn run(state: AppState, mut shutdown: watch::Receiver<bool>) {
    if !super::ENABLED {
        tracing::info!("Uptime monitoring is disabled; existing monitors will not be probed");
        return;
    }
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut tasks = JoinSet::new();
    let mut cleanup_at = 0;
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { break; }
            }
            Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                if !matches!(result, Ok(Ok(()))) { tracing::warn!("Uptime check could not be persisted"); }
            }
            _ = tick.tick() => {
                let now = Utc::now().timestamp();
                if now >= cleanup_at {
                    // Only incident metadata lives in SQLite. Raw checks have a ClickHouse TTL.
                    if sqlx::query("DELETE FROM uptime_incidents WHERE ended_at IS NOT NULL AND ended_at < ?")
                        .bind(now - super::HISTORY_DAYS * 86400).execute(&state.sqlite).await.is_err() {
                        tracing::warn!("Uptime incident cleanup failed");
                    }
                    cleanup_at = now + 3600;
                }
                while tasks.len() < 16 {
                    match claim(&state.sqlite, now).await {
                        Ok(Some(monitor)) => { let state = state.clone(); tasks.spawn(async move { process(&state, monitor).await }); }
                        Ok(None) => break,
                        Err(_) => { tracing::warn!("Uptime scheduler could not claim work"); break; }
                    }
                }
            }
        }
    }
    // Let short in-flight requests finish without cancelling a ClickHouse write.
    while tasks.join_next().await.is_some() {}
}

pub(super) async fn claim(pool: &SqlitePool, now: i64) -> Result<Option<Monitor>, ApiError> {
    Ok(sqlx::query_as::<_, Monitor>("UPDATE uptime_monitors SET lease_token = ?, lease_until = ?, next_check_at = ? WHERE id = (SELECT m.id FROM uptime_monitors m JOIN sites s ON s.id = m.site_id WHERE m.enabled = 1 AND s.archived_at IS NULL AND s.deleted_at IS NULL AND s.erasure_pending_at IS NULL AND (m.next_check_at <= ? OR (m.lease_token IS NOT NULL AND m.lease_until <= ?)) AND (m.lease_until IS NULL OR m.lease_until <= ?) ORDER BY m.next_check_at, m.id LIMIT 1) RETURNING *")
        .bind(Uuid::new_v4().to_string()).bind(now + 120).bind(now + 300)
        .bind(now).bind(now).bind(now).fetch_optional(pool).await?)
}
async fn process(state: &AppState, monitor: Monitor) -> Result<(), ApiError> {
    let limit = allowance(());
    if !eligible_ids(&state.sqlite, &monitor.organization_id, limit)
        .await?
        .contains(&monitor.id)
    {
        suspend(&state.sqlite, &monitor).await?;
        return Ok(());
    }
    let started_at = Utc::now().timestamp();
    let mut observation = probe::check(&monitor.url).await;
    if !observation.healthy() && observation.failure.as_deref() != Some("checker_unavailable") {
        tokio::time::sleep(Duration::from_secs(10)).await;
        // Re-resolve/re-validate DNS for confirmation, and recheck pause/deletion/plan.
        if !still_enabled(&state.sqlite, &monitor).await? {
            return Ok(());
        }
        observation = probe::check(&monitor.url).await;
    }
    let tracking_id: Option<String> =
        sqlx::query_scalar("SELECT tracking_id FROM sites WHERE id = ?")
            .bind(&monitor.site_id)
            .fetch_optional(&state.sqlite)
            .await?;
    let Some(tracking_id) = tracking_id else {
        return Ok(());
    };
    let fence = state.clickhouse.ingestion_fence(&tracking_id);
    let _guard = fence.read().await;
    if let Some(check_state) = record(&state.sqlite, &monitor, &observation, started_at).await? {
        let row = json!({"site_id":tracking_id, "monitor_id":monitor.id, "checked_at":started_at,
            "status_code":observation.status_code, "duration_ms":observation.duration_ms,
            "state":check_state, "failure":observation.failure.unwrap_or_default()});
        // A history failure must not suppress a real incident or fabricate checks.
        // This write is not retried after an ambiguous response (no double counts).
        if state.clickhouse.insert_uptime_check(&row).await.is_err() {
            tracing::warn!(monitor_id = monitor.id, "Uptime history write failed");
        }
    }
    Ok(())
}
async fn still_enabled(pool: &SqlitePool, monitor: &Monitor) -> Result<bool, ApiError> {
    let same: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM uptime_monitors WHERE id = ? AND enabled = 1 AND lease_token = ?)")
        .bind(&monitor.id).bind(&monitor.lease_token).fetch_one(pool).await?;
    let limit = allowance(());
    Ok(same
        && eligible_ids(pool, &monitor.organization_id, limit)
            .await?
            .contains(&monitor.id))
}
async fn suspend(pool: &SqlitePool, monitor: &Monitor) -> Result<(), ApiError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let result = sqlx::query("UPDATE uptime_monitors SET state = 'unknown', last_checked_at = NULL, lease_token = NULL, lease_until = NULL WHERE id = ? AND lease_token = ?")
        .bind(&monitor.id).bind(&monitor.lease_token).execute(&mut *tx).await?;
    if result.rows_affected() > 0 {
        sqlx::query("UPDATE uptime_incidents SET ended_at = ?, resolution = 'monitoring_suspended' WHERE monitor_id = ? AND ended_at IS NULL")
            .bind(Utc::now().timestamp()).bind(&monitor.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(super) async fn record(
    pool: &SqlitePool,
    monitor: &Monitor,
    observation: &Observation,
    checked_at: i64,
) -> Result<Option<&'static str>, ApiError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let current = sqlx::query_as::<_, Monitor>("SELECT m.* FROM uptime_monitors m JOIN sites s ON s.id = m.site_id JOIN organizations o ON o.id = m.organization_id WHERE m.id = ? AND m.lease_token = ? AND m.enabled = 1 AND s.archived_at IS NULL AND s.deleted_at IS NULL AND s.erasure_pending_at IS NULL AND o.archived_at IS NULL")
        .bind(&monitor.id).bind(&monitor.lease_token).fetch_optional(&mut *tx).await?;
    let Some(current) = current else {
        return Ok(None);
    };
    let limit = allowance(());
    let ids: Vec<String> = sqlx::query_scalar("SELECT m.id FROM uptime_monitors m JOIN sites s ON s.id = m.site_id WHERE m.organization_id = ? AND m.enabled = 1 AND s.archived_at IS NULL AND s.deleted_at IS NULL AND s.erasure_pending_at IS NULL ORDER BY m.created_at, m.id LIMIT ?")
        .bind(&current.organization_id).bind(limit).fetch_all(&mut *tx).await?;
    if !ids.contains(&current.id) {
        return Ok(None);
    }
    let state = if observation.healthy() {
        "up"
    } else if observation.failure.as_deref() == Some("checker_unavailable") {
        "unknown"
    } else {
        "down"
    };
    let incident: Option<String> = sqlx::query_scalar(
        "SELECT id FROM uptime_incidents WHERE monitor_id = ? AND ended_at IS NULL",
    )
    .bind(&current.id)
    .fetch_optional(&mut *tx)
    .await?;
    let _transition = match (state, incident) {
        ("down", None) => {
            let id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO uptime_incidents (id, monitor_id, started_at, failure, status_code) VALUES (?, ?, ?, ?, ?)")
                .bind(&id).bind(&current.id).bind(checked_at).bind(&observation.failure).bind(observation.status_code.map(i64::from)).execute(&mut *tx).await?;
            Some((id, "down"))
        }
        ("up", Some(id)) => {
            sqlx::query(
                "UPDATE uptime_incidents SET ended_at = ?, resolution = 'recovered' WHERE id = ?",
            )
            .bind(checked_at)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
            Some((id, "recovered"))
        }
        _ => None,
    };
    sqlx::query("UPDATE uptime_monitors SET state = ?, last_checked_at = ?, status_code = ?, duration_ms = ?, failure = ?, lease_token = NULL, lease_until = NULL WHERE id = ? AND lease_token = ?")
        .bind(state).bind(checked_at).bind(observation.status_code.map(i64::from)).bind(observation.duration_ms as i64)
        .bind(&observation.failure).bind(&current.id).bind(&current.lease_token).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(state))
}
