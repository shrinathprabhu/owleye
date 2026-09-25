//! Dashboard-managed availability monitoring; independent of SDK traffic quotas.
mod probe;
mod worker;
use crate::{auth, models::clickhouse_string, sites, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;
pub(crate) use worker::run;

pub(crate) const HISTORY_DAYS: i64 = 3;
pub(crate) const HISTORY_CHECK_LIMIT: i64 = HISTORY_DAYS * 24 * 12;

pub(crate) const TABLES: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS uptime_monitors (
        id TEXT PRIMARY KEY, site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
        organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
        name TEXT NOT NULL, url TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1,
        state TEXT NOT NULL DEFAULT 'unknown', last_checked_at INTEGER,
        status_code INTEGER, duration_ms INTEGER, failure TEXT,
        next_check_at INTEGER NOT NULL DEFAULT 0, lease_token TEXT, lease_until INTEGER,
        created_at INTEGER NOT NULL, UNIQUE(site_id, url))",
    "CREATE INDEX IF NOT EXISTS idx_uptime_due ON uptime_monitors(enabled, next_check_at, lease_until)",
    "CREATE INDEX IF NOT EXISTS idx_uptime_org ON uptime_monitors(organization_id, created_at, id)",
    "CREATE TABLE IF NOT EXISTS uptime_incidents (
        id TEXT PRIMARY KEY, monitor_id TEXT NOT NULL REFERENCES uptime_monitors(id) ON DELETE CASCADE,
        started_at INTEGER NOT NULL, ended_at INTEGER, resolution TEXT,
        failure TEXT NOT NULL, status_code INTEGER)",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_uptime_open_incident ON uptime_incidents(monitor_id) WHERE ended_at IS NULL",
    "CREATE INDEX IF NOT EXISTS idx_uptime_incident_history ON uptime_incidents(monitor_id, started_at)",
];

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub(super) struct Monitor {
    pub id: String,
    pub site_id: String,
    pub organization_id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub state: String,
    pub last_checked_at: Option<i64>,
    pub status_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub failure: Option<String>,
    pub created_at: i64,
    #[serde(skip)]
    pub lease_token: Option<String>,
}
#[derive(Serialize, sqlx::FromRow)]
struct Incident {
    id: String,
    started_at: i64,
    ended_at: Option<i64>,
    resolution: Option<String>,
    failure: String,
    status_code: Option<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateMonitor {
    name: String,
    url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateMonitor {
    enabled: Option<bool>,
}

pub(crate) fn allowance(_plan: ()) -> i64 {
    i64::MAX
}
async fn access(
    state: &AppState,
    headers: &HeaderMap,
    identifier: &str,
    manage: bool,
) -> Result<sites::ActiveSite, ApiError> {
    let session = auth::authenticated_session(state, headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = if manage {
        sites::resolve_site_for_admin(&state.sqlite, &session.id, identifier).await?
    } else {
        sites::resolve_site_for_user(&state.sqlite, &session.id, identifier).await?
    };
    let member: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organizations o WHERE o.id = ? AND o.archived_at IS NULL AND (o.created_by_user_id = ? OR EXISTS(SELECT 1 FROM organization_members m WHERE m.organization_id = o.id AND m.user_id = ?)))")
        .bind(&site.organization_id).bind(&session.id).bind(&session.id).fetch_one(&state.sqlite).await?;
    if !member {
        return Err(ApiError::Forbidden(
            "Organization membership is required".into(),
        ));
    }
    Ok(site)
}

pub(crate) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let site = access(&state, &headers, &site_id, false).await?;
    let org = site.organization_id.as_deref().unwrap();
    let limit = allowance(());
    let used: i64 =
        sqlx::query_scalar("SELECT count(*) FROM uptime_monitors WHERE organization_id = ?")
            .bind(org)
            .fetch_one(&state.sqlite)
            .await?;
    let eligible = eligible_ids(&state.sqlite, org, limit).await?;
    let mut monitors = sqlx::query_as::<_, Monitor>(
        "SELECT * FROM uptime_monitors WHERE site_id = ? ORDER BY created_at, id",
    )
    .bind(&site.id)
    .fetch_all(&state.sqlite)
    .await?;
    for monitor in &mut monitors {
        monitor.state = if !monitor.enabled {
            "paused"
        } else if !eligible.contains(&monitor.id) {
            "disabled"
        } else if monitor
            .last_checked_at
            .is_none_or(|time| Utc::now().timestamp() - time > 660)
        {
            "unknown"
        } else {
            &monitor.state
        }
        .to_owned();
    }
    Ok(Json(
        json!({"monitors":monitors, "limit":limit, "used":used, "interval_seconds":300, "history_days":HISTORY_DAYS}),
    ))
}
pub(crate) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<String>,
    Json(input): Json<CreateMonitor>,
) -> Result<Json<Value>, ApiError> {
    let site = access(&state, &headers, &site_id, true).await?;
    let name = input.name.trim();
    if name.is_empty() || name.len() > 100 || name.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "Monitor name must be 1–100 bytes without control characters".into(),
        ));
    }
    let url = probe::parse_url(&input.url)?.to_string();
    let org = site.organization_id.as_deref().unwrap();
    // IMMEDIATE serializes monitor creation writes.
    let mut tx = state.sqlite.begin_with("BEGIN IMMEDIATE").await?;
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sites WHERE id = ? AND archived_at IS NULL AND deleted_at IS NULL AND erasure_pending_at IS NULL)")
        .bind(&site.id).fetch_one(&mut *tx).await?;
    if !active {
        return Err(ApiError::Conflict("This project is being deleted".into()));
    }
    let limit = allowance(());
    if limit == 0 {
        return Err(ApiError::Forbidden(
            "Uptime monitoring requires Pro or Business".into(),
        ));
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM uptime_monitors WHERE organization_id = ?")
            .bind(org)
            .fetch_one(&mut *tx)
            .await?;
    if count >= limit {
        return Err(ApiError::Conflict(format!(
            "Your organization has reached its {limit}-monitor allowance"
        )));
    }
    let id = Uuid::new_v4().to_string();
    let result = sqlx::query("INSERT INTO uptime_monitors (id, site_id, organization_id, name, url, created_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(site_id, url) DO NOTHING")
        .bind(&id).bind(&site.id).bind(org).bind(name).bind(url).bind(Utc::now().timestamp()).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::Conflict(
            "This URL already has a monitor in this project".into(),
        ));
    }
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
pub(crate) async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_id, id)): Path<(String, String)>,
    Json(input): Json<UpdateMonitor>,
) -> Result<Json<Value>, ApiError> {
    let site = access(&state, &headers, &site_id, true).await?;
    let mut tx = state.sqlite.begin_with("BEGIN IMMEDIATE").await?;
    let monitor =
        sqlx::query_as::<_, Monitor>("SELECT * FROM uptime_monitors WHERE id = ? AND site_id = ?")
            .bind(&id)
            .bind(&site.id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::BadRequest("Monitor not found".into()))?;
    if (input.enabled == Some(true)) && allowance(()) == 0 {
        return Err(ApiError::Forbidden(
            "Uptime monitoring requires Pro or Business".into(),
        ));
    }
    let changed = input
        .enabled
        .is_some_and(|enabled| enabled != monitor.enabled);
    sqlx::query("UPDATE uptime_monitors SET enabled = COALESCE(?, enabled), state = CASE WHEN ? THEN 'unknown' ELSE state END, last_checked_at = CASE WHEN ? THEN NULL ELSE last_checked_at END, next_check_at = CASE WHEN ? THEN 0 ELSE next_check_at END, lease_token = CASE WHEN ? THEN NULL ELSE lease_token END, lease_until = CASE WHEN ? THEN NULL ELSE lease_until END WHERE id = ?")
        .bind(input.enabled).bind(changed).bind(changed).bind(changed).bind(changed).bind(changed).bind(&id).execute(&mut *tx).await?;
    if input.enabled == Some(false) {
        sqlx::query("UPDATE uptime_incidents SET ended_at = ?, resolution = 'paused' WHERE monitor_id = ? AND ended_at IS NULL")
            .bind(Utc::now().timestamp()).bind(&id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"status":"updated"})))
}
pub(crate) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_id, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let site = access(&state, &headers, &site_id, true).await?;
    let mut tx = state.sqlite.begin_with("BEGIN IMMEDIATE").await?;
    let result = sqlx::query("DELETE FROM uptime_monitors WHERE id = ? AND site_id = ?")
        .bind(&id)
        .bind(&site.id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest("Monitor not found".into()));
    }

    tx.commit().await?;
    Ok(Json(json!({"status":"deleted"})))
}

pub(crate) async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_id, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let site = access(&state, &headers, &site_id, false).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM uptime_monitors WHERE id = ? AND site_id = ?)",
    )
    .bind(&id)
    .bind(&site.id)
    .fetch_one(&state.sqlite)
    .await?;
    if !exists {
        return Err(ApiError::BadRequest("Monitor not found".into()));
    }
    let incidents = sqlx::query_as::<_, Incident>("SELECT * FROM uptime_incidents WHERE monitor_id = ? AND (ended_at IS NULL OR ended_at >= ?) ORDER BY started_at DESC LIMIT 100")
        .bind(&id).bind(Utc::now().timestamp() - HISTORY_DAYS * 86400).fetch_all(&state.sqlite).await?;
    let query = format!("SELECT checked_at, status_code, duration_ms, state, failure FROM owleye_uptime_checks WHERE site_id = {} AND monitor_id = {} AND checked_at >= toUnixTimestamp(now() - INTERVAL {HISTORY_DAYS} DAY) ORDER BY checked_at DESC LIMIT {HISTORY_CHECK_LIMIT}", clickhouse_string(&site.tracking_id), clickhouse_string(&id));
    // Operational state and incidents remain useful if history storage is down.
    let (checks, history_available) =
        match state.clickhouse.query_json_each_row::<Value>(&query).await {
            Ok(rows) => (rows, true),
            Err(_) => (vec![], false),
        };
    Ok(Json(
        json!({"incidents":incidents,"checks":checks,"history_available":history_available}),
    ))
}
pub(super) async fn eligible_ids(
    pool: &SqlitePool,
    org: &str,
    limit: i64,
) -> Result<Vec<String>, ApiError> {
    Ok(sqlx::query_scalar("SELECT m.id FROM uptime_monitors m JOIN sites s ON s.id = m.site_id JOIN organizations o ON o.id = m.organization_id WHERE m.organization_id = ? AND m.enabled = 1 AND s.archived_at IS NULL AND s.deleted_at IS NULL AND s.erasure_pending_at IS NULL AND o.archived_at IS NULL ORDER BY m.created_at, m.id LIMIT ?")
        .bind(org).bind(limit).fetch_all(pool).await?)
}

// Called only after ClickHouse erasure succeeds. The durable site erasure fence
// already prevents any in-flight worker from recording or sending new results.
pub(crate) async fn erase_site_configuration(
    pool: &SqlitePool,
    site_id: &str,
) -> Result<(), ApiError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("DELETE FROM uptime_monitors WHERE site_id = ?")
        .bind(site_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
