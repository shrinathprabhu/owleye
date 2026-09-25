use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use data_encoding::BASE64URL_NOPAD;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    ai, auth, demo,
    models::clickhouse_string,
    retention::ACTIVE_ROW_PREDICATE,
    sites::{self, ActiveSite},
    ApiError, AppState,
};

const API_KEY_HEADER: &str = "x-owleye-api-key";
const STATS_READ_SCOPE: &str = "stats:read";
const EVENTS_READ_SCOPE: &str = "events:read";
const AI_PROMPT_SCOPE: &str = "ai:prompt";
const MAX_KEY_NAME_BYTES: usize = 120;
const DEFAULT_EVENT_DAYS: u16 = 7;
const MAX_EVENT_DAYS: u16 = 90;
const DEFAULT_EVENT_LIMIT: u16 = 500;
const MAX_EVENT_LIMIT: u16 = 5_000;
const DEFAULT_EXPORT_LIMIT: u16 = 5_000;
const MAX_EXPORT_LIMIT: u16 = 10_000;

#[derive(Debug, Serialize)]
pub(crate) struct DeveloperSettingsResponse {
    allowed_scopes: Vec<&'static str>,
    api_access_enabled: bool,
    developer_mode: bool,
    endpoints: DeveloperEndpoints,
    site_id: String,
}

#[derive(Debug, Serialize)]
struct DeveloperEndpoints {
    control_plane_backup: String,
    events: String,
    events_export: String,
    stats: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateDeveloperSettingsRequest {
    #[serde(default)]
    api_access_enabled: Option<bool>,
    #[serde(default)]
    developer_mode: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DeveloperKeySummary {
    created_at: String,
    expires_at: Option<String>,
    id: String,
    key_prefix: String,
    last_used_at: Option<String>,
    name: String,
    revoked_at: Option<String>,
    scopes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreatedDeveloperKeyResponse {
    api_key: DeveloperKeySummary,
    secret: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateDeveloperKeyRequest {
    lifetime: String,
    name: String,
    scopes: Vec<String>,
}

#[derive(Debug, FromRow)]
struct DeveloperKeyRow {
    created_at: String,
    expires_at: Option<String>,
    id: String,
    key_prefix: String,
    last_used_at: Option<String>,
    name: String,
    revoked_at: Option<String>,
    scopes_json: String,
}

impl DeveloperKeyRow {
    fn into_summary(self) -> Result<DeveloperKeySummary, ApiError> {
        Ok(DeveloperKeySummary {
            created_at: self.created_at,
            expires_at: self.expires_at,
            id: self.id,
            key_prefix: self.key_prefix,
            last_used_at: self.last_used_at,
            name: self.name,
            revoked_at: self.revoked_at,
            scopes: serde_json::from_str(&self.scopes_json)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct AuthorizedKeyRow {
    created_by_user_id: String,
    expires_at: Option<String>,
    id: String,
    scopes_json: String,
}

#[derive(Debug)]
struct AuthorizedDeveloper {
    owner_user_id: String,
    site: ActiveSite,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeveloperEventsQuery {
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    before_event_id: Option<String>,
    #[serde(default)]
    days: Option<u16>,
    #[serde(default)]
    limit: Option<u16>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeveloperEvent {
    browser: String,
    country: String,
    device: String,
    event_id: String,
    event_name: String,
    event_type: String,
    occurred_at: String,
    operating_system: String,
    path: String,
    referrer_host: String,
    region: String,
    rule_id: String,
    utm_campaign: String,
    utm_medium: String,
    utm_source: String,
    visitor_id: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DeveloperEventsResponse {
    data: Vec<DeveloperEvent>,
    pagination: DeveloperEventPagination,
    site_id: String,
}

#[derive(Debug, Serialize)]
struct DeveloperEventPagination {
    days: u16,
    limit: u16,
    next_before: Option<String>,
    next_event_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeveloperPromptRequest {
    prompt: String,
    #[serde(default)]
    request_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventExportQuery {
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    before_event_id: Option<String>,
    #[serde(default)]
    days: Option<u16>,
    #[serde(default)]
    limit: Option<u16>,
}

pub(crate) async fn get_developer_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Json<DeveloperSettingsResponse>, ApiError> {
    let (_, site) = owner_site(&state, &headers, &site_identifier).await?;
    Ok(Json(load_developer_settings(&state.sqlite, &site).await?))
}

pub(crate) async fn update_developer_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<UpdateDeveloperSettingsRequest>,
) -> Result<Json<DeveloperSettingsResponse>, ApiError> {
    let (user_id, site) = owner_site(&state, &headers, &site_identifier).await?;
    auth::reject_demo_workspace_mutation(&user_id)?;
    if request.developer_mode.is_none() && request.api_access_enabled.is_none() {
        return Err(ApiError::BadRequest(
            "At least one developer setting must be provided".to_owned(),
        ));
    }

    let current = developer_toggles(&state.sqlite, &site.id).await?;
    let developer_mode = request.developer_mode.unwrap_or(current.0);
    let api_access_enabled = if developer_mode {
        request.api_access_enabled.unwrap_or(current.1)
    } else {
        false
    };
    if request.api_access_enabled == Some(true) && !developer_mode {
        return Err(ApiError::BadRequest(
            "Developer mode must be enabled before API access".to_owned(),
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO site_settings (site_id, developer_mode, api_access_enabled)
        VALUES (?, ?, ?)
        ON CONFLICT(site_id) DO UPDATE SET
            developer_mode = excluded.developer_mode,
            api_access_enabled = excluded.api_access_enabled
        "#,
    )
    .bind(&site.id)
    .bind(developer_mode)
    .bind(api_access_enabled)
    .execute(&state.sqlite)
    .await?;

    Ok(Json(load_developer_settings(&state.sqlite, &site).await?))
}

pub(crate) async fn list_developer_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Json<Vec<DeveloperKeySummary>>, ApiError> {
    let (user_id, site) = owner_site(&state, &headers, &site_identifier).await?;
    if user_id == demo::DEMO_USER_ID {
        return Ok(Json(Vec::new()));
    }
    Ok(Json(load_key_summaries(&state.sqlite, &site.id).await?))
}

pub(crate) async fn create_developer_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<CreateDeveloperKeyRequest>,
) -> Result<Response, ApiError> {
    let (user_id, site) = owner_site(&state, &headers, &site_identifier).await?;
    auth::reject_demo_workspace_mutation(&user_id)?;
    let toggles = developer_toggles(&state.sqlite, &site.id).await?;

    if !toggles.0 || !toggles.1 {
        return Err(ApiError::Forbidden(
            "Developer mode and API access must both be enabled".to_owned(),
        ));
    }
    let name = normalize_key_name(&request.name)?;
    let scopes = normalize_scopes(&request.scopes)?;

    let expires_at = expiry_for_lifetime(&request.lifetime)?;
    let created = create_key_record(
        &state.sqlite,
        &state.settings.hash_salt,
        &site.id,
        &user_id,
        &name,
        &scopes,
        expires_at.as_deref(),
    )
    .await?;

    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(created),
    )
        .into_response())
}

pub(crate) async fn revoke_developer_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, key_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let (user_id, site) = owner_site(&state, &headers, &site_identifier).await?;
    auth::reject_demo_workspace_mutation(&user_id)?;
    revoke_key_record(&state.sqlite, &site.id, &key_id).await?;
    Ok(Json(json!({ "status": "revoked" })))
}

pub(crate) async fn developer_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Query(query): Query<DeveloperEventsQuery>,
) -> Result<Response, ApiError> {
    let authorized = authorize_key(&state, &headers, &site_identifier, EVENTS_READ_SCOPE).await?;
    let days = query
        .days
        .unwrap_or(DEFAULT_EVENT_DAYS)
        .clamp(1, MAX_EVENT_DAYS);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_EVENT_LIMIT)
        .clamp(1, MAX_EVENT_LIMIT);
    let before = query.before.as_deref().map(normalize_cursor).transpose()?;
    let before_event_id = query
        .before_event_id
        .as_deref()
        .map(normalize_event_id)
        .transpose()?;
    if before_event_id.is_some() && before.is_none() {
        return Err(ApiError::BadRequest(
            "before_event_id requires before".to_owned(),
        ));
    }
    let sql = developer_events_query(
        &authorized.site.tracking_id,
        days,
        u32::from(limit),
        before.as_deref(),
        before_event_id.as_deref(),
    );
    let data = state
        .clickhouse
        .query_json_each_row::<DeveloperEvent>(&sql)
        .await?;
    let next_before = (data.len() == usize::from(limit))
        .then(|| data.last().map(|event| event.occurred_at.clone()))
        .flatten();
    let next_event_id = (data.len() == usize::from(limit))
        .then(|| data.last().map(|event| event.event_id.clone()))
        .flatten();

    Ok(no_store(Json(DeveloperEventsResponse {
        data,
        pagination: DeveloperEventPagination {
            days,
            limit,
            next_before,
            next_event_id,
        },
        site_id: authorized.site.tracking_id,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeveloperStatsQuery {
    days: Option<u16>,
}

pub(crate) async fn developer_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site): Path<String>,
    Query(query): Query<DeveloperStatsQuery>,
) -> Result<Response, ApiError> {
    let authorized = authorize_key(&state, &headers, &site, STATS_READ_SCOPE).await?;
    let days = query.days.unwrap_or(7);
    if !(1..=90).contains(&days) {
        return Err(ApiError::BadRequest(
            "days must be between 1 and 90".to_owned(),
        ));
    }
    Ok(no_store(Json(
        crate::stats::live_overview(&state.clickhouse, authorized.site.tracking_id, days).await?,
    )))
}

pub(crate) async fn openapi() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (
                header::CACHE_CONTROL,
                "public, max-age=300, must-revalidate",
            ),
        ],
        include_str!("../openapi.json"),
    )
}

pub(crate) async fn developer_prompt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<DeveloperPromptRequest>,
) -> Result<Response, ApiError> {
    let authorized = authorize_key(&state, &headers, &site_identifier, AI_PROMPT_SCOPE).await?;
    let response = ai::consume_developer_prompt(
        &state,
        &authorized.site.id,
        &authorized.owner_user_id,
        &request.prompt,
        request.request_id,
    )
    .await?;
    Ok(no_store(response))
}

pub(crate) async fn download_control_plane_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Response, ApiError> {
    let (user_id, site) = owner_site(&state, &headers, &site_identifier).await?;
    auth::reject_demo_workspace_mutation(&user_id)?;
    reject_cloud_free_export(&state, &site).await?;
    if !developer_toggles(&state.sqlite, &site.id).await?.0 {
        return Err(ApiError::Forbidden(
            "Enable developer mode before creating a backup".to_owned(),
        ));
    }
    let backup = build_control_plane_backup(&state.sqlite, &site.id).await?;
    let body = serde_json::to_vec_pretty(&backup)?;
    let file_name = format!("owleye-{}-control-plane.json", site.tracking_id);
    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{file_name}\""))
            .map_err(|error| ApiError::Internal(error.into()))?,
    );
    Ok((response_headers, body).into_response())
}

pub(crate) async fn download_event_export(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Query(query): Query<EventExportQuery>,
) -> Result<Response, ApiError> {
    let (user_id, site) = owner_site(&state, &headers, &site_identifier).await?;
    auth::reject_demo_workspace_mutation(&user_id)?;
    let max_days = owner_export_max_days(&state, &site).await?;
    if !developer_toggles(&state.sqlite, &site.id).await?.0 {
        return Err(ApiError::Forbidden(
            "Enable developer mode before exporting analytics".to_owned(),
        ));
    }
    let days = query.days.unwrap_or(max_days).clamp(1, max_days);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_EXPORT_LIMIT)
        .clamp(1, MAX_EXPORT_LIMIT);
    let before = query.before.as_deref().map(normalize_cursor).transpose()?;
    let before_event_id = query
        .before_event_id
        .as_deref()
        .map(normalize_event_id)
        .transpose()?;
    if before_event_id.is_some() && before.is_none() {
        return Err(ApiError::BadRequest(
            "before_event_id requires before".to_owned(),
        ));
    }
    let sql = developer_events_query(
        &site.tracking_id,
        days,
        u32::from(limit),
        before.as_deref(),
        before_event_id.as_deref(),
    );
    let events = state
        .clickhouse
        .query_json_each_row::<DeveloperEvent>(&sql)
        .await?;
    let estimated_capacity = events.len().saturating_mul(512).min(8 * 1024 * 1024);
    let mut body = Vec::with_capacity(estimated_capacity);
    for event in &events {
        serde_json::to_writer(&mut body, event)?;
        body.push(b'\n');
    }

    let file_name = format!("owleye-{}-events-{}d.ndjson", site.tracking_id, days);
    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
    );
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{file_name}\""))
            .map_err(|error| ApiError::Internal(error.into()))?,
    );
    response_headers.insert(
        "x-owleye-row-count",
        HeaderValue::from_str(&events.len().to_string())
            .map_err(|error| ApiError::Internal(error.into()))?,
    );
    if events.len() == usize::from(limit) {
        if let Some(last) = events.last() {
            response_headers.insert(
                "x-owleye-next-before",
                HeaderValue::from_str(&last.occurred_at)
                    .map_err(|error| ApiError::Internal(error.into()))?,
            );
            response_headers.insert(
                "x-owleye-next-event-id",
                HeaderValue::from_str(&last.event_id)
                    .map_err(|error| ApiError::Internal(error.into()))?,
            );
        }
    }
    Ok((response_headers, body).into_response())
}

async fn reject_cloud_free_export(_state: &AppState, _site: &ActiveSite) -> Result<(), ApiError> {
    Ok(())
}

async fn owner_export_max_days(_state: &AppState, _site: &ActiveSite) -> Result<u16, ApiError> {
    Ok(u16::MAX)
}

async fn owner_site(
    state: &AppState,
    headers: &HeaderMap,
    identifier: &str,
) -> Result<(String, ActiveSite), ApiError> {
    let session = auth::authenticated_session(state, headers).await?;
    if auth::is_demo_session(&session) {
        let site = demo::demo_site(identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        if !sites::demo_site_role(site).is_owner() {
            return Err(ApiError::Forbidden(
                "Site owner access is required".to_owned(),
            ));
        }
        return Ok((
            session.id,
            ActiveSite {
                created_by_user_id: Some(demo::DEMO_USER_ID.to_owned()),
                id: site.id.to_owned(),
                organization_id: None,
                team_id: None,
                tracking_id: site.tracking_id.to_owned(),
            },
        ));
    }
    let site = sites::resolve_site_for_owner(&state.sqlite, &session.id, identifier).await?;
    Ok((session.id, site))
}

async fn load_developer_settings(
    pool: &SqlitePool,
    site: &ActiveSite,
) -> Result<DeveloperSettingsResponse, ApiError> {
    let (developer_mode, api_access_enabled) = developer_toggles(pool, &site.id).await?;
    let mut allowed_scopes = Vec::new();
    for scope in [EVENTS_READ_SCOPE, STATS_READ_SCOPE, AI_PROMPT_SCOPE] {
        allowed_scopes.push(scope);
    }
    Ok(DeveloperSettingsResponse {
        allowed_scopes,
        api_access_enabled,
        developer_mode,
        endpoints: DeveloperEndpoints {
            control_plane_backup: format!("/v1/sites/{}/developer/backup", site.tracking_id),
            events: format!("/v1/developer/sites/{}/events", site.tracking_id),
            events_export: format!("/v1/sites/{}/developer/events-export", site.tracking_id),
            stats: format!("/v1/developer/sites/{}/stats", site.tracking_id),
            prompt: format!("/v1/developer/sites/{}/prompt", site.tracking_id),
        },
        site_id: site.tracking_id.clone(),
    })
}

async fn developer_toggles(pool: &SqlitePool, site_id: &str) -> Result<(bool, bool), ApiError> {
    Ok(sqlx::query_as::<_, (bool, bool)>(
        r#"
        SELECT COALESCE(developer_mode, 0), COALESCE(api_access_enabled, 0)
        FROM site_settings
        WHERE site_id = ?
        LIMIT 1
        "#,
    )
    .bind(site_id)
    .fetch_optional(pool)
    .await?
    .unwrap_or((false, false)))
}

async fn authorize_key(
    state: &AppState,
    headers: &HeaderMap,
    site_identifier: &str,
    required_scope: &str,
) -> Result<AuthorizedDeveloper, ApiError> {
    let secret = read_secret(headers)
        .ok_or_else(|| ApiError::Unauthorized("Developer API key is required".to_owned()))?;
    let site = sites::resolve_active_site(&state.sqlite, site_identifier).await?;
    let (developer_mode, api_access_enabled) = developer_toggles(&state.sqlite, &site.id).await?;
    if !developer_mode || !api_access_enabled {
        return Err(ApiError::Unauthorized(
            "Developer API access is disabled".to_owned(),
        ));
    }
    let key_hash = auth::hash_secret(&state.settings.hash_salt, &["developer-api-key", &secret]);
    let record = load_authorized_key(&state.sqlite, &site.id, &key_hash)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("Invalid developer API key".to_owned()))?;
    validate_authorized_record(&record, required_scope, Utc::now())?;
    if !sites::effective_site_role(&state.sqlite, &record.created_by_user_id, &site)
        .await?
        .is_owner()
    {
        return Err(ApiError::Forbidden(
            "The key owner no longer owns this site".to_owned(),
        ));
    }
    let request_limit = 600;

    state
        .rate_limiter
        .check(
            crate::rate_limit::RateLimitPolicy::developer_organization(request_limit),
            format!(
                "org:{}",
                site.organization_id.as_deref().unwrap_or(&site.id)
            ),
        )
        .await?;
    sqlx::query("UPDATE developer_api_keys SET last_used_at = ? WHERE id = ? AND (last_used_at IS NULL OR julianday(last_used_at) < julianday('now', '-1 minute'))")
        .bind(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true))
        .bind(&record.id)
        .execute(&state.sqlite)
        .await?;

    Ok(AuthorizedDeveloper {
        owner_user_id: record.created_by_user_id,
        site,
    })
}

async fn load_authorized_key(
    pool: &SqlitePool,
    site_id: &str,
    key_hash: &str,
) -> Result<Option<AuthorizedKeyRow>, ApiError> {
    Ok(sqlx::query_as::<_, AuthorizedKeyRow>(
        r#"
        SELECT id, created_by_user_id, scopes_json, expires_at
        FROM developer_api_keys
        WHERE site_id = ? AND key_hash = ? AND revoked_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(site_id)
    .bind(key_hash)
    .fetch_optional(pool)
    .await?)
}

fn validate_authorized_record(
    record: &AuthorizedKeyRow,
    required_scope: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if record
        .expires_at
        .as_deref()
        .is_some_and(|expiry| !expiry_is_after(expiry, now))
    {
        return Err(ApiError::Unauthorized(
            "Developer API key has expired".to_owned(),
        ));
    }
    let scopes: Vec<String> = serde_json::from_str(&record.scopes_json)?;
    if !scopes.iter().any(|scope| scope == required_scope) {
        return Err(ApiError::Forbidden(format!(
            "Developer API key is missing the {required_scope} scope"
        )));
    }
    Ok(())
}

fn read_secret(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(API_KEY_HEADER) {
        return value
            .to_str()
            .ok()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
    }
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn generate_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("owl_dev_{}", BASE64URL_NOPAD.encode(&bytes))
}

async fn create_key_record(
    pool: &SqlitePool,
    hash_salt: &str,
    site_id: &str,
    user_id: &str,
    name: &str,
    scopes: &[String],
    expires_at: Option<&str>,
) -> Result<CreatedDeveloperKeyResponse, ApiError> {
    let secret = generate_secret();
    let key_prefix = secret.chars().take(20).collect::<String>();
    let key_hash = auth::hash_secret(hash_salt, &["developer-api-key", &secret]);
    let id = Uuid::new_v4().to_string();
    let scopes_json = serde_json::to_string(scopes)?;
    sqlx::query(
        r#"
        INSERT INTO developer_api_keys (
            id, site_id, created_by_user_id, name, key_prefix, key_hash,
            scopes_json, expires_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(site_id)
    .bind(user_id)
    .bind(name)
    .bind(key_prefix)
    .bind(key_hash)
    .bind(scopes_json)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(CreatedDeveloperKeyResponse {
        api_key: load_key_summary(pool, site_id, &id).await?,
        secret,
    })
}

async fn load_key_summaries(
    pool: &SqlitePool,
    site_id: &str,
) -> Result<Vec<DeveloperKeySummary>, ApiError> {
    let rows = sqlx::query_as::<_, DeveloperKeyRow>(
        r#"
        SELECT id, name, key_prefix, scopes_json, last_used_at, expires_at,
            revoked_at, created_at
        FROM developer_api_keys
        WHERE site_id = ?
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(DeveloperKeyRow::into_summary)
        .collect()
}

async fn load_key_summary(
    pool: &SqlitePool,
    site_id: &str,
    key_id: &str,
) -> Result<DeveloperKeySummary, ApiError> {
    sqlx::query_as::<_, DeveloperKeyRow>(
        r#"
        SELECT id, name, key_prefix, scopes_json, last_used_at, expires_at,
            revoked_at, created_at
        FROM developer_api_keys
        WHERE site_id = ? AND id = ?
        LIMIT 1
        "#,
    )
    .bind(site_id)
    .bind(key_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::BadRequest("Unknown developer API key".to_owned()))?
    .into_summary()
}

async fn revoke_key_record(pool: &SqlitePool, site_id: &str, key_id: &str) -> Result<(), ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE developer_api_keys
        SET revoked_at = COALESCE(revoked_at, ?)
        WHERE site_id = ? AND id = ?
        "#,
    )
    .bind(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true))
    .bind(site_id)
    .bind(key_id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest("Unknown developer API key".to_owned()));
    }
    Ok(())
}

fn normalize_key_name(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_KEY_NAME_BYTES || value.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "Key name must be between 1 and 120 characters".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

fn normalize_scopes(values: &[String]) -> Result<Vec<String>, ApiError> {
    if values.is_empty() {
        return Err(ApiError::BadRequest(
            "Select at least one developer API scope".to_owned(),
        ));
    }
    let mut scopes = values
        .iter()
        .map(|scope| scope.trim().to_owned())
        .collect::<Vec<_>>();
    scopes.sort();
    scopes.dedup();
    if scopes.iter().any(|scope| {
        !matches!(
            scope.as_str(),
            EVENTS_READ_SCOPE | STATS_READ_SCOPE | AI_PROMPT_SCOPE
        )
    }) {
        return Err(ApiError::BadRequest(
            "Developer keys only support events:read, stats:read and ai:prompt".to_owned(),
        ));
    }
    Ok(scopes)
}

fn expiry_for_lifetime(value: &str) -> Result<Option<String>, ApiError> {
    let duration = match value {
        "1h" => Some(Duration::hours(1)),
        "24h" => Some(Duration::hours(24)),
        "7d" => Some(Duration::days(7)),
        "permanent" => None,
        _ => {
            return Err(ApiError::BadRequest(
                "lifetime must be 1h, 24h, 7d, or permanent".to_owned(),
            ))
        }
    };
    Ok(duration
        .map(|duration| (Utc::now() + duration).to_rfc3339_opts(SecondsFormat::Millis, true)))
}

fn expiry_is_after(value: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(value)
        .map(|expiry| expiry.with_timezone(&Utc) > now)
        .unwrap_or(false)
}

fn normalize_cursor(value: &str) -> Result<String, ApiError> {
    let parsed = DateTime::parse_from_rfc3339(value.trim())
        .map_err(|_| ApiError::BadRequest("before must be an RFC3339 timestamp".to_owned()))?
        .with_timezone(&Utc);
    Ok(parsed.to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn normalize_event_id(value: &str) -> Result<String, ApiError> {
    Uuid::parse_str(value.trim())
        .map(|value| value.to_string())
        .map_err(|_| ApiError::BadRequest("before_event_id must be a UUID".to_owned()))
}

fn developer_events_query(
    tracking_id: &str,
    days: u16,
    limit: u32,
    before: Option<&str>,
    before_event_id: Option<&str>,
) -> String {
    let mut filters = vec![
        format!("site_id = {}", clickhouse_string(tracking_id)),
        format!("occurred_at >= now() - INTERVAL {days} DAY"),
        ACTIVE_ROW_PREDICATE.to_owned(),
    ];
    if let Some(before) = before {
        let before = clickhouse_string(before);
        if let Some(event_id) = before_event_id {
            filters.push(format!(
                "(occurred_at < parseDateTime64BestEffort({before}) OR (occurred_at = parseDateTime64BestEffort({before}) AND event_id < toUUID({})))",
                clickhouse_string(event_id)
            ));
        } else {
            filters.push(format!("occurred_at < parseDateTime64BestEffort({before})"));
        }
    }
    format!(
        r#"
        SELECT
            toString(event_id) AS event_id,
            formatDateTime(occurred_at, '%Y-%m-%dT%H:%i:%S.%fZ', 'UTC') AS occurred_at,
            leftUTF8(event_type, 64) AS event_type,
            leftUTF8(event_name, 128) AS event_name,
            leftUTF8(visitor_id, 128) AS visitor_id,
            leftUTF8(url_path, 1024) AS path,
            leftUTF8(referrer_host, 255) AS referrer_host,
            leftUTF8(utm_source, 256) AS utm_source,
            leftUTF8(utm_medium, 256) AS utm_medium,
            leftUTF8(utm_campaign, 256) AS utm_campaign,
            leftUTF8(country, 128) AS country,
            leftUTF8(region, 128) AS region,
            leftUTF8(browser_name, 64) AS browser,
            leftUTF8(os_name, 64) AS operating_system,
            leftUTF8(device_type, 64) AS device,
            leftUTF8(rule_id, 128) AS rule_id
        FROM (
            SELECT * FROM owleye_events
            WHERE {filters}
            ORDER BY occurred_at DESC, event_id DESC
            LIMIT {limit}
        )
        "#,
        filters = filters.join(" AND ")
    )
}

#[derive(Debug, FromRow, Serialize)]
struct BackupSite {
    created_at: String,
    domain: String,
    id: String,
    name: String,
    timezone: String,
    tracking_id: String,
    updated_at: String,
}

#[derive(Debug, FromRow, Serialize)]
struct BackupDomain {
    created_at: String,
    domain: String,
    id: String,
    is_primary: bool,
}

#[derive(Debug, FromRow, Serialize)]
struct BackupRule {
    capture_text: bool,
    created_at: String,
    description: Option<String>,
    enabled: bool,
    id: String,
    metadata: Value,
    name: String,
    sample_rate: f64,
    selector: String,
    trigger_config: Value,
    r#type: String,
    updated_at: String,
}

#[derive(Debug, FromRow)]
struct BackupRuleRow {
    capture_text: bool,
    created_at: String,
    description: Option<String>,
    enabled: bool,
    id: String,
    metadata_json: String,
    name: String,
    sample_rate: f64,
    selector: String,
    trigger_config_json: String,
    r#type: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct BackupDashboard {
    charts: Vec<BackupChart>,
    created_at: i64,
    id: String,
    is_default: bool,
    layout: Option<String>,
    name: String,
    updated_at: i64,
}

#[derive(Debug, FromRow)]
struct BackupDashboardRow {
    created_at: i64,
    id: String,
    is_default: bool,
    layout: Option<String>,
    name: String,
    updated_at: i64,
}

#[derive(Debug, FromRow, Serialize)]
struct BackupChart {
    chart_type: String,
    display_config: Option<String>,
    height: i64,
    id: String,
    position_x: i64,
    position_y: i64,
    query_config: String,
    title: String,
    width: i64,
}

async fn build_control_plane_backup(pool: &SqlitePool, site_id: &str) -> Result<Value, ApiError> {
    let site = sqlx::query_as::<_, BackupSite>(
        r#"
        SELECT id, tracking_id, name, domain,
            COALESCE(default_timezone, timezone, 'UTC') AS timezone,
            created_at, updated_at
        FROM sites WHERE id = ? LIMIT 1
        "#,
    )
    .bind(site_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::BadRequest("Unknown site".to_owned()))?;
    let domains = sqlx::query_as::<_, BackupDomain>(
        "SELECT id, domain, is_primary, created_at FROM site_domains WHERE site_id = ? ORDER BY is_primary DESC, created_at ASC",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;
    let rule_rows = sqlx::query_as::<_, BackupRuleRow>(
        r#"
        SELECT id, name, description, selector, type, capture_text, enabled,
            sample_rate, trigger_config_json, metadata_json, created_at, updated_at
        FROM tracking_rules WHERE site_id = ? ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;
    let rules = rule_rows
        .into_iter()
        .map(|row| {
            Ok(BackupRule {
                capture_text: row.capture_text,
                created_at: row.created_at,
                description: row.description,
                enabled: row.enabled,
                id: row.id,
                metadata: serde_json::from_str(&row.metadata_json)?,
                name: row.name,
                sample_rate: row.sample_rate,
                selector: row.selector,
                trigger_config: serde_json::from_str(&row.trigger_config_json)?,
                r#type: row.r#type,
                updated_at: row.updated_at,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let dashboard_rows = sqlx::query_as::<_, BackupDashboardRow>(
        "SELECT id, name, layout, is_default, created_at, updated_at FROM dashboards WHERE site_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;
    let mut dashboards = Vec::with_capacity(dashboard_rows.len());
    for dashboard in dashboard_rows {
        let charts = sqlx::query_as::<_, BackupChart>(
            r#"
            SELECT id, chart_type, title, query_config, display_config,
                position_x, position_y, width, height
            FROM dashboard_charts WHERE dashboard_id = ?
            ORDER BY position_y ASC, position_x ASC, id ASC
            "#,
        )
        .bind(&dashboard.id)
        .fetch_all(pool)
        .await?;
        dashboards.push(BackupDashboard {
            charts,
            created_at: dashboard.created_at,
            id: dashboard.id,
            is_default: dashboard.is_default,
            layout: dashboard.layout,
            name: dashboard.name,
            updated_at: dashboard.updated_at,
        });
    }
    let settings = sqlx::query_as::<_, (bool, bool, bool, bool)>(
        r#"
        SELECT tracking_paused, ai_enabled, developer_mode, api_access_enabled
        FROM site_settings WHERE site_id = ? LIMIT 1
        "#,
    )
    .bind(site_id)
    .fetch_optional(pool)
    .await?;
    let settings = settings.map(|row| {
        json!({
            "tracking_paused": row.0,
            "ai_enabled": row.1,
            "developer_mode": row.2,
            "api_access_enabled": row.3,
        })
    });

    Ok(json!({
        "format": "owleye-control-plane-backup",
        "format_version": 1,
        "generated_at": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        "site": site,
        "domains": domains,
        "settings": settings,
        "rules": rules,
        "dashboards": dashboards,
    }))
}

fn no_store(response: impl IntoResponse) -> Response {
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        response,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use crate::storage::sqlite;

    use super::*;

    #[test]
    fn developer_scopes_are_exact_and_never_include_ingestion() {
        assert_eq!(
            normalize_scopes(&[EVENTS_READ_SCOPE.to_owned(), AI_PROMPT_SCOPE.to_owned()]).unwrap(),
            vec![AI_PROMPT_SCOPE.to_owned(), EVENTS_READ_SCOPE.to_owned()]
        );
        assert!(normalize_scopes(&["events:write".to_owned()]).is_err());
        assert!(normalize_scopes(&["sites:write".to_owned()]).is_err());
        assert!(normalize_scopes(&[]).is_err());
    }

    #[test]
    fn developer_hash_has_a_separate_domain_from_ingestion_keys() {
        let secret = "owl_dev_example";
        let developer = auth::hash_secret("salt", &["developer-api-key", secret]);
        let ingest = auth::hash_secret("salt", &["api-key", secret]);
        assert_ne!(developer, ingest);
    }

    #[tokio::test]
    async fn keys_are_hashed_site_confined_expirable_revocable_and_secret_once() {
        let pool = test_pool().await;
        seed_owner_and_sites(&pool).await;
        let scopes = vec![EVENTS_READ_SCOPE.to_owned()];
        let expiry = (Utc::now() + Duration::hours(1)).to_rfc3339_opts(SecondsFormat::Millis, true);
        let created = create_key_record(
            &pool,
            "salt",
            "site-a",
            "owner",
            "LLM reader",
            &scopes,
            Some(&expiry),
        )
        .await
        .unwrap();
        assert!(created.secret.starts_with("owl_dev_"));
        assert_eq!(created.api_key.scopes, scopes);
        let serialized_summary = serde_json::to_string(&created.api_key).unwrap();
        assert!(!serialized_summary.contains(&created.secret));
        assert!(!serialized_summary.contains("key_hash"));
        let stored_hash: String =
            sqlx::query_scalar("SELECT key_hash FROM developer_api_keys WHERE id = ?")
                .bind(&created.api_key.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            stored_hash,
            auth::hash_secret("salt", &["developer-api-key", &created.secret])
        );
        assert_ne!(stored_hash, created.secret);
        let authorized = load_authorized_key(&pool, "site-a", &stored_hash)
            .await
            .unwrap()
            .expect("the matching site can resolve its key");
        assert!(load_authorized_key(&pool, "site-b", &stored_hash)
            .await
            .unwrap()
            .is_none());
        assert!(validate_authorized_record(&authorized, EVENTS_READ_SCOPE, Utc::now()).is_ok());
        assert!(matches!(
            validate_authorized_record(&authorized, AI_PROMPT_SCOPE, Utc::now()),
            Err(ApiError::Forbidden(_))
        ));
        assert!(load_key_summaries(&pool, "site-b")
            .await
            .unwrap()
            .is_empty());
        assert!(expiry_is_after(&expiry, Utc::now()));

        sqlx::query("UPDATE developer_api_keys SET expires_at = ? WHERE id = ?")
            .bind((Utc::now() - Duration::minutes(1)).to_rfc3339())
            .bind(&created.api_key.id)
            .execute(&pool)
            .await
            .unwrap();
        let expired = load_authorized_key(&pool, "site-a", &stored_hash)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            validate_authorized_record(&expired, EVENTS_READ_SCOPE, Utc::now()),
            Err(ApiError::Unauthorized(_))
        ));

        revoke_key_record(&pool, "site-b", &created.api_key.id)
            .await
            .unwrap_err();
        revoke_key_record(&pool, "site-a", &created.api_key.id)
            .await
            .unwrap();
        let revoked: Option<String> =
            sqlx::query_scalar("SELECT revoked_at FROM developer_api_keys WHERE id = ?")
                .bind(&created.api_key.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(revoked.is_some());
    }

    #[tokio::test]
    async fn toggles_default_off_and_backup_excludes_credentials_and_sessions() {
        let pool = test_pool().await;
        seed_owner_and_sites(&pool).await;
        assert_eq!(
            developer_toggles(&pool, "site-a").await.unwrap(),
            (false, false)
        );
        sqlx::query(
            "INSERT INTO site_settings (site_id, developer_mode, api_access_enabled) VALUES ('site-a', 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            developer_toggles(&pool, "site-a").await.unwrap(),
            (true, true)
        );
        sqlx::query("UPDATE site_settings SET api_access_enabled = 0 WHERE site_id = 'site-a'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            developer_toggles(&pool, "site-a").await.unwrap(),
            (true, false)
        );
        let backup = build_control_plane_backup(&pool, "site-a").await.unwrap();
        let serialized = serde_json::to_string(&backup).unwrap();
        for excluded in [
            "key_hash",
            "secret_key_hash",
            "totp_secret",
            "auth_sessions",
            "developer_api_keys",
            "api_keys",
        ] {
            assert!(!serialized.contains(excluded), "backup leaked {excluded}");
        }
        assert!(serialized.contains("owleye-control-plane-backup"));
        assert!(serialized.contains("tracking-a"));
    }

    async fn seed_owner_and_sites(pool: &SqlitePool) {
        sqlx::query("INSERT INTO users (id, email) VALUES ('owner', 'owner@example.com')")
            .execute(pool)
            .await
            .unwrap();
        for (id, tracking) in [("site-a", "tracking-a"), ("site-b", "tracking-b")] {
            sqlx::query(
                "INSERT INTO sites (id, tracking_id, name, domain, created_by_user_id) VALUES (?, ?, ?, ?, 'owner')",
            )
            .bind(id)
            .bind(tracking)
            .bind(id)
            .bind(format!("{id}.example.com"))
            .execute(pool)
            .await
            .unwrap();
        }
    }

    async fn test_pool() -> SqlitePool {
        let db_path =
            std::env::temp_dir().join(format!("owleye-developer-access-{}.sqlite", Uuid::new_v4()));
        sqlite::connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap()
    }
}
