use super::*;
use sqlx::SqliteConnection;

pub(crate) const TABLES: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS ai_requests (id TEXT PRIMARY KEY, site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE, user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE, status TEXT NOT NULL CHECK(status IN ('pending','completed','failed')), expires_at INTEGER NOT NULL, created_at INTEGER NOT NULL)",
    "CREATE INDEX IF NOT EXISTS idx_ai_requests_recent ON ai_requests(organization_id, created_at)",
    "CREATE INDEX IF NOT EXISTS idx_ai_requests_org ON ai_requests(organization_id, status, expires_at)",
];

pub(super) async fn scope_on(
    connection: &mut SqliteConnection,
    site: &sites::ActiveSite,
    user: &str,
) -> Result<bool, ApiError> {
    let owner = sqlx::query_scalar::<_, bool>(r#"
        SELECT (COALESCE(s.created_by_user_id = ?, 0) OR EXISTS(SELECT 1 FROM site_memberships sm WHERE sm.site_id=s.id AND sm.user_id=? AND sm.role='owner'))
        FROM sites s JOIN organizations o ON o.id=s.organization_id JOIN users u ON u.id=?
        WHERE s.id=? AND s.organization_id=? AND s.tracking_id=? AND s.archived_at IS NULL AND s.deleted_at IS NULL AND s.erasure_pending_at IS NULL
        AND o.archived_at IS NULL AND u.status='active'
        AND (s.created_by_user_id=? OR EXISTS(SELECT 1 FROM site_memberships sm WHERE sm.site_id=s.id AND sm.user_id=?))
        AND (o.created_by_user_id=? OR EXISTS(SELECT 1 FROM organization_members om WHERE om.organization_id=o.id AND om.user_id=?))
    "#).bind(user).bind(user).bind(user).bind(&site.id).bind(site.organization_id.as_deref()).bind(&site.tracking_id)
        .bind(user).bind(user).bind(user).bind(user).fetch_optional(connection).await?;
    owner.ok_or_else(|| {
        ApiError::Forbidden("You do not have access to this app and organization".into())
    })
}

async fn authorize_on(
    connection: &mut SqliteConnection,
    site: &sites::ActiveSite,
    user: &str,
    _exclude: &str,
) -> Result<(), ApiError> {
    scope_on(connection, site, user).await?;
    let enabled: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT ai_enabled FROM site_settings WHERE site_id=?),0)",
    )
    .bind(&site.id)
    .fetch_one(connection)
    .await?;
    if !enabled {
        return Err(ApiError::Forbidden("AI is disabled for this app".into()));
    }
    Ok(())
}

pub(super) async fn reserve(
    pool: &SqlitePool,
    site: &sites::ActiveSite,
    user: &str,
    id: &str,
) -> Result<(), ApiError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    scope_on(&mut tx, site, user).await?;
    let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ai_requests WHERE id=?)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if duplicate {
        return Err(ApiError::Conflict(
            "This AI request was already submitted. It will not be executed again.".into(),
        ));
    }
    authorize_on(&mut tx, site, user, "").await?;
    let attempts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ai_requests WHERE organization_id=? AND created_at>unixepoch()-60",
    )
    .bind(site.organization_id.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_requests WHERE organization_id=? AND status='pending' AND expires_at>unixepoch()")
        .bind(site.organization_id.as_deref()).fetch_one(&mut *tx).await?;
    if attempts >= 5 || pending >= 2 {
        return Err(ApiError::TooManyRequests {
            retry_after_seconds: 60,
        });
    }
    sqlx::query("UPDATE ai_requests SET status='failed' WHERE status='pending' AND expires_at<=unixepoch() AND organization_id=?")
        .bind(site.organization_id.as_deref()).execute(&mut *tx).await?;
    sqlx::query(
        "DELETE FROM ai_requests WHERE created_at<unixepoch()-7776000 AND organization_id=?",
    )
    .bind(site.organization_id.as_deref())
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO ai_requests(id,site_id,user_id,organization_id,status,expires_at,created_at) VALUES (?,?,?,?,'pending',unixepoch()+120,unixepoch())")
        .bind(id).bind(&site.id).bind(user).bind(site.organization_id.as_deref()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn fail(pool: &SqlitePool, id: &str) -> Result<(), ApiError> {
    sqlx::query("UPDATE ai_requests SET status='failed' WHERE id=? AND status='pending'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(super) async fn commit(
    pool: &SqlitePool,
    site: &sites::ActiveSite,
    user: &str,
    id: &str,
) -> Result<(), ApiError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    authorize_on(&mut tx, site, user, id).await?;
    let changed=sqlx::query("UPDATE ai_requests SET status='completed' WHERE id=? AND site_id=? AND user_id=? AND status='pending' AND expires_at>unixepoch()").bind(id).bind(&site.id).bind(user).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(ApiError::Conflict(
            "AI request expired; submit a new question".into(),
        ));
    }
    tx.commit().await?;
    Ok(())
}
