use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::{Duration, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{ai, auth, demo, retention, storage::clickhouse::ClickHouse, ApiError, AppState};

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ActiveSite {
    pub created_by_user_id: Option<String>,
    pub id: String,
    pub organization_id: Option<String>,
    pub team_id: Option<String>,
    pub tracking_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SiteRole {
    Owner,
    Admin,
    ReadOnly,
}

impl SiteRole {
    fn from_database(value: &str) -> Self {
        match value {
            "owner" => Self::Owner,
            "admin" => Self::Admin,
            _ => Self::ReadOnly,
        }
    }

    pub(crate) fn can_manage(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    pub(crate) fn is_owner(self) -> bool {
        self == Self::Owner
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SitePermissions {
    ai_use: bool,
    analytics_read: bool,
    rules_write: bool,
    settings_manage: bool,
    users_manage: bool,
}

impl From<SiteRole> for SitePermissions {
    fn from(role: SiteRole) -> Self {
        Self {
            ai_use: false,
            analytics_read: true,
            rules_write: role.can_manage(),
            settings_manage: role.is_owner(),
            users_manage: role.can_manage(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SiteEntitlements {
    ai: bool,
    uptime: bool,
}

#[derive(Debug, Serialize)]
pub struct ManagedSite {
    #[serde(skip_serializing_if = "Option::is_none")]
    entitlements: Option<SiteEntitlements>,
    created_at: String,
    domain: String,
    id: String,
    name: String,
    organization_id: Option<String>,
    permissions: SitePermissions,
    public_key: Option<String>,
    role: SiteRole,
    timezone: String,
    tracking_id: String,
    updated_at: String,
}

impl ManagedSite {
    fn with_ai_use(mut self, ai_use: bool) -> Self {
        self.permissions.ai_use = ai_use;
        self
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ManagedSiteRow {
    created_at: String,
    domain: String,
    id: String,
    name: String,
    organization_id: Option<String>,
    public_key: Option<String>,
    timezone: String,
    tracking_id: String,
    updated_at: String,
}

impl ManagedSiteRow {
    fn with_role(self, role: SiteRole) -> ManagedSite {
        ManagedSite {
            entitlements: None,
            created_at: self.created_at,
            domain: self.domain,
            id: self.id,
            name: self.name,
            organization_id: self.organization_id,
            permissions: role.into(),
            public_key: self.public_key,
            role,
            timezone: self.timezone,
            tracking_id: self.tracking_id,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ManagedSiteAccessRow {
    ai_use: bool,
    created_at: String,
    domain: String,
    id: String,
    name: String,
    organization_id: Option<String>,
    public_key: Option<String>,
    role: String,
    timezone: String,
    tracking_id: String,
    updated_at: String,
}

impl ManagedSiteAccessRow {
    fn into_site(self) -> ManagedSite {
        ManagedSiteRow {
            created_at: self.created_at,
            domain: self.domain,
            id: self.id,
            name: self.name,
            organization_id: self.organization_id,
            public_key: self.public_key,
            timezone: self.timezone,
            tracking_id: self.tracking_id,
            updated_at: self.updated_at,
        }
        .with_role(SiteRole::from_database(&self.role))
        .with_ai_use(self.ai_use)
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SiteDomain {
    created_at: String,
    domain: String,
    id: String,
    is_primary: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateSiteRequest {
    domain: Option<String>,
    #[serde(default)]
    domains: Option<CreateDomainsInput>,
    name: String,
    organization_id: Option<String>,
    timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CreateDomainsInput {
    CommaSeparated(String),
    List(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct UpdateSiteRequest {
    domain: Option<String>,
    name: Option<String>,
    timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDomainRequest {
    domain: String,
}

pub async fn resolve_active_site(
    pool: &SqlitePool,
    identifier: &str,
) -> Result<ActiveSite, ApiError> {
    sqlx::query_as::<_, ActiveSite>(
        r#"
        SELECT id, tracking_id, organization_id, team_id, created_by_user_id
        FROM sites
        WHERE (id = ? OR tracking_id = ?)
            AND archived_at IS NULL
            AND deleted_at IS NULL
            AND erasure_pending_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(identifier)
    .bind(identifier)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::BadRequest("Unknown or inactive site_id".to_owned()))
}

/// Revalidate the erasure fence immediately before an event batch enters the
/// ClickHouse writer. The initial site lookup is intentionally not enough: an
/// owner can request erasure while an already-authorized ingest request is
/// still constructing its bounded event rows.
pub(crate) async fn ensure_site_accepts_ingest(
    pool: &SqlitePool,
    site_id: &str,
) -> Result<(), ApiError> {
    let accepting = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM sites
            WHERE id = ?
                AND archived_at IS NULL
                AND deleted_at IS NULL
                AND erasure_pending_at IS NULL
        )
        "#,
    )
    .bind(site_id)
    .fetch_one(pool)
    .await?
        == 1;
    if accepting {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "Unknown or inactive site_id".to_owned(),
        ))
    }
}

pub async fn resolve_site_for_user(
    pool: &SqlitePool,
    user_id: &str,
    identifier: &str,
) -> Result<ActiveSite, ApiError> {
    sqlx::query_as::<_, ActiveSite>(
        r#"
        SELECT s.id, s.tracking_id, s.organization_id, s.team_id, s.created_by_user_id
        FROM sites AS s
        WHERE (s.id = ? OR s.tracking_id = ?)
            AND s.archived_at IS NULL
            AND s.deleted_at IS NULL
            AND (
                s.created_by_user_id = ?
                OR EXISTS (
                    SELECT 1
                    FROM site_memberships AS membership
                    WHERE membership.site_id = s.id
                        AND membership.user_id = ?
                )
            )
        LIMIT 1
        "#,
    )
    .bind(identifier)
    .bind(identifier)
    .bind(user_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))
}

pub async fn resolve_site_for_admin(
    pool: &SqlitePool,
    user_id: &str,
    identifier: &str,
) -> Result<ActiveSite, ApiError> {
    sqlx::query_as::<_, ActiveSite>(
        r#"
        SELECT s.id, s.tracking_id, s.organization_id, s.team_id, s.created_by_user_id
        FROM sites AS s
        WHERE (s.id = ? OR s.tracking_id = ?)
            AND s.archived_at IS NULL
            AND s.deleted_at IS NULL
            AND (
                s.created_by_user_id = ?
                OR EXISTS (
                    SELECT 1
                    FROM site_memberships AS membership
                    WHERE membership.site_id = s.id
                        AND membership.user_id = ?
                        AND membership.role IN ('owner', 'admin')
                )
            )
        LIMIT 1
        "#,
    )
    .bind(identifier)
    .bind(identifier)
    .bind(user_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::Forbidden("Site administrator access is required".to_owned()))
}

pub(crate) async fn resolve_site_for_owner(
    pool: &SqlitePool,
    user_id: &str,
    identifier: &str,
) -> Result<ActiveSite, ApiError> {
    let site = resolve_site_for_user(pool, user_id, identifier).await?;
    if effective_site_role(pool, user_id, &site).await?.is_owner() {
        Ok(site)
    } else {
        Err(ApiError::Forbidden(
            "Site owner access is required".to_owned(),
        ))
    }
}

pub(crate) async fn effective_site_role(
    pool: &SqlitePool,
    user_id: &str,
    site: &ActiveSite,
) -> Result<SiteRole, ApiError> {
    if site.created_by_user_id.as_deref() == Some(user_id) {
        return Ok(SiteRole::Owner);
    }

    sqlx::query_scalar::<_, String>(
        "SELECT role FROM site_memberships WHERE site_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&site.id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .map(|role| SiteRole::from_database(&role))
    .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))
}

pub async fn origin_is_allowed(
    pool: &SqlitePool,
    site_id: &str,
    origin: &str,
    allow_loopback_origins: bool,
) -> Result<bool, ApiError> {
    let Some(origin_host) = origin_host(origin) else {
        return Ok(false);
    };
    let mut domains = sqlx::query_scalar::<_, String>(
        r#"
        SELECT domain
        FROM sites
        WHERE id = ?
        UNION
        SELECT domain
        FROM site_domains
        WHERE site_id = ?
        "#,
    )
    .bind(site_id)
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    // An existing site contributes its primary domain even when it is blank.
    // Do not interpret an unknown site as an unrestricted site.
    if domains.is_empty() {
        return Ok(false);
    }
    domains.retain(|domain| !domain.trim().is_empty());
    if domains.is_empty() {
        return Ok(true);
    }
    if is_loopback_host(&origin_host) {
        return Ok(allow_loopback_origins);
    }

    Ok(domains
        .iter()
        .any(|domain| domain_matches(domain, &origin_host)))
}

pub(crate) async fn list_sites(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ManagedSite>>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        return Ok(Json(demo_managed_sites()));
    }
    Ok(Json(load_managed_sites(&state.sqlite, &session.id).await?))
}

async fn load_managed_sites(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<ManagedSite>, ApiError> {
    let sites = sqlx::query_as::<_, ManagedSiteAccessRow>(
        r#"
        SELECT DISTINCT
            s.id,
            s.organization_id,
            s.tracking_id,
            s.public_key,
            s.name,
            s.domain,
            COALESCE(s.default_timezone, s.timezone, 'UTC') AS timezone,
            s.created_at,
            s.updated_at,
            CASE
                WHEN s.created_by_user_id = ? THEN 'owner'
                ELSE COALESCE(membership.role, 'read_only')
            END AS role,
            1 AS ai_use
        FROM sites AS s
        LEFT JOIN site_memberships AS membership
            ON membership.site_id = s.id AND membership.user_id = ?
        WHERE s.archived_at IS NULL
            AND s.deleted_at IS NULL
            AND (
                s.created_by_user_id = ?
                OR membership.user_id IS NOT NULL
            )
        ORDER BY s.created_at ASC
        "#,
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::with_capacity(sites.len());
    for row in sites {
        let mut site = row.into_site();
        site.entitlements = Some(SiteEntitlements {
            ai: true,
            uptime: true,
        });
        result.push(site);
    }
    Ok(result)
}

pub(crate) async fn create_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSiteRequest>,
) -> Result<Json<ManagedSite>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let name = normalize_name(&request.name)?;
    let domains = normalize_create_domains(request.domain.as_deref(), request.domains.as_ref())?;
    let domain = domains.first().cloned().unwrap_or_default();
    let timezone = normalize_timezone(request.timezone.as_deref().unwrap_or("UTC"))?;
    let organization_id = organization_for_new_site(
        &state.sqlite,
        &session.id,
        request.organization_id.as_deref(),
    )
    .await?;
    let id = Uuid::new_v4().to_string();
    let tracking_id = format!("owl_{}", Uuid::new_v4().simple());
    let public_key = format!("owl_pub_{}", Uuid::new_v4().simple());
    let now = Utc::now().to_rfc3339();
    let mut transaction = state.sqlite.begin_with("BEGIN IMMEDIATE").await?;

    sqlx::query(
        r#"
        INSERT INTO sites (
            id,
            organization_id,
            tracking_id,
            public_key,
            name,
            domain,
            default_timezone,
            timezone,
            created_by_user_id,
            created_at,
            updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&organization_id)
    .bind(&tracking_id)
    .bind(&public_key)
    .bind(&name)
    .bind(&domain)
    .bind(&timezone)
    .bind(&timezone)
    .bind(&session.id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO site_memberships (site_id, user_id, role, created_at, updated_at)
        VALUES (?, ?, 'owner', ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&session.id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await?;
    for (index, allowed_domain) in domains.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO site_domains (id, site_id, domain, is_primary, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&id)
        .bind(allowed_domain)
        .bind(index == 0)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query("INSERT OR IGNORE INTO site_memberships(site_id,user_id,role) SELECT ?,id,CASE WHEN is_admin=1 THEN 'owner' ELSE 'read_only' END FROM users WHERE status='active'").bind(&id).execute(&mut *transaction).await?;
    transaction.commit().await?;

    Ok(Json(
        fetch_managed_site(&state.sqlite, &id, SiteRole::Owner)
            .await?
            .with_ai_use(true),
    ))
}

pub(crate) async fn get_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(identifier): Path<String>,
) -> Result<Json<ManagedSite>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        return demo_managed_site(&identifier).map(Json);
    }
    let site = resolve_site_for_user(&state.sqlite, &session.id, &identifier).await?;
    let role = effective_site_role(&state.sqlite, &session.id, &site).await?;
    let ai_use = ai::user_has_ai_page_access(&state.sqlite, &site, &session.id).await?;
    Ok(Json(
        fetch_managed_site(&state.sqlite, &site.id, role)
            .await?
            .with_ai_use(ai_use),
    ))
}

pub(crate) async fn update_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(identifier): Path<String>,
    Json(request): Json<UpdateSiteRequest>,
) -> Result<Json<ManagedSite>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = resolve_site_for_owner(&state.sqlite, &session.id, &identifier).await?;
    let name = request.name.as_deref().map(normalize_name).transpose()?;
    let domain = request
        .domain
        .as_deref()
        .map(|domain| normalize_domain(domain, false))
        .transpose()?;
    let timezone = request
        .timezone
        .as_deref()
        .map(normalize_timezone)
        .transpose()?;
    if name.is_none() && domain.is_none() && timezone.is_none() {
        return Err(ApiError::BadRequest(
            "At least one site field must be provided".to_owned(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let mut transaction = state.sqlite.begin().await?;
    sqlx::query(
        r#"
        UPDATE sites
        SET
            name = COALESCE(?, name),
            domain = COALESCE(?, domain),
            default_timezone = COALESCE(?, default_timezone),
            timezone = COALESCE(?, timezone),
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(name)
    .bind(domain.as_deref())
    .bind(timezone.as_deref())
    .bind(timezone.as_deref())
    .bind(&now)
    .bind(&site.id)
    .execute(&mut *transaction)
    .await?;

    if let Some(domain) = domain {
        sqlx::query("UPDATE site_domains SET is_primary = 0 WHERE site_id = ?")
            .bind(&site.id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            r#"
            INSERT INTO site_domains (id, site_id, domain, is_primary, created_at)
            VALUES (?, ?, ?, 1, ?)
            ON CONFLICT(site_id, domain)
            DO UPDATE SET is_primary = 1
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&site.id)
        .bind(domain)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;

    let role = effective_site_role(&state.sqlite, &session.id, &site).await?;
    let ai_use = ai::user_has_ai_page_access(&state.sqlite, &site, &session.id).await?;
    Ok(Json(
        fetch_managed_site(&state.sqlite, &site.id, role)
            .await?
            .with_ai_use(ai_use),
    ))
}

pub(crate) async fn delete_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(identifier): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = resolve_site_for_owner(&state.sqlite, &session.id, &identifier).await?;
    let now = Utc::now().to_rfc3339();
    submit_site_deletion(&state.clickhouse, &state.sqlite, &site, &now).await?;

    Ok(Json(json!({
        "analytics_deletion": "completed",
        "deletion_mode": "synchronous_evidenced",
        "site_id": site.id,
        "status": "deleted"
    })))
}

pub(crate) async fn submit_site_deletion(
    clickhouse: &ClickHouse,
    pool: &SqlitePool,
    site: &ActiveSite,
    deleted_at: &str,
) -> Result<(), ApiError> {
    // Explicit erasure preempts scheduled retention and waits for ClickHouse
    // replicas before the control-plane site is archived.
    let deletion_time = chrono::DateTime::parse_from_rfc3339(deleted_at)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| ApiError::Internal(error.into()))?;
    retention::execute_site_erasure(clickhouse, pool, site, deletion_time).await?;
    let result = sqlx::query(
        "UPDATE sites SET archived_at = ?, deleted_at = unixepoch(), updated_at = ? WHERE id = ?",
    )
    .bind(deleted_at)
    .bind(deleted_at)
    .bind(&site.id)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(ApiError::Conflict(
            "Site deletion could not be finalized".to_owned(),
        ));
    }
    Ok(())
}

pub(crate) async fn list_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(identifier): Path<String>,
) -> Result<Json<Vec<SiteDomain>>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        let site = demo::demo_site(&identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Ok(Json(vec![SiteDomain {
            created_at: demo_timestamp(site.created_days_ago),
            domain: site.domain.to_owned(),
            id: format!("domain_{}", site.id),
            is_primary: true,
        }]));
    }
    let site = resolve_site_for_user(&state.sqlite, &session.id, &identifier).await?;
    let domains = sqlx::query_as::<_, SiteDomain>(
        r#"
        SELECT id, domain, is_primary, created_at
        FROM site_domains
        WHERE site_id = ?
        ORDER BY is_primary DESC, created_at ASC
        "#,
    )
    .bind(site.id)
    .fetch_all(&state.sqlite)
    .await?;

    Ok(Json(domains))
}

pub(crate) async fn create_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(identifier): Path<String>,
    Json(request): Json<CreateDomainRequest>,
) -> Result<Json<SiteDomain>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = resolve_site_for_admin(&state.sqlite, &session.id, &identifier).await?;
    let domain = normalize_domain(&request.domain, true)?;
    let domain_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let record = sqlx::query_as::<_, SiteDomain>(
        r#"
        INSERT INTO site_domains (id, site_id, domain, is_primary, created_at)
        VALUES (?, ?, ?, 0, ?)
        ON CONFLICT(site_id, domain)
        DO UPDATE SET domain = excluded.domain
        RETURNING id, domain, is_primary, created_at
        "#,
    )
    .bind(domain_id)
    .bind(site.id)
    .bind(domain)
    .bind(now)
    .fetch_one(&state.sqlite)
    .await?;

    Ok(Json(record))
}

pub(crate) async fn delete_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((identifier, domain_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = resolve_site_for_admin(&state.sqlite, &session.id, &identifier).await?;
    let is_primary = sqlx::query_scalar::<_, bool>(
        "SELECT is_primary FROM site_domains WHERE id = ? AND site_id = ?",
    )
    .bind(&domain_id)
    .bind(&site.id)
    .fetch_optional(&state.sqlite)
    .await?
    .ok_or_else(|| ApiError::BadRequest("Unknown site domain".to_owned()))?;
    if is_primary {
        return Err(ApiError::BadRequest(
            "Change the site's primary domain before deleting it".to_owned(),
        ));
    }

    sqlx::query("DELETE FROM site_domains WHERE id = ? AND site_id = ?")
        .bind(domain_id)
        .bind(site.id)
        .execute(&state.sqlite)
        .await?;
    Ok(Json(json!({ "status": "deleted" })))
}

fn demo_managed_sites() -> Vec<ManagedSite> {
    demo::DEMO_SITES
        .iter()
        .map(managed_site_from_demo)
        .collect()
}

fn demo_managed_site(identifier: &str) -> Result<ManagedSite, ApiError> {
    demo::demo_site(identifier)
        .map(managed_site_from_demo)
        .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))
}

fn managed_site_from_demo(site: &demo::DemoSiteDefinition) -> ManagedSite {
    let role = demo_site_role(site);
    ManagedSite {
        entitlements: None,
        created_at: demo_timestamp(site.created_days_ago),
        domain: site.domain.to_owned(),
        id: site.id.to_owned(),
        name: site.name.to_owned(),
        organization_id: Some(demo::PREVIEW_ORGANIZATION_ID.to_owned()),
        permissions: role.into(),
        public_key: Some(site.public_key.to_owned()),
        role,
        timezone: site.timezone.to_owned(),
        tracking_id: site.tracking_id.to_owned(),
        updated_at: demo_timestamp(0),
    }
    .with_ai_use(ai::demo_user_has_ai_page_access(site))
}

pub(crate) fn demo_site_role(site: &demo::DemoSiteDefinition) -> SiteRole {
    match site.profile {
        demo::DemoTrafficProfile::LowVolume | demo::DemoTrafficProfile::Mixed => SiteRole::ReadOnly,
        _ => SiteRole::Owner,
    }
}

fn demo_timestamp(days_ago: i64) -> String {
    let date = Utc::now().date_naive() - Duration::days(days_ago);
    date.and_hms_opt(0, 0, 0)
        .expect("midnight is a valid time")
        .and_utc()
        .to_rfc3339()
}

async fn fetch_managed_site(
    pool: &SqlitePool,
    site_id: &str,
    role: SiteRole,
) -> Result<ManagedSite, ApiError> {
    let row = sqlx::query_as::<_, ManagedSiteRow>(
        r#"
        SELECT
            id,
            organization_id,
            tracking_id,
            public_key,
            name,
            domain,
            COALESCE(default_timezone, timezone, 'UTC') AS timezone,
            created_at,
            updated_at
        FROM sites
        WHERE id = ?
        LIMIT 1
        "#,
    )
    .bind(site_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::BadRequest("Unknown site".to_owned()))?;
    Ok(row.with_role(role))
}

async fn organization_for_new_site(
    pool: &SqlitePool,
    user_id: &str,
    requested: Option<&str>,
) -> Result<String, ApiError> {
    if requested.is_some_and(|id| id != "default") {
        return Err(ApiError::BadRequest(
            "Only the default workspace exists".into(),
        ));
    }
    crate::auth::require_admin(pool, user_id).await?;
    Ok("default".into())
}

fn normalize_name(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 || value.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "Site name must be between 1 and 120 characters".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

fn normalize_timezone(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+'))
    {
        return Err(ApiError::BadRequest("Invalid timezone".to_owned()));
    }
    Ok(value.to_owned())
}

fn normalize_domain(value: &str, allow_wildcard: bool) -> Result<String, ApiError> {
    let value = value.trim().trim_end_matches('/');
    let (wildcard, value) = value
        .strip_prefix("*.")
        .map(|domain| (true, domain))
        .unwrap_or((false, value));
    if wildcard && !allow_wildcard {
        return Err(ApiError::BadRequest(
            "The primary domain cannot be a wildcard".to_owned(),
        ));
    }
    let candidate = if value.contains("://") {
        value.to_owned()
    } else {
        format!("http://{value}")
    };
    let url = Url::parse(&candidate)
        .map_err(|_| ApiError::BadRequest("Invalid site domain".to_owned()))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.scheme(), "http" | "https")
        || (url.path() != "/" && !url.path().is_empty())
    {
        return Err(ApiError::BadRequest("Invalid site domain".to_owned()));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::BadRequest("Invalid site domain".to_owned()))?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if wildcard {
        Ok(format!("*.{host}"))
    } else {
        Ok(host)
    }
}

fn normalize_create_domains(
    legacy_domain: Option<&str>,
    domains: Option<&CreateDomainsInput>,
) -> Result<Vec<String>, ApiError> {
    let mut values = Vec::new();
    if let Some(domain) = legacy_domain {
        values.extend(domain.split(','));
    }
    match domains {
        Some(CreateDomainsInput::CommaSeparated(value)) => values.extend(value.split(',')),
        Some(CreateDomainsInput::List(list)) => {
            for value in list {
                values.extend(value.split(','));
            }
        }
        None => {}
    }

    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let domain = normalize_domain(value, false)?;
        if !normalized.contains(&domain) {
            normalized.push(domain);
        }
    }
    if normalized.len() > 50 {
        return Err(ApiError::BadRequest(
            "A site can start with at most 50 allowed domains".to_owned(),
        ));
    }
    Ok(normalized)
}

fn origin_host(origin: &str) -> Option<String> {
    let url = Url::parse(origin).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }

    url.host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']).trim_end_matches('.');
    host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn domain_matches(configured_domain: &str, origin_host: &str) -> bool {
    let configured_domain = configured_domain.trim().trim_end_matches('/');
    let (wildcard, configured_domain) = configured_domain
        .strip_prefix("*.")
        .map(|domain| (true, domain))
        .unwrap_or((false, configured_domain));
    let candidate = if configured_domain.contains("://") {
        configured_domain.to_owned()
    } else {
        format!("http://{configured_domain}")
    };
    let Some(configured_host) = Url::parse(&candidate)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
    else {
        return false;
    };

    if wildcard {
        origin_host != configured_host
            && origin_host
                .strip_suffix(&configured_host)
                .is_some_and(|prefix| prefix.ends_with('.'))
    } else {
        origin_host == configured_host
    }
}
