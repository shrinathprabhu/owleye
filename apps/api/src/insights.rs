use std::collections::BTreeMap;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{Duration, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth, demo,
    demo::{DemoSiteDefinition, DemoTrafficProfile},
    models::clickhouse_string,
    retention::ACTIVE_ROW_PREDICATE,
    sites, ApiError, AppState,
};

const DEFAULT_DAYS: u16 = 30;
const MAX_DAYS: u16 = 365;
const PERFORMANCE_METRICS: &[&str] = &["LCP", "INP", "CLS", "FCP", "TTFB"];

#[derive(Debug, Deserialize)]
pub(crate) struct RangeQuery {
    #[serde(default)]
    days: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PerformanceQuery {
    #[serde(default)]
    days: Option<u16>,
    #[serde(default)]
    metric: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateCampaignRequest {
    #[serde(default)]
    campaign_key: Option<String>,
    destination_url: String,
    medium: String,
    name: String,
    source: String,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub(crate) struct CampaignDefinition {
    campaign_key: String,
    created_at: String,
    destination_url: String,
    id: String,
    medium: String,
    name: String,
    source: String,
    tracking_url: String,
    updated_at: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct CampaignAnalytics {
    campaign_key: String,
    conversions: u64,
    medium: String,
    source: String,
    views: u64,
    visitors: u64,
}

#[derive(Debug, Serialize)]
struct CampaignItem {
    campaign_key: String,
    conversions: u64,
    created_at: Option<String>,
    destination_url: Option<String>,
    id: Option<String>,
    medium: String,
    name: String,
    source: String,
    tracking_url: Option<String>,
    updated_at: Option<String>,
    views: u64,
    visitors: u64,
}

#[derive(Debug, Serialize)]
struct CampaignTotals {
    campaigns: usize,
    conversions: u64,
    views: u64,
    visitors: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct CampaignsResponse {
    days: u16,
    items: Vec<CampaignItem>,
    site_id: String,
    totals: CampaignTotals,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PerformanceMetricSummary {
    metric: String,
    p50: f64,
    p75: f64,
    p95: f64,
    rating: String,
    samples: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PerformancePoint {
    date: String,
    p50: f64,
    p75: f64,
    p95: f64,
    samples: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PerformancePage {
    p75: f64,
    path: String,
    samples: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct PerformanceResponse {
    collecting: bool,
    days: u16,
    metric: String,
    metrics: Vec<PerformanceMetricSummary>,
    pages: Vec<PerformancePage>,
    series: Vec<PerformancePoint>,
    site_id: String,
}

pub(crate) async fn list_campaigns(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Query(query): Query<RangeQuery>,
) -> Result<Json<CampaignsResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    let days = query.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);
    if auth::is_demo_session(&session) {
        let site = demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Ok(Json(demo_campaigns(site, days)));
    }

    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    let definitions = campaign_definitions(&state, &site.id).await?;
    let analytics = state
        .clickhouse
        .query_json_each_row::<CampaignAnalytics>(&campaign_query(&site.tracking_id, days))
        .await?;
    Ok(Json(campaign_response(
        site.tracking_id,
        days,
        definitions,
        analytics,
    )))
}

pub(crate) async fn create_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<CreateCampaignRequest>,
) -> Result<Json<CampaignDefinition>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    let name = bounded_text("Campaign name", &request.name, 120)?;
    let source = bounded_text("Campaign source", &request.source, 80)?;
    let medium = bounded_text("Campaign medium", &request.medium, 80)?;
    let campaign_key = campaign_key(request.campaign_key.as_deref().unwrap_or(&name))?;
    let destination_url = destination_url(&request.destination_url)?;
    let tracking_url = tracking_url(&destination_url, &campaign_key, &source, &medium)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query_as::<_, CampaignDefinition>(
        r#"
        INSERT INTO campaigns (
            id, site_id, created_by_user_id, name, campaign_key, source,
            medium, destination_url, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id, name, campaign_key, source, medium, destination_url,
            ? AS tracking_url, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(site.id)
    .bind(session.id)
    .bind(name)
    .bind(campaign_key)
    .bind(source)
    .bind(medium)
    .bind(destination_url)
    .bind(&now)
    .bind(&now)
    .bind(tracking_url)
    .fetch_one(&state.sqlite)
    .await
    .map_err(|error| {
        if error.to_string().contains("UNIQUE constraint failed") {
            ApiError::Conflict("A campaign with this key already exists".to_owned())
        } else {
            ApiError::Database(error)
        }
    })?;
    Ok(Json(result))
}

pub(crate) async fn delete_campaign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, campaign_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    let result = sqlx::query("DELETE FROM campaigns WHERE id = ? AND site_id = ?")
        .bind(&campaign_id)
        .bind(&site.id)
        .execute(&state.sqlite)
        .await?;
    if result.rows_affected() != 1 {
        return Err(ApiError::BadRequest("Unknown campaign".to_owned()));
    }
    Ok(Json(json!({ "deleted": true, "id": campaign_id })))
}

pub(crate) async fn performance_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Query(query): Query<PerformanceQuery>,
) -> Result<Json<PerformanceResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    let days = query.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);
    let metric = normalize_metric(query.metric.as_deref())?;
    if auth::is_demo_session(&session) {
        let site = demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Ok(Json(demo_performance(site, days, metric)));
    }

    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    let summary_query = performance_summary_query(&site.tracking_id, days);
    let series_query = performance_series_query(&site.tracking_id, days, metric);
    let pages_query = performance_pages_query(&site.tracking_id, days, metric);
    let (mut metrics, series, pages) = tokio::try_join!(
        state
            .clickhouse
            .query_json_each_row::<PerformanceMetricSummary>(&summary_query),
        state
            .clickhouse
            .query_json_each_row::<PerformancePoint>(&series_query),
        state
            .clickhouse
            .query_json_each_row::<PerformancePage>(&pages_query),
    )?;
    for summary in &mut metrics {
        summary.rating = metric_rating(&summary.metric, summary.p75).to_owned();
    }
    metrics.sort_by_key(|summary| {
        PERFORMANCE_METRICS
            .iter()
            .position(|metric| *metric == summary.metric)
            .unwrap_or(usize::MAX)
    });
    Ok(Json(PerformanceResponse {
        collecting: metrics.iter().any(|summary| summary.samples > 0),
        days,
        metric: metric.to_owned(),
        metrics,
        pages,
        series,
        site_id: site.tracking_id,
    }))
}

async fn campaign_definitions(
    state: &AppState,
    site_id: &str,
) -> Result<Vec<CampaignDefinition>, ApiError> {
    let rows = sqlx::query_as::<_, CampaignDefinition>(
        r#"
        SELECT id, name, campaign_key, source, medium, destination_url,
            '' AS tracking_url, created_at, updated_at
        FROM campaigns
        WHERE site_id = ?
        ORDER BY updated_at DESC, name ASC
        "#,
    )
    .bind(site_id)
    .fetch_all(&state.sqlite)
    .await?;
    rows.into_iter()
        .map(|mut row| {
            row.tracking_url = tracking_url(
                &row.destination_url,
                &row.campaign_key,
                &row.source,
                &row.medium,
            )?;
            Ok(row)
        })
        .collect()
}

fn campaign_response(
    site_id: String,
    days: u16,
    definitions: Vec<CampaignDefinition>,
    analytics: Vec<CampaignAnalytics>,
) -> CampaignsResponse {
    let mut by_key = analytics
        .into_iter()
        .map(|row| (row.campaign_key.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let mut items = definitions
        .into_iter()
        .map(|definition| {
            let values = by_key.remove(&definition.campaign_key).unwrap_or_default();
            CampaignItem {
                campaign_key: definition.campaign_key,
                conversions: values.conversions,
                created_at: Some(definition.created_at),
                destination_url: Some(definition.destination_url),
                id: Some(definition.id),
                medium: definition.medium,
                name: definition.name,
                source: definition.source,
                tracking_url: Some(definition.tracking_url),
                updated_at: Some(definition.updated_at),
                views: values.views,
                visitors: values.visitors,
            }
        })
        .collect::<Vec<_>>();
    items.extend(by_key.into_values().map(|row| CampaignItem {
        campaign_key: row.campaign_key.clone(),
        conversions: row.conversions,
        created_at: None,
        destination_url: None,
        id: None,
        medium: row.medium,
        name: row.campaign_key,
        source: row.source,
        tracking_url: None,
        updated_at: None,
        views: row.views,
        visitors: row.visitors,
    }));
    items.sort_by(|left, right| {
        right
            .visitors
            .cmp(&left.visitors)
            .then_with(|| left.name.cmp(&right.name))
    });
    let totals = CampaignTotals {
        campaigns: items.len(),
        conversions: items.iter().map(|item| item.conversions).sum(),
        views: items.iter().map(|item| item.views).sum(),
        visitors: items.iter().map(|item| item.visitors).sum(),
    };
    CampaignsResponse {
        days,
        items,
        site_id,
        totals,
    }
}

fn campaign_query(site_id: &str, days: u16) -> String {
    let site_id = clickhouse_string(site_id);
    format!(
        r#"
        WITH attributed_sessions AS (
            SELECT
                anon_session_id,
                argMin(utm_campaign, occurred_at) AS campaign_key,
                argMin(utm_source, occurred_at) AS source,
                argMin(utm_medium, occurred_at) AS medium
            FROM owleye_events
            WHERE site_id = {site_id}
                AND occurred_at >= now() - INTERVAL {days} DAY
                AND event_type = 'pageview'
                AND utm_campaign != ''
                AND {active}
            GROUP BY anon_session_id
        )
        SELECT
            attributed_sessions.campaign_key AS campaign_key,
            any(attributed_sessions.source) AS source,
            any(attributed_sessions.medium) AS medium,
            uniqCombined64(events.visitor_id) AS visitors,
            toUInt64(countIf(events.event_type = 'pageview')) AS views,
            toUInt64(countIf(events.event_type IN ('external', 'rule'))) AS conversions
        FROM owleye_events AS events
        INNER JOIN attributed_sessions USING (anon_session_id)
        WHERE events.site_id = {site_id}
            AND events.occurred_at >= now() - INTERVAL {days} DAY
            AND {active_events}
        GROUP BY campaign_key
        ORDER BY visitors DESC, campaign_key ASC
        LIMIT 200
        "#,
        active = ACTIVE_ROW_PREDICATE,
        active_events = ACTIVE_ROW_PREDICATE.replace("retention_", "events.retention_"),
    )
}

fn performance_summary_query(site_id: &str, days: u16) -> String {
    format!(
        r#"
        WITH {metric} AS metric, {value} AS value
        SELECT metric, toFloat64(quantileTDigest(0.50)(value)) AS p50,
            toFloat64(quantileTDigest(0.75)(value)) AS p75,
            toFloat64(quantileTDigest(0.95)(value)) AS p95,
            toUInt64(count()) AS samples, '' AS rating
        FROM owleye_events
        WHERE site_id = {site_id} AND occurred_at >= now() - INTERVAL {days} DAY
            AND event_type = 'performance' AND startsWith(event_name, 'web_vital_')
            AND value >= 0 AND {active}
        GROUP BY metric
        SETTINGS output_format_json_quote_64bit_integers = 0
        "#,
        metric = performance_metric_expression(),
        value = performance_value_expression(),
        site_id = clickhouse_string(site_id),
        active = ACTIVE_ROW_PREDICATE,
    )
}

fn performance_series_query(site_id: &str, days: u16, metric_name: &str) -> String {
    format!(
        r#"
        WITH {value} AS value
        SELECT toString(toDate(occurred_at)) AS date,
            toFloat64(quantileTDigest(0.50)(value)) AS p50,
            toFloat64(quantileTDigest(0.75)(value)) AS p75,
            toFloat64(quantileTDigest(0.95)(value)) AS p95,
            toUInt64(count()) AS samples
        FROM owleye_events
        WHERE site_id = {site_id} AND occurred_at >= now() - INTERVAL {days} DAY
            AND event_type = 'performance' AND event_name = {event_name}
            AND value >= 0 AND {active}
        GROUP BY date ORDER BY date ASC
        SETTINGS output_format_json_quote_64bit_integers = 0
        "#,
        value = performance_value_expression(),
        site_id = clickhouse_string(site_id),
        event_name = clickhouse_string(&format!("web_vital_{}", metric_name.to_lowercase())),
        active = ACTIVE_ROW_PREDICATE,
    )
}

fn performance_pages_query(site_id: &str, days: u16, metric_name: &str) -> String {
    format!(
        r#"
        WITH {value} AS value
        SELECT if(url_path = '', '/', url_path) AS path,
            toFloat64(quantileTDigest(0.75)(value)) AS p75,
            toUInt64(count()) AS samples
        FROM owleye_events
        WHERE site_id = {site_id} AND occurred_at >= now() - INTERVAL {days} DAY
            AND event_type = 'performance' AND event_name = {event_name}
            AND value >= 0 AND {active}
        GROUP BY path HAVING samples >= 3
        ORDER BY p75 DESC, samples DESC LIMIT 20
        SETTINGS output_format_json_quote_64bit_integers = 0
        "#,
        value = performance_value_expression(),
        site_id = clickhouse_string(site_id),
        event_name = clickhouse_string(&format!("web_vital_{}", metric_name.to_lowercase())),
        active = ACTIVE_ROW_PREDICATE,
    )
}

fn performance_metric_expression() -> &'static str {
    "upper(replaceOne(event_name, 'web_vital_', ''))"
}

fn performance_value_expression() -> &'static str {
    "if(event_name = 'web_vital_cls', JSONExtractFloat(payload_json, 'value'), if(isNull(duration_ms), JSONExtractFloat(payload_json, 'value'), toFloat64(duration_ms)))"
}

fn normalize_metric(metric: Option<&str>) -> Result<&'static str, ApiError> {
    let metric = metric.unwrap_or("LCP").trim().to_ascii_uppercase();
    PERFORMANCE_METRICS
        .iter()
        .copied()
        .find(|candidate| *candidate == metric)
        .ok_or_else(|| {
            ApiError::BadRequest("metric must be LCP, INP, CLS, FCP, or TTFB".to_owned())
        })
}

fn bounded_text(label: &str, value: &str, max: usize) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(format!(
            "{label} must be between 1 and {max} characters"
        )));
    }
    Ok(value.to_owned())
}

fn campaign_key(value: &str) -> Result<String, ApiError> {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let normalized = normalized.trim_matches('-').to_owned();
    if normalized.is_empty() || normalized.len() > 128 {
        return Err(ApiError::BadRequest(
            "Campaign key must contain letters or numbers and be at most 128 characters".to_owned(),
        ));
    }
    Ok(normalized)
}

fn destination_url(value: &str) -> Result<String, ApiError> {
    let mut url = Url::parse(value.trim()).map_err(|_| {
        ApiError::BadRequest("Destination must be an absolute HTTP(S) URL".to_owned())
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ApiError::BadRequest(
            "Destination must be an HTTP(S) URL without credentials".to_owned(),
        ));
    }
    url.set_fragment(None);
    Ok(url.to_string())
}

fn tracking_url(
    destination: &str,
    campaign_key: &str,
    source: &str,
    medium: &str,
) -> Result<String, ApiError> {
    let mut url = Url::parse(destination)
        .map_err(|_| ApiError::BadRequest("Stored campaign destination is invalid".to_owned()))?;
    let retained = url
        .query_pairs()
        .filter(|(key, _)| !matches!(key.as_ref(), "utm_source" | "utm_medium" | "utm_campaign"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in retained {
            query.append_pair(&key, &value);
        }
        query.append_pair("utm_source", source);
        query.append_pair("utm_medium", medium);
        query.append_pair("utm_campaign", campaign_key);
    }
    Ok(url.to_string())
}

fn metric_rating(metric: &str, value: f64) -> &'static str {
    let (good, poor) = match metric {
        "CLS" => (0.1, 0.25),
        "INP" => (200.0, 500.0),
        "FCP" => (1_800.0, 3_000.0),
        "TTFB" => (800.0, 1_800.0),
        _ => (2_500.0, 4_000.0),
    };
    if value <= good {
        "good"
    } else if value <= poor {
        "needs_improvement"
    } else {
        "poor"
    }
}

fn demo_campaigns(site: &DemoSiteDefinition, days: u16) -> CampaignsResponse {
    let base = match site.profile {
        DemoTrafficProfile::Massive => 12_600,
        DemoTrafficProfile::Campaign => 2_360,
        DemoTrafficProfile::Mixed => 920,
        DemoTrafficProfile::LowVolume => 180,
        DemoTrafficProfile::New => 24,
    } * u64::from(days)
        / 30;
    let definitions = [
        (
            "Privacy analytics",
            "privacy-analytics",
            "newsletter",
            "email",
            100,
        ),
        ("Launch week", "launch-week", "product-hunt", "social", 88),
        ("Founder post", "founder-post", "linkedin", "social", 72),
        ("Launch day", "launch-day", "partners", "referral", 36),
        ("Agency bundle", "agency-bundle", "agency", "partner", 24),
    ];
    let items = definitions
        .into_iter()
        .map(|(name, key, source, medium, weight)| {
            let visitors = base.saturating_mul(weight) / 100;
            CampaignItem {
                campaign_key: key.to_owned(),
                conversions: visitors.saturating_mul(14 + weight / 10) / 100,
                created_at: Some("2026-07-01T10:00:00Z".to_owned()),
                destination_url: Some(format!("https://{}/", site.domain)),
                id: Some(format!("demo-campaign-{key}")),
                medium: medium.to_owned(),
                name: name.to_owned(),
                source: source.to_owned(),
                tracking_url: tracking_url(
                    &format!("https://{}/", site.domain),
                    key,
                    source,
                    medium,
                )
                .ok(),
                updated_at: Some("2026-08-28T10:00:00Z".to_owned()),
                views: visitors.saturating_mul(23) / 10,
                visitors,
            }
        })
        .collect::<Vec<_>>();
    let totals = CampaignTotals {
        campaigns: items.len(),
        conversions: items.iter().map(|item| item.conversions).sum(),
        views: items.iter().map(|item| item.views).sum(),
        visitors: items.iter().map(|item| item.visitors).sum(),
    };
    CampaignsResponse {
        days,
        items,
        site_id: site.tracking_id.to_owned(),
        totals,
    }
}

fn demo_performance(
    site: &DemoSiteDefinition,
    days: u16,
    selected_metric: &str,
) -> PerformanceResponse {
    let metrics = [
        ("LCP", 2_360.0),
        ("INP", 184.0),
        ("CLS", 0.082),
        ("FCP", 1_420.0),
        ("TTFB", 690.0),
    ]
    .into_iter()
    .map(|(metric, p75)| PerformanceMetricSummary {
        metric: metric.to_owned(),
        p50: p75 * 0.78,
        p75,
        p95: p75 * 1.48,
        rating: metric_rating(metric, p75).to_owned(),
        samples: 284,
    })
    .collect::<Vec<_>>();
    let selected = metrics
        .iter()
        .find(|summary| summary.metric == selected_metric)
        .map(|summary| summary.p75)
        .unwrap_or(1.0);
    let point_count = usize::from(days.min(30));
    let today = Utc::now().date_naive();
    let series = (0..point_count)
        .map(|index| {
            let phase = ((index * 17 + 9) % 13) as f64 / 100.0;
            let p75 = selected * (0.92 + phase);
            PerformancePoint {
                date: (today - Duration::days((point_count - index - 1) as i64)).to_string(),
                p50: p75 * 0.78,
                p75,
                p95: p75 * 1.42,
                samples: 8 + ((index * 7) % 19) as u64,
            }
        })
        .collect();
    let pages = ["/checkout", "/pricing", "/docs", "/"]
        .into_iter()
        .enumerate()
        .map(|(index, path)| PerformancePage {
            p75: selected * (1.42 - index as f64 * 0.12),
            path: path.to_owned(),
            samples: 18 + index as u64 * 7,
        })
        .collect();
    PerformanceResponse {
        collecting: true,
        days,
        metric: selected_metric.to_owned(),
        metrics,
        pages,
        series,
        site_id: site.tracking_id.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_links_preserve_safe_query_and_replace_attribution() {
        let url = tracking_url(
            "https://example.test/pricing?plan=pro",
            "launch-week",
            "newsletter",
            "email",
        )
        .unwrap();
        assert!(url.contains("plan=pro"));
        assert!(url.contains("utm_campaign=launch-week"));
        assert!(url.contains("utm_source=newsletter"));
    }

    #[test]
    fn performance_queries_are_scoped_and_read_payload_values() {
        let query = performance_summary_query("site'quoted", 30);
        assert!(query.contains("site_id = 'site\\'quoted'"));
        assert!(query.contains("JSONExtractFloat(payload_json, 'value')"));
        assert!(query.contains(ACTIVE_ROW_PREDICATE));
    }

    #[tokio::test]
    #[ignore = "requires a disposable loopback ClickHouse instance"]
    async fn real_clickhouse_performance_handles_samples_sparse_and_empty_results() {
        use crate::storage::clickhouse::ClickHouse;
        let base = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("loopback ClickHouse URL");
        let mut url = reqwest::Url::parse(&base).unwrap();
        assert!(matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        ));
        let db = format!("performance_test_{}", Uuid::new_v4().simple());
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        async fn execute(client: &reqwest::Client, url: &reqwest::Url, sql: String) {
            let response = client.post(url.clone()).body(sql).send().await.unwrap();
            assert!(
                response.status().is_success(),
                "{}",
                response.text().await.unwrap()
            );
        }
        execute(&client, &url, format!("CREATE DATABASE {db}")).await;
        url.query_pairs_mut().clear().append_pair("database", &db);
        execute(&client, &url, "CREATE TABLE owleye_events (site_id String, occurred_at DateTime64(3,'UTC'), retention_active_until Nullable(DateTime64(3,'UTC')), event_type String, event_name String, duration_ms Nullable(UInt64), payload_json String, url_path String) ENGINE=MergeTree ORDER BY (site_id,occurred_at)".into()).await;
        execute(&client, &url, "INSERT INTO owleye_events SELECT 'site', now64(3)-INTERVAL 1 HOUR, now64(3)+INTERVAL 10 DAY, 'performance', 'web_vital_lcp', 1200, '{}', '/pricing' FROM numbers(6)".into()).await;
        execute(&client, &url, r#"INSERT INTO owleye_events VALUES ('site',now64(3)-INTERVAL 1 HOUR,now64(3)+INTERVAL 10 DAY,'performance','web_vital_cls',NULL,'{"value":0.125}','/pricing'), ('site',now64(3)-INTERVAL 1 HOUR,now64(3)+INTERVAL 10 DAY,'performance','web_vital_ttfb',250,'{}','/pricing'), ('other',now64(3)-INTERVAL 1 HOUR,now64(3)+INTERVAL 10 DAY,'performance','web_vital_lcp',9999,'{}','/private'), ('site',now64(3)-INTERVAL 1 HOUR,now64(3)-INTERVAL 1 MINUTE,'performance','web_vital_lcp',9999,'{}','/expired')"#.into()).await;
        // The real server may default to quoted UInt64 JSON; query settings must
        // make all three result shapes deserialize without narrowing counters.
        url.query_pairs_mut()
            .append_pair("output_format_json_quote_64bit_integers", "1");
        let clickhouse = ClickHouse::new(url.to_string()).unwrap();
        let outcome = async {
            let summary = clickhouse
                .query_json_each_row::<PerformanceMetricSummary>(&performance_summary_query(
                    "site", 30,
                ))
                .await?;
            assert_eq!(summary.len(), 3);
            let lcp = summary.iter().find(|m| m.metric == "LCP").unwrap();
            assert_eq!((lcp.samples, lcp.p75), (6, 1200.0));
            assert_eq!(
                summary.iter().find(|m| m.metric == "CLS").unwrap().p75,
                0.125
            );
            let series = clickhouse
                .query_json_each_row::<PerformancePoint>(&performance_series_query(
                    "site", 30, "TTFB",
                ))
                .await?;
            assert_eq!(series.len(), 1);
            assert_eq!((series[0].samples, series[0].p75), (1, 250.0));
            let pages = clickhouse
                .query_json_each_row::<PerformancePage>(&performance_pages_query("site", 30, "LCP"))
                .await?;
            assert_eq!(pages.len(), 1);
            assert_eq!((pages[0].path.as_str(), pages[0].samples), ("/pricing", 6));
            assert!(clickhouse
                .query_json_each_row::<PerformancePage>(&performance_pages_query(
                    "site", 30, "TTFB"
                ))
                .await?
                .is_empty());
            assert!(clickhouse
                .query_json_each_row::<PerformancePoint>(&performance_series_query(
                    "site", 30, "INP"
                ))
                .await?
                .is_empty());
            assert!(clickhouse
                .query_json_each_row::<PerformanceMetricSummary>(&performance_summary_query(
                    "empty", 30
                ))
                .await?
                .is_empty());
            Ok::<_, anyhow::Error>(())
        }
        .await;
        execute(&client, &url, format!("DROP DATABASE {db}")).await;
        outcome.unwrap();
    }

    #[test]
    fn web_vital_thresholds_match_field_guidance() {
        assert_eq!(metric_rating("LCP", 2_500.0), "good");
        assert_eq!(metric_rating("LCP", 3_000.0), "needs_improvement");
        assert_eq!(metric_rating("CLS", 0.3), "poor");
    }
}
