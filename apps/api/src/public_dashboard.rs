use crate::{
    auth,
    privacy::audit::{self, AuditEvent},
    sites, stats, ApiError, AppState,
};
use axum::{
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap},
    middleware::Next,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;

pub(crate) const TABLES: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS public_overview_shares (site_id TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE, config_json TEXT NOT NULL, revision TEXT NOT NULL, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)",
    "CREATE TABLE IF NOT EXISTS public_overview_urls (site_id TEXT NOT NULL REFERENCES public_overview_shares(site_id) ON DELETE CASCADE, url TEXT NOT NULL, PRIMARY KEY(site_id, url))",
    "CREATE INDEX IF NOT EXISTS idx_public_overview_url ON public_overview_urls(url)",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Metric {
    Visitors,
    Pageviews,
    Events,
    Sessions,
}
impl Metric {
    pub(crate) fn key(&self) -> &'static str {
        match self {
            Self::Visitors => "visitors",
            Self::Pageviews => "pageviews",
            Self::Events => "events",
            Self::Sessions => "sessions",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Breakdown {
    Countries,
    Browsers,
    Devices,
    OperatingSystems,
}
impl Breakdown {
    pub(crate) fn key(&self) -> &'static str {
        match self {
            Self::Countries => "countries",
            Self::Browsers => "browsers",
            Self::Devices => "devices",
            Self::OperatingSystems => "operating_systems",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShareConfig {
    pub enabled: bool,
    pub metrics: Vec<Metric>,
    pub traffic: bool,
    pub breakdowns: Vec<Breakdown>,
    pub max_days: u16,
    pub urls: Vec<String>,
}
impl Default for ShareConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            metrics: vec![Metric::Visitors, Metric::Pageviews],
            traffic: true,
            breakdowns: vec![],
            max_days: 7,
            urls: vec![],
        }
    }
}
fn eligible(_plan: ()) -> bool {
    true
}
fn unavailable() -> ApiError {
    ApiError::Forbidden("This public dashboard is unavailable.".into())
}

// This is a browser-origin policy, not authentication. Public data remains public.
pub(crate) fn allowed_origin(origin: &str) -> bool {
    reqwest::Url::parse(origin).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.origin().ascii_serialization() == origin
    })
}
pub(crate) async fn require_origin(request: Request, next: Next) -> Result<Response, ApiError> {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok());
    if !origin.is_some_and(allowed_origin) {
        return Err(ApiError::Forbidden(
            "This origin cannot access public dashboards.".into(),
        ));
    }
    Ok(next.run(request).await)
}

fn normalize_url(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.len() > 512 || value.contains('*') {
        return Err(ApiError::BadRequest(
            "Use an exact app domain, without wildcards.".into(),
        ));
    }
    let input = if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let url =
        reqwest::Url::parse(&input).map_err(|_| ApiError::BadRequest("Invalid app URL.".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::BadRequest(
            "Use an app URL without a path, credentials, query, or fragment.".into(),
        ));
    }
    Ok(url.to_string())
}
async fn available_urls(pool: &SqlitePool, site: &str) -> Result<Vec<String>, ApiError> {
    let values: Vec<String> = sqlx::query_scalar("SELECT domain FROM sites WHERE id = ? UNION SELECT domain FROM site_domains WHERE site_id = ?").bind(site).bind(site).fetch_all(pool).await?;
    let mut urls: Vec<_> = values
        .iter()
        .filter_map(|v| normalize_url(v).ok())
        .collect();
    urls.sort();
    urls.dedup();
    Ok(urls)
}
async fn read_config(pool: &SqlitePool, id: &str) -> Result<(ShareConfig, String), ApiError> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT config_json, revision FROM public_overview_shares WHERE site_id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    match row {
        Some((config, rev)) => Ok((serde_json::from_str(&config)?, rev)),
        None => Ok((ShareConfig::default(), String::new())),
    }
}
async fn owner(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
) -> Result<(sites::ActiveSite, String), ApiError> {
    let session = auth::authenticated_session(state, headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_owner(&state.sqlite, &session.id, id).await?;
    // Public publishing also requires membership in the app's organization.
    let member: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organizations o WHERE o.id = ? AND o.archived_at IS NULL AND (o.created_by_user_id = ? OR EXISTS(SELECT 1 FROM organization_members m WHERE m.organization_id = o.id AND m.user_id = ?)))")
        .bind(&site.organization_id).bind(&session.id).bind(&session.id).fetch_one(&state.sqlite).await?;
    if !member {
        return Err(ApiError::Forbidden(
            "Organization membership is required.".into(),
        ));
    }
    Ok((site, session.id))
}
pub(crate) async fn settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (site, _) = owner(&state, &headers, &id).await?;
    let plan = ();
    let (config, _) = read_config(&state.sqlite, &site.id).await?;
    Ok(Json(
        json!({"eligible": eligible(plan), "config": config, "available_urls": available_urls(&state.sqlite, &site.id).await?, "site_id": site.id}),
    ))
}
pub(crate) async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut config): Json<ShareConfig>,
) -> Result<Json<Value>, ApiError> {
    let (site, user) = owner(&state, &headers, &id).await?;
    if config.metrics.is_empty()
        || config.metrics.len() > 4
        || config.breakdowns.len() > 4
        || config.urls.len() > 20
        || ![7, 30, 90].contains(&config.max_days)
    {
        return Err(ApiError::BadRequest(
            "Select one to four metrics and a 7, 30, or 90 day history window.".into(),
        ));
    }
    if config
        .metrics
        .iter()
        .enumerate()
        .any(|(i, m)| config.metrics[..i].contains(m))
        || config
            .breakdowns
            .iter()
            .enumerate()
            .any(|(i, m)| config.breakdowns[..i].contains(m))
    {
        return Err(ApiError::BadRequest("Selections must be unique.".into()));
    }
    let available = available_urls(&state.sqlite, &site.id).await?;
    config.urls = config
        .urls
        .iter()
        .map(|v| normalize_url(v))
        .collect::<Result<Vec<_>, _>>()?;
    config.urls.sort();
    config.urls.dedup();
    if config.urls.iter().any(|url| !available.contains(url)) {
        return Err(ApiError::BadRequest(
            "Only URLs already configured on this app can be shared.".into(),
        ));
    }
    let mut tx = state.sqlite.begin_with("BEGIN IMMEDIATE").await?;
    let plan = ();
    if config.enabled && !eligible(plan) {
        return Err(ApiError::Forbidden(
            "Public sharing requires Pro or Business.".into(),
        ));
    }
    sqlx::query("INSERT INTO public_overview_shares (site_id, config_json, revision) VALUES (?, ?, ?) ON CONFLICT(site_id) DO UPDATE SET config_json = excluded.config_json, revision = excluded.revision, updated_at = CURRENT_TIMESTAMP")
        .bind(&site.id).bind(serde_json::to_string(&config)?).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM public_overview_urls WHERE site_id = ?")
        .bind(&site.id)
        .execute(&mut *tx)
        .await?;
    if config.enabled {
        for url in &config.urls {
            sqlx::query("INSERT INTO public_overview_urls (site_id, url) VALUES (?, ?)")
                .bind(&site.id)
                .bind(url)
                .execute(&mut *tx)
                .await?;
        }
    }
    audit::record_on(&mut tx, AuditEvent { action: "public_overview.updated", metadata: json!({"enabled": config.enabled, "metrics": config.metrics, "breakdowns": config.breakdowns, "max_days": config.max_days}), organization_id: site.organization_id.as_deref(), target_id: Some(&site.id), target_type: Some("site"), user_id: Some(&user) }).await?;
    tx.commit().await?;
    Ok(Json(json!({"config": config})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicQuery {
    site_id: Option<String>,
    site: Option<String>,
    days: Option<u16>,
}
#[derive(sqlx::FromRow)]
struct SharedSite {
    id: String,
    tracking_id: String,
    name: String,
}
async fn resolve(
    pool: &SqlitePool,
    query: &PublicQuery,
) -> Result<(SharedSite, ShareConfig, String), ApiError> {
    let (by_id, selector) = match (&query.site_id, &query.site) {
        (Some(id), None) if !id.is_empty() && id.len() <= 128 => (true, id.clone()),
        (None, Some(url)) => (false, normalize_url(url)?),
        _ => {
            return Err(ApiError::BadRequest(
                "Provide either site_id or site.".into(),
            ))
        }
    };
    let candidates = sqlx::query_as::<_, SharedSite>("SELECT s.id, s.tracking_id, s.name, s.organization_id FROM sites s JOIN organizations o ON o.id = s.organization_id JOIN public_overview_shares p ON p.site_id = s.id WHERE s.archived_at IS NULL AND s.deleted_at IS NULL AND s.erasure_pending_at IS NULL AND o.archived_at IS NULL AND ((? = 1 AND s.id = ?) OR (? = 0 AND EXISTS(SELECT 1 FROM public_overview_urls u WHERE u.site_id = s.id AND u.url = ?))) LIMIT 2")
        .bind(by_id).bind(&selector).bind(by_id).bind(&selector).fetch_all(pool).await?;
    if candidates.len() != 1 {
        return Err(unavailable());
    }
    let site = candidates.into_iter().next().unwrap();
    let (config, revision) = read_config(pool, &site.id).await?;
    if !config.enabled || !eligible(()) {
        return Err(unavailable());
    }
    if !by_id
        && (!config.urls.contains(&selector)
            || !available_urls(pool, &site.id).await?.contains(&selector))
    {
        return Err(unavailable());
    }
    Ok((site, config, revision))
}
pub(crate) async fn overview(
    State(state): State<AppState>,
    Query(query): Query<PublicQuery>,
) -> Result<Json<Value>, ApiError> {
    let (site, config, revision) = resolve(&state.sqlite, &query).await?;
    let days = query.days.unwrap_or(config.max_days);
    if ![7, 30, 90].contains(&days) || days > config.max_days {
        return Err(ApiError::BadRequest(
            "This history window is not shared.".into(),
        ));
    }
    state
        .rate_limiter
        .check(
            crate::rate_limit::RateLimitPolicy::public_overview_site(),
            &site.id,
        )
        .await?;
    let data = stats::public_overview(&state.clickhouse, &site.tracking_id, days, &config).await?;
    // Revocation, changed selections, archived apps, and expired plans take effect
    // even when they happen while ClickHouse is producing the response.
    let (_, current, current_revision) = resolve(&state.sqlite, &query).await?;
    if current_revision != revision || !current.enabled {
        return Err(unavailable());
    }
    Ok(Json(
        json!({"name": site.name, "max_days": config.max_days, "metrics": config.metrics, "data": data}),
    ))
}
