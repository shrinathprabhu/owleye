use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderMap},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, SecondsFormat, Utc};
use data_encoding::BASE64URL_NOPAD;
use rand::{rngs::OsRng, RngCore};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    auth,
    sites::{self, ActiveSite},
    ApiError, AppState,
};

const API_KEY_HEADER: &str = "x-owleye-api-key";
const EVENTS_WRITE_SCOPE: &str = "events:write";
const MAX_KEY_NAME_BYTES: usize = 120;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ApiKeySummary {
    id: String,
    name: String,
    key_prefix: String,
    scopes: Vec<String>,
    last_used_at: Option<String>,
    expires_at: Option<String>,
    revoked_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreatedApiKeyResponse {
    api_key: ApiKeySummary,
    secret: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateApiKeyRequest {
    name: String,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    scopes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateApiKeyRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    expires_at: ExpiryUpdate,
}

#[derive(Debug, Default)]
enum ExpiryUpdate {
    #[default]
    Missing,
    Clear,
    Set(String),
}

impl<'de> Deserialize<'de> for ExpiryUpdate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Null => Ok(Self::Clear),
            Value::String(value) => Ok(Self::Set(value)),
            _ => Err(D::Error::custom(
                "expires_at must be an RFC3339 string or null",
            )),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ApiKeySummaryRow {
    id: String,
    name: String,
    key_prefix: String,
    scopes_json: String,
    legacy_scopes: Option<String>,
    last_used_at: Option<String>,
    expires_at: Option<String>,
    revoked_at: Option<String>,
    created_at: String,
}

impl ApiKeySummaryRow {
    fn into_summary(self) -> ApiKeySummary {
        Self::summary(
            self.id,
            self.name,
            self.key_prefix,
            &self.scopes_json,
            self.legacy_scopes.as_deref(),
            self.last_used_at,
            self.expires_at,
            self.revoked_at,
            self.created_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn summary(
        id: String,
        name: String,
        key_prefix: String,
        scopes_json: &str,
        legacy_scopes: Option<&str>,
        last_used_at: Option<String>,
        expires_at: Option<String>,
        revoked_at: Option<String>,
        created_at: String,
    ) -> ApiKeySummary {
        ApiKeySummary {
            id,
            name,
            key_prefix,
            scopes: stored_scopes(scopes_json, legacy_scopes),
            last_used_at,
            expires_at,
            revoked_at,
            created_at,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum IngestScope {
    ApiKey { site_id: String },
}

impl IngestScope {
    pub(crate) fn allows_site(&self, site: &ActiveSite) -> bool {
        match self {
            Self::ApiKey { site_id } => site_id == &site.id,
        }
    }
}

pub async fn require_api_key(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let Some(api_key) = read_api_key(&request) else {
        return Err(ApiError::Unauthorized("API key is required".to_owned()));
    };

    let key_hash = auth::hash_secret(&state.settings.hash_salt, &["api-key", &api_key]);
    let now = Utc::now().to_rfc3339();
    let record = sqlx::query_as::<_, ApiKeyRecord>(
        r#"
        SELECT id, site_id, scopes_json, scopes, expires_at
        FROM api_keys
        WHERE key_hash = ?
            AND revoked_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(&key_hash)
    .fetch_optional(&state.sqlite)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("Invalid API key".to_owned()))?;

    if !record.allows_event_write()
        || record
            .expires_at
            .as_deref()
            .is_some_and(|expiry| !expiry_is_after(expiry, Utc::now()))
    {
        return Err(ApiError::Unauthorized("Invalid API key".to_owned()));
    }
    // Organization-wide API keys from early development builds are deliberately
    // not accepted here. Ingestion credentials are data-integrity boundaries and
    // must name exactly one site.
    let site_id = record
        .site_id
        .ok_or_else(|| ApiError::Unauthorized("Invalid API key".to_owned()))?;

    sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ? AND (last_used_at IS NULL OR julianday(last_used_at) < julianday('now', '-1 minute'))")
        .bind(now)
        .bind(record.id)
        .execute(&state.sqlite)
        .await?;

    request
        .extensions_mut()
        .insert(IngestScope::ApiKey { site_id });

    Ok(next.run(request).await)
}

pub(crate) async fn list_api_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Json<Vec<ApiKeySummary>>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        return Ok(Json(Vec::new()));
    }
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    Ok(Json(load_api_key_summaries(&state.sqlite, &site.id).await?))
}

pub(crate) async fn create_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<CreateApiKeyRequest>,
) -> Result<Response, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    let name = normalize_key_name(&request.name)?;
    let expires_at = request
        .expires_at
        .as_deref()
        .map(normalize_expiry)
        .transpose()?;
    validate_requested_scopes(request.scopes.as_deref())?;

    Ok(created_api_key_http_response(
        create_api_key_record(
            &state.sqlite,
            &state.settings.hash_salt,
            &session.id,
            &site,
            &name,
            expires_at.as_deref(),
        )
        .await?,
    ))
}

pub(crate) async fn update_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, key_id)): Path<(String, String)>,
    Json(request): Json<UpdateApiKeyRequest>,
) -> Result<Json<ApiKeySummary>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    let name = request
        .name
        .as_deref()
        .map(normalize_key_name)
        .transpose()?;
    let (update_expiry, expires_at) = match request.expires_at {
        ExpiryUpdate::Missing => (false, None),
        ExpiryUpdate::Clear => (true, None),
        ExpiryUpdate::Set(value) => (true, Some(normalize_expiry(&value)?)),
    };
    if name.is_none() && !update_expiry {
        return Err(ApiError::BadRequest(
            "At least one API key field must be provided".to_owned(),
        ));
    }

    update_api_key_record(
        &state.sqlite,
        &site.id,
        &key_id,
        name.as_deref(),
        update_expiry,
        expires_at.as_deref(),
    )
    .await?;

    Ok(Json(
        load_api_key_summary(&state.sqlite, &site.id, &key_id).await?,
    ))
}

pub(crate) async fn revoke_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, key_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    revoke_api_key_record(&state.sqlite, &site.id, &key_id).await?;
    Ok(Json(json!({ "status": "revoked" })))
}

fn read_api_key(request: &Request) -> Option<String> {
    if let Some(value) = request.headers().get(API_KEY_HEADER) {
        return value
            .to_str()
            .ok()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
    }

    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[derive(sqlx::FromRow)]
struct ApiKeyRecord {
    expires_at: Option<String>,
    id: String,
    scopes: Option<String>,
    scopes_json: String,
    site_id: Option<String>,
}

impl ApiKeyRecord {
    fn allows_event_write(&self) -> bool {
        stored_scopes(&self.scopes_json, self.scopes.as_deref())
            .iter()
            .any(|scope| scope == EVENTS_WRITE_SCOPE)
    }
}

async fn create_api_key_record(
    pool: &SqlitePool,
    hash_salt: &str,
    user_id: &str,
    site: &ActiveSite,
    name: &str,
    expires_at: Option<&str>,
) -> Result<CreatedApiKeyResponse, ApiError> {
    let secret = generate_api_key();
    let key_prefix = secret.chars().take(18).collect::<String>();
    let key_hash = auth::hash_secret(hash_salt, &["api-key", &secret]);
    let id = Uuid::new_v4().to_string();
    let scopes_json = serde_json::to_string(&[EVENTS_WRITE_SCOPE])?;

    sqlx::query(
        r#"
        INSERT INTO api_keys (
            id,
            organization_id,
            site_id,
            team_id,
            user_id,
            name,
            key_prefix,
            key_hash,
            scopes_json,
            scopes,
            expires_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&site.organization_id)
    .bind(&site.id)
    .bind(&site.team_id)
    .bind(user_id)
    .bind(name)
    .bind(&key_prefix)
    .bind(key_hash)
    .bind(&scopes_json)
    .bind(EVENTS_WRITE_SCOPE)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(CreatedApiKeyResponse {
        api_key: load_api_key_summary(pool, &site.id, &id).await?,
        secret,
    })
}

fn created_api_key_http_response(created: CreatedApiKeyResponse) -> Response {
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(created),
    )
        .into_response()
}

async fn load_api_key_summaries(
    pool: &SqlitePool,
    site_id: &str,
) -> Result<Vec<ApiKeySummary>, ApiError> {
    let rows = sqlx::query_as::<_, ApiKeySummaryRow>(
        r#"
        SELECT
            id,
            name,
            key_prefix,
            scopes_json,
            scopes AS legacy_scopes,
            last_used_at,
            expires_at,
            revoked_at,
            created_at
        FROM api_keys
        WHERE site_id = ?
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(ApiKeySummaryRow::into_summary)
        .collect())
}

async fn load_api_key_summary(
    pool: &SqlitePool,
    site_id: &str,
    key_id: &str,
) -> Result<ApiKeySummary, ApiError> {
    sqlx::query_as::<_, ApiKeySummaryRow>(
        r#"
        SELECT
            id,
            name,
            key_prefix,
            scopes_json,
            scopes AS legacy_scopes,
            last_used_at,
            expires_at,
            revoked_at,
            created_at
        FROM api_keys
        WHERE id = ? AND site_id = ?
        LIMIT 1
        "#,
    )
    .bind(key_id)
    .bind(site_id)
    .fetch_optional(pool)
    .await?
    .map(ApiKeySummaryRow::into_summary)
    .ok_or_else(|| ApiError::BadRequest("Unknown API key".to_owned()))
}

async fn revoke_api_key_record(
    pool: &SqlitePool,
    site_id: &str,
    key_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        UPDATE api_keys
        SET revoked_at = COALESCE(revoked_at, ?)
        WHERE id = ? AND site_id = ?
        "#,
    )
    .bind(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true))
    .bind(key_id)
    .bind(site_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn update_api_key_record(
    pool: &SqlitePool,
    site_id: &str,
    key_id: &str,
    name: Option<&str>,
    update_expiry: bool,
    expires_at: Option<&str>,
) -> Result<(), ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE api_keys
        SET
            name = COALESCE(?, name),
            expires_at = CASE WHEN ? THEN ? ELSE expires_at END
        WHERE id = ? AND site_id = ?
        "#,
    )
    .bind(name)
    .bind(update_expiry)
    .bind(expires_at)
    .bind(key_id)
    .bind(site_id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest("Unknown API key".to_owned()));
    }
    Ok(())
}

fn generate_api_key() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("owl_sk_{}", BASE64URL_NOPAD.encode(&bytes))
}

fn normalize_key_name(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_KEY_NAME_BYTES || value.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "API key name must be between 1 and 120 characters".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

fn normalize_expiry(value: &str) -> Result<String, ApiError> {
    let parsed = DateTime::parse_from_rfc3339(value.trim())
        .map_err(|_| ApiError::BadRequest("expires_at must be an RFC3339 timestamp".to_owned()))?
        .with_timezone(&Utc);
    if parsed <= Utc::now() {
        return Err(ApiError::BadRequest(
            "expires_at must be in the future".to_owned(),
        ));
    }
    Ok(parsed.to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn expiry_is_after(value: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc) > now)
        .unwrap_or(false)
}

fn validate_requested_scopes(scopes: Option<&[String]>) -> Result<(), ApiError> {
    if scopes.is_some_and(|scopes| scopes != [EVENTS_WRITE_SCOPE]) {
        return Err(ApiError::BadRequest(
            "API keys currently require exactly the events:write scope".to_owned(),
        ));
    }
    Ok(())
}

fn stored_scopes(scopes_json: &str, legacy_scopes: Option<&str>) -> Vec<String> {
    let mut scopes = parse_scopes(scopes_json);
    if scopes.is_empty() {
        scopes = legacy_scopes.map(parse_scopes).unwrap_or_default();
    }
    scopes.sort();
    scopes.dedup();
    scopes
}

fn parse_scopes(value: &str) -> Vec<String> {
    let value = value.trim();
    if value.is_empty() {
        return Vec::new();
    }
    if let Ok(scopes) = serde_json::from_str::<Vec<String>>(value) {
        return scopes
            .into_iter()
            .map(|scope| scope.trim().to_owned())
            .filter(|scope| !scope.is_empty())
            .collect();
    }
    value
        .split([',', ' '])
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use crate::storage::sqlite;

    use super::*;

    #[test]
    fn api_key_hash_changes_with_salt() {
        let first = auth::hash_secret("salt-a", &["api-key", "owleye_secret"]);
        let second = auth::hash_secret("salt-b", &["api-key", "owleye_secret"]);

        assert_ne!(first, second);
    }

    #[test]
    fn managed_scopes_require_explicit_event_write_permission() {
        let record = |scopes_json: &str, scopes: Option<&str>| ApiKeyRecord {
            expires_at: None,
            id: "key".to_owned(),
            scopes: scopes.map(str::to_owned),
            scopes_json: scopes_json.to_owned(),
            site_id: Some("site".to_owned()),
        };

        assert!(record(r#"["events:write"]"#, None).allows_event_write());
        assert!(record("[]", Some("events:write")).allows_event_write());
        assert!(!record(r#"["events:read"]"#, Some("events:write")).allows_event_write());
        assert!(!record("[]", None).allows_event_write());
        assert!(!record(r#"["unknown"]"#, None).allows_event_write());
        assert!(validate_requested_scopes(None).is_ok());
        assert!(validate_requested_scopes(Some(&["events:write".to_owned()])).is_ok());
        assert!(validate_requested_scopes(Some(&["events:read".to_owned()])).is_err());
    }

    #[test]
    fn expiry_is_normalized_to_utc_and_must_be_future() {
        assert_eq!(
            normalize_expiry("2099-01-01T12:00:00+05:30").unwrap(),
            "2099-01-01T06:30:00.000Z"
        );
        assert!(normalize_expiry("2020-01-01T00:00:00Z").is_err());
        assert!(!expiry_is_after("invalid", Utc::now()));
    }

    #[test]
    fn every_ingestion_credential_is_strictly_site_scoped() {
        let site_a = ActiveSite {
            created_by_user_id: None,
            id: "site-a".to_owned(),
            organization_id: Some("org-a".to_owned()),
            team_id: Some("team-a".to_owned()),
            tracking_id: "public-a".to_owned(),
        };
        let site_b = ActiveSite {
            created_by_user_id: None,
            id: "site-b".to_owned(),
            organization_id: Some("org-a".to_owned()),
            team_id: Some("team-a".to_owned()),
            tracking_id: "public-b".to_owned(),
        };

        let current_site_key = IngestScope::ApiKey {
            site_id: "site-a".to_owned(),
        };
        assert!(current_site_key.allows_site(&site_a));
        assert!(
            !current_site_key.allows_site(&site_b),
            "sites sharing organization/team metadata remain isolated"
        );
    }

    #[tokio::test]
    async fn managed_keys_are_hashed_site_scoped_and_idempotently_revoked() {
        let pool = test_pool().await;
        let (site_a, site_b) = seed_sites(&pool).await;
        assert!(sites::resolve_site_for_admin(&pool, "owner", &site_a.id)
            .await
            .is_ok());
        assert!(matches!(
            sites::resolve_site_for_admin(&pool, "outsider", &site_a.id).await,
            Err(ApiError::Forbidden(_))
        ));

        let expiry = normalize_expiry(&(Utc::now() + Duration::days(1)).to_rfc3339()).unwrap();
        let created = create_api_key_record(
            &pool,
            "test-salt",
            "owner",
            &site_a,
            "Production ingest",
            Some(&expiry),
        )
        .await
        .unwrap();

        assert!(created.secret.starts_with("owl_sk_"));
        assert_eq!(created.api_key.key_prefix.len(), 18);
        assert_eq!(created.api_key.scopes, vec![EVENTS_WRITE_SCOPE]);
        let summary_json = serde_json::to_value(&created.api_key).unwrap();
        assert!(summary_json.get("secret").is_none());
        assert!(summary_json.get("key_hash").is_none());
        let create_json = serde_json::to_value(&created).unwrap();
        assert!(create_json.get("api_key").is_some());
        assert!(create_json.get("secret").is_some());
        assert_eq!(create_json.as_object().unwrap().len(), 2);

        let (stored_hash, stored_site, stored_expiry): (String, String, Option<String>) =
            sqlx::query_as("SELECT key_hash, site_id, expires_at FROM api_keys WHERE id = ?")
                .bind(&created.api_key.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            stored_hash,
            auth::hash_secret("test-salt", &["api-key", &created.secret])
        );
        assert_ne!(stored_hash, created.secret);
        assert_eq!(stored_site, site_a.id);
        assert_eq!(stored_expiry.as_deref(), Some(expiry.as_str()));
        assert!(load_api_key_summaries(&pool, &site_b.id)
            .await
            .unwrap()
            .is_empty());

        revoke_api_key_record(&pool, &site_b.id, &created.api_key.id)
            .await
            .unwrap();
        let revoked: Option<String> =
            sqlx::query_scalar("SELECT revoked_at FROM api_keys WHERE id = ?")
                .bind(&created.api_key.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(revoked.is_none());

        revoke_api_key_record(&pool, &site_a.id, &created.api_key.id)
            .await
            .unwrap();
        let first_revocation: String =
            sqlx::query_scalar("SELECT revoked_at FROM api_keys WHERE id = ?")
                .bind(&created.api_key.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        revoke_api_key_record(&pool, &site_a.id, &created.api_key.id)
            .await
            .unwrap();
        let second_revocation: String =
            sqlx::query_scalar("SELECT revoked_at FROM api_keys WHERE id = ?")
                .bind(&created.api_key.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(first_revocation, second_revocation);

        let response = created_api_key_http_response(created);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::PRAGMA], "no-cache");
    }

    #[tokio::test]
    async fn explicit_null_clears_expiry_while_omission_keeps_it() {
        let pool = test_pool().await;
        let (site, _) = seed_sites(&pool).await;
        let expiry = normalize_expiry(&(Utc::now() + Duration::days(1)).to_rfc3339()).unwrap();
        let created =
            create_api_key_record(&pool, "test-salt", "owner", &site, "Initial", Some(&expiry))
                .await
                .unwrap();

        let omitted: UpdateApiKeyRequest =
            serde_json::from_value(json!({ "name": "Renamed" })).unwrap();
        assert!(matches!(omitted.expires_at, ExpiryUpdate::Missing));
        update_api_key_record(
            &pool,
            &site.id,
            &created.api_key.id,
            omitted.name.as_deref(),
            false,
            None,
        )
        .await
        .unwrap();
        let retained: Option<String> =
            sqlx::query_scalar("SELECT expires_at FROM api_keys WHERE id = ?")
                .bind(&created.api_key.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(retained.as_deref(), Some(expiry.as_str()));

        let clear: UpdateApiKeyRequest =
            serde_json::from_value(json!({ "expires_at": null })).unwrap();
        assert!(matches!(clear.expires_at, ExpiryUpdate::Clear));
        update_api_key_record(&pool, &site.id, &created.api_key.id, None, true, None)
            .await
            .unwrap();
        let cleared: Option<String> =
            sqlx::query_scalar("SELECT expires_at FROM api_keys WHERE id = ?")
                .bind(&created.api_key.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(cleared.is_none());
    }

    async fn seed_sites(pool: &SqlitePool) -> (ActiveSite, ActiveSite) {
        for (id, email) in [
            ("owner", "owner@example.com"),
            ("outsider", "outsider@example.com"),
        ] {
            sqlx::query("INSERT INTO users (id, email) VALUES (?, ?)")
                .bind(id)
                .bind(email)
                .execute(pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ('org', 'Org', 'org')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO organization_members (organization_id, user_id, role) VALUES ('org', 'owner', 'owner')",
        )
        .execute(pool)
        .await
        .unwrap();
        for (id, tracking_id) in [("site-a", "public-a"), ("site-b", "public-b")] {
            sqlx::query(
                r#"
                INSERT INTO sites (
                    id, organization_id, tracking_id, name, domain, created_by_user_id
                )
                VALUES (?, 'org', ?, ?, ?, 'owner')
                "#,
            )
            .bind(id)
            .bind(tracking_id)
            .bind(id)
            .bind(format!("{id}.example.com"))
            .execute(pool)
            .await
            .unwrap();
        }

        (
            ActiveSite {
                created_by_user_id: Some("owner".to_owned()),
                id: "site-a".to_owned(),
                organization_id: Some("org".to_owned()),
                team_id: None,
                tracking_id: "public-a".to_owned(),
            },
            ActiveSite {
                created_by_user_id: Some("owner".to_owned()),
                id: "site-b".to_owned(),
                organization_id: Some("org".to_owned()),
                team_id: None,
                tracking_id: "public-b".to_owned(),
            },
        )
    }

    async fn test_pool() -> SqlitePool {
        let db_path =
            std::env::temp_dir().join(format!("owleye-api-keys-{}.sqlite", Uuid::new_v4()));
        sqlite::connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap()
    }
}
