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
    #[serde(default)]
    term: String,
    #[serde(default)]
    content: String,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub(crate) struct CampaignDefinition {
    term: String,
    content: String,
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
    #[serde(default = "campaign_row_kind")]
    kind: String,
    #[serde(default)]
    term: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    variant_count: u64,
    campaign_key: String,
    conversions: u64,
    medium: String,
    source: String,
    views: u64,
    visitors: u64,
}

fn campaign_row_kind() -> String {
    "campaign".into()
}
#[derive(Debug, Serialize)]
struct CampaignVariant {
    source: String,
    medium: String,
    term: String,
    content: String,
    conversions: u64,
    views: u64,
    visitors: u64,
}
#[derive(Debug, Serialize)]
struct CampaignItem {
    term: String,
    content: String,
    variants: Vec<CampaignVariant>,
    variants_withheld: bool,
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
    #[serde(default)]
    good_samples: Option<u64>,
    #[serde(default)]
    needs_improvement_samples: Option<u64>,
    #[serde(default)]
    poor_samples: Option<u64>,
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
        .with_report_scope(&site.tracking_id, 1, async {
            state
                .clickhouse
                .query_report::<CampaignAnalytics>(&campaign_query(&site.tracking_id, days))
                .await
        })
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
    let name = campaign_label("Campaign name", &request.name, 120, false)?;
    let source = campaign_label("Campaign source", &request.source, 80, false)?;
    let medium = campaign_label("Campaign medium", &request.medium, 80, false)?;
    let term = campaign_label("UTM term", &request.term, 200, true)?;
    let content = campaign_label("UTM content", &request.content, 200, true)?;
    let raw_key = request.campaign_key.as_deref().unwrap_or(&name);
    campaign_label("Campaign key", raw_key, 128, false)?;
    let campaign_key = campaign_key(raw_key)?;
    let destination_url = destination_url(&request.destination_url)?;
    let tracking_url = tracking_url(
        &destination_url,
        &campaign_key,
        &source,
        &medium,
        &term,
        &content,
    )?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query_as::<_, CampaignDefinition>(
        r#"
        INSERT INTO campaigns (
            id, site_id, created_by_user_id, name, campaign_key, source,
            medium, term, content, destination_url, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id, name, campaign_key, source, medium, term, content, destination_url,
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
    .bind(term)
    .bind(content)
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
    let (mut metrics, series, pages) = state
        .clickhouse
        .with_report_scope(&site.tracking_id, 3, async {
            tokio::try_join!(
                state
                    .clickhouse
                    .query_report::<PerformanceMetricSummary>(&summary_query),
                state
                    .clickhouse
                    .query_report::<PerformancePoint>(&series_query),
                state
                    .clickhouse
                    .query_report::<PerformancePage>(&pages_query),
            )
        })
        .await?;
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
        SELECT id, name, campaign_key, source, medium, term, content, destination_url,
            '' AS tracking_url, created_at, updated_at
        FROM campaigns
        WHERE site_id = ?
        ORDER BY updated_at DESC, name ASC LIMIT 200
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
                &row.term,
                &row.content,
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
    let mut by_key = BTreeMap::new();
    let mut variants: BTreeMap<String, Vec<CampaignVariant>> = BTreeMap::new();
    let mut overall = None;
    for row in analytics {
        match row.kind.as_str() {
            "total" => overall = Some((row.conversions, row.views, row.visitors)),
            "variant" => {
                let safe = |value: &str| {
                    value.is_empty()
                        || crate::privacy::attribution::campaign_value(value).as_deref()
                            == Some(value)
                };
                if row.visitors >= 5
                    && [
                        &row.campaign_key,
                        &row.source,
                        &row.medium,
                        &row.term,
                        &row.content,
                    ]
                    .into_iter()
                    .all(|s| safe(s))
                {
                    variants
                        .entry(row.campaign_key)
                        .or_default()
                        .push(CampaignVariant {
                            source: row.source,
                            medium: row.medium,
                            term: row.term,
                            content: row.content,
                            conversions: row.conversions,
                            views: row.views,
                            visitors: row.visitors,
                        });
                }
            }
            _ => {
                by_key.insert(row.campaign_key.clone(), row);
            }
        }
    }
    let mut items = definitions
        .into_iter()
        .map(|definition| {
            let values = by_key.remove(&definition.campaign_key).unwrap_or_default();
            let released = variants
                .remove(&definition.campaign_key)
                .unwrap_or_default();
            CampaignItem {
                term: definition.term,
                content: definition.content,
                variants_withheld: values.variant_count > released.len() as u64,
                variants: released,
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
    items.extend(by_key.into_values().map(|row| {
        let released = variants.remove(&row.campaign_key).unwrap_or_default();
        CampaignItem {
            term: String::new(),
            content: String::new(),
            variants_withheld: row.variant_count > released.len() as u64,
            variants: released,
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
        }
    }));
    items.sort_by(|left, right| {
        right
            .visitors
            .cmp(&left.visitors)
            .then_with(|| left.name.cmp(&right.name))
    });
    let totals = CampaignTotals {
        campaigns: items.len(),
        conversions: overall
            .map(|t| t.0)
            .unwrap_or_else(|| items.iter().map(|item| item.conversions).sum()),
        views: overall
            .map(|t| t.1)
            .unwrap_or_else(|| items.iter().map(|item| item.views).sum()),
        visitors: overall
            .map(|t| t.2)
            .unwrap_or_else(|| items.iter().map(|item| item.visitors).sum()),
    };
    CampaignsResponse {
        days,
        items,
        site_id,
        totals,
    }
}

fn campaign_query(site_id: &str, days: u16) -> String {
    let site = clickhouse_string(site_id);
    let active = ACTIVE_ROW_PREDICATE;
    let active_events = ACTIVE_ROW_PREDICATE.replace("retention_", "events.retention_");
    format!(
        r#"WITH attributed_sessions AS (
      SELECT anon_session_id,argMin(tuple(utm_campaign,utm_source,utm_medium,utm_term,utm_content),tuple(occurred_at,toString(event_id))) AS attribution
      FROM owleye_events WHERE site_id={site} AND occurred_at>=now()-INTERVAL {days} DAY
        AND event_type='pageview' AND utm_campaign!='' AND anon_session_id!='' AND {active}
      GROUP BY anon_session_id
    ), scoped AS (
      SELECT attribution.1 AS campaign_key,attribution.2 AS source,attribution.3 AS medium,
        attribution.4 AS term,attribution.5 AS content,
        events.visitor_id AS visitor_id,events.event_type AS event_type
      FROM owleye_events AS events INNER JOIN attributed_sessions USING(anon_session_id)
      WHERE events.site_id={site} AND events.occurred_at>=now()-INTERVAL {days} DAY AND {active_events}
    ), grouped AS (
      SELECT multiIf(grouping(campaign_key)=1,'total',grouping(source)=1,'campaign','variant') AS kind,
        campaign_key,source,medium,term,content,
        any(scoped.source) AS first_source,any(scoped.medium) AS first_medium,
        toUInt64(uniqExact(tuple(scoped.source,scoped.medium,scoped.term,scoped.content))) AS variant_count,
        toUInt64(countIf(event_type IN ('external','rule'))) AS conversions,
        toUInt64(countIf(event_type='pageview')) AS views,
        toUInt64(uniqCombined64If(visitor_id,visitor_id!='')) AS visitors
      FROM scoped
      GROUP BY GROUPING SETS ((),(campaign_key),(campaign_key,source,medium,term,content))
      HAVING grouping(source)=1 OR uniqExactIf(visitor_id,visitor_id!='')>=5
    ), ranked AS (
      SELECT *,row_number() OVER(PARTITION BY kind ORDER BY visitors DESC,campaign_key,source,medium,term,content) AS kind_rank,
        row_number() OVER(PARTITION BY kind,campaign_key ORDER BY visitors DESC,source,medium,term,content) AS campaign_rank
      FROM grouped
    )
    SELECT kind,campaign_key,if(kind='campaign',first_source,ranked.source) AS source,
      if(kind='campaign',first_medium,ranked.medium) AS medium,term,content,
      if(kind='variant',toUInt64(0),ranked.variant_count) AS variant_count,conversions,views,visitors
    FROM ranked WHERE kind='total' OR (kind='campaign' AND kind_rank<=200)
      OR (kind='variant' AND kind_rank<=200 AND campaign_rank<=20)
    ORDER BY kind,campaign_key,visitors DESC,source,medium,term,content
    SETTINGS group_by_use_nulls=0,output_format_json_quote_64bit_integers=0"#
    )
}

fn performance_summary_query(site_id: &str, days: u16) -> String {
    format!(
        r#"
        WITH {metric} AS metric, {value} AS value,
            multiIf(metric='CLS',0.1,metric='INP',200,metric='LCP',2500,metric='FCP',1800,800) AS good_limit,
            multiIf(metric='CLS',0.25,metric='INP',500,metric='LCP',4000,metric='FCP',3000,1800) AS poor_limit,
            (uniqUpToIf(4)(visitor_id,visitor_id!='')>=5
                AND (countIf(value<=good_limit)=0 OR uniqUpToIf(4)(visitor_id,visitor_id!='' AND value<=good_limit)>=5)
                AND (countIf(value>good_limit AND value<=poor_limit)=0 OR uniqUpToIf(4)(visitor_id,visitor_id!='' AND value>good_limit AND value<=poor_limit)>=5)
                AND (countIf(value>poor_limit)=0 OR uniqUpToIf(4)(visitor_id,visitor_id!='' AND value>poor_limit)>=5)) AS release_distribution
        SELECT metric, toFloat64(quantileTDigest(0.50)(value)) AS p50,
            toFloat64(quantileTDigest(0.75)(value)) AS p75,
            toFloat64(quantileTDigest(0.95)(value)) AS p95,
            toUInt64(count()) AS samples, '' AS rating,
            if(release_distribution,toUInt64(countIf(value<=good_limit)),NULL) AS good_samples,
            if(release_distribution,toUInt64(countIf(value>good_limit AND value<=poor_limit)),NULL) AS needs_improvement_samples,
            if(release_distribution,toUInt64(countIf(value>poor_limit)),NULL) AS poor_samples
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

fn campaign_label(
    field: &str,
    value: &str,
    max: usize,
    optional: bool,
) -> Result<String, ApiError> {
    if optional && value.trim().is_empty() {
        return Ok(String::new());
    }
    if value.trim().len() > max {
        return Err(ApiError::BadRequest(format!(
            "{field} must be at most {max} bytes"
        )));
    }
    crate::privacy::attribution::campaign_value(value).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "{field} must be a marketing label without personal identifiers, URLs, or secrets"
        ))
    })
}

fn tracking_url(
    destination: &str,
    campaign_key: &str,
    source: &str,
    medium: &str,
    term: &str,
    content: &str,
) -> Result<String, ApiError> {
    let mut url = Url::parse(destination)
        .map_err(|_| ApiError::BadRequest("Stored campaign destination is invalid".to_owned()))?;
    let retained = url
        .query_pairs()
        .filter(|(key, _)| {
            !crate::privacy::attribution::UTM_KEYS.contains(&key.to_ascii_lowercase().as_str())
        })
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
        if !term.is_empty() {
            query.append_pair("utm_term", term);
        }
        if !content.is_empty() {
            query.append_pair("utm_content", content);
        }
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
                term: String::new(),
                content: String::new(),
                variants: vec![],
                variants_withheld: false,
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
                    "",
                    "",
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
        good_samples: None,
        needs_improvement_samples: None,
        poor_samples: None,
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
    fn five_tag_links_replace_case_insensitive_duplicates_and_reject_sensitive_labels() {
        let url = tracking_url(
            "https://example.test/?plan=pro&UTM_TERM=old&utm_content=old",
            "launch",
            "newsletter",
            "email",
            "privacy analytics",
            "hero",
        )
        .unwrap();
        let parsed = Url::parse(&url).unwrap();
        let pairs = parsed.query_pairs().collect::<Vec<_>>();
        assert_eq!(
            pairs
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("utm_term"))
                .count(),
            1
        );
        assert_eq!(
            pairs.iter().find(|(k, _)| k == "utm_term").unwrap().1,
            "privacy analytics"
        );
        assert!(url.contains("utm_content=hero") && url.contains("plan=pro"));
        for value in [
            "person@example.test",
            "recipient-id-42",
            "https://example.test/post",
            "user%40example.test",
        ] {
            assert!(campaign_label("term", value, 200, true).is_err(), "{value}");
        }
        assert!(campaign_label("content", &"é".repeat(101), 200, true).is_err());
        let old:CreateCampaignRequest=serde_json::from_value(json!({"destination_url":"https://example.test/","source":"newsletter","medium":"email","name":"Launch"})).unwrap();
        assert!(old.term.is_empty() && old.content.is_empty());
    }
    #[test]
    fn campaign_totals_are_independent_of_variant_rows_and_distinct_visitors_are_not_summed() {
        let rows=vec![
            json!({"kind":"total","campaign_key":"","source":"","medium":"","conversions":5,"views":18,"visitors":8}),
            json!({"kind":"campaign","campaign_key":"a","source":"newsletter","medium":"email","conversions":3,"views":10,"visitors":6,"variant_count":2}),
            json!({"kind":"campaign","campaign_key":"b","source":"newsletter","medium":"email","conversions":2,"views":8,"visitors":6,"variant_count":1}),
            json!({"kind":"variant","campaign_key":"a","source":"newsletter","medium":"email","term":"privacy analytics","content":"hero","conversions":2,"views":6,"visitors":6}),
        ].into_iter().map(|v|serde_json::from_value(v).unwrap()).collect();
        let response = campaign_response("site".into(), 30, vec![], rows);
        assert_eq!(response.totals.visitors, 8);
        assert_eq!(response.totals.views, 18);
        assert_eq!(response.items[0].visitors + response.items[1].visitors, 12);
        let a = response
            .items
            .iter()
            .find(|i| i.campaign_key == "a")
            .unwrap();
        assert!(a.variants_withheld);
        assert_eq!(a.variants.len(), 1);
        assert_eq!(a.variants[0].content, "hero");
    }
    #[test]
    fn campaign_links_preserve_safe_query_and_replace_attribution() {
        let url = tracking_url(
            "https://example.test/pricing?plan=pro",
            "launch-week",
            "newsletter",
            "email",
            "",
            "",
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
    async fn real_clickhouse_five_tag_campaigns_preserve_totals_distincts_and_sparse_variants() {
        use crate::storage::clickhouse::ClickHouse;
        let base = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("loopback ClickHouse URL");
        let mut url = Url::parse(&base).unwrap();
        assert!(matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        ));
        let db = format!("campaign_test_{}", Uuid::new_v4().simple());
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        async fn execute(client: &reqwest::Client, url: &Url, sql: String) {
            let response = client.post(url.clone()).body(sql).send().await.unwrap();
            assert!(
                response.status().is_success(),
                "{}",
                response.text().await.unwrap()
            );
        }
        execute(&client, &url, format!("CREATE DATABASE {db}")).await;
        url.query_pairs_mut().clear().append_pair("database", &db);
        let ch = ClickHouse::new(url.to_string()).unwrap();
        ch.init().await.unwrap();
        // Ten campaign-A cohorts: a released six-visitor hero variant and a
        // withheld four-visitor footer variant. All have a separate browser
        // referrer, and conversions inherit the first page's coherent tuple.
        execute(&client,&url,r#"INSERT INTO owleye_events
          (event_id,site_id,occurred_at,retention_active_until,event_type,event_name,anon_session_id,visitor_id,utm_campaign,utm_source,utm_medium,utm_term,utm_content,referrer_host,url_host)
          SELECT generateUUIDv4(event_type),'campaign-site',now64(3)-INTERVAL 2 HOUR+toIntervalSecond(event_type='external'),now64(3)+INTERVAL 10 DAY,
            event_type,event_type,concat('a-',toString(number)),concat('visitor-',toString(number)),
            if(event_type='pageview','launch',''),if(event_type='pageview','newsletter',''),if(event_type='pageview','email',''),
            if(event_type='pageview','privacy analytics',''),if(event_type='pageview',if(number<6,'hero','footer'),''),
            'reddit.com','site.example'
          FROM numbers(10) ARRAY JOIN ['pageview','external'] AS event_type"#.into()).await;
        // A second campaign has six visitors who overlap campaign A. Overall
        // distinct visitors remain ten rather than the sum sixteen.
        execute(&client,&url,r#"INSERT INTO owleye_events
          (event_id,site_id,occurred_at,retention_active_until,event_type,anon_session_id,visitor_id,utm_campaign,utm_source,utm_medium,utm_term,utm_content)
          SELECT generateUUIDv4(),'campaign-site',now64(3)-INTERVAL 1 HOUR,now64(3)+INTERVAL 10 DAY,'pageview',concat('b-',toString(number)),concat('visitor-',toString(number)),
            'spring','google','cpc','analytics tool','banner' FROM numbers(6)"#.into()).await;
        // Expired, another tenant, and unattributable legacy empty-session
        // rows must not join a real cohort or inflate report totals.
        execute(&client,&url,r#"INSERT INTO owleye_events
          (event_id,site_id,occurred_at,retention_active_until,event_type,anon_session_id,visitor_id,utm_campaign,utm_source,utm_medium)
          SELECT generateUUIDv4(),if(number=1,'other-site','campaign-site'),now64(3)-INTERVAL 1 HOUR,
            if(number=0,now64(3)-INTERVAL 1 MINUTE,now64(3)+INTERVAL 10 DAY),'pageview',if(number=2,'','a-0'),concat('private-',toString(number)),'private-campaign','private','private' FROM numbers(3)"#.into()).await;
        let result = ch
            .with_report_scope("campaign-site", 1, async {
                ch.query_report::<CampaignAnalytics>(&campaign_query("campaign-site", 30))
                    .await
            })
            .await;
        let rows = result.unwrap();
        let response = campaign_response("campaign-site".into(), 30, vec![], rows);
        assert_eq!(
            (
                response.totals.views,
                response.totals.visitors,
                response.totals.conversions
            ),
            (16, 10, 10)
        );
        assert_eq!(response.items.len(), 2);
        let launch = response
            .items
            .iter()
            .find(|item| item.campaign_key == "launch")
            .unwrap();
        assert_eq!(
            (launch.views, launch.visitors, launch.conversions),
            (10, 10, 10)
        );
        assert!(launch.variants_withheld);
        assert_eq!(launch.variants.len(), 1);
        let hero = &launch.variants[0];
        assert_eq!((hero.views, hero.visitors, hero.conversions), (6, 6, 6));
        assert_eq!(
            (hero.term.as_str(), hero.content.as_str()),
            ("privacy analytics", "hero")
        );
        let spring = response
            .items
            .iter()
            .find(|item| item.campaign_key == "spring")
            .unwrap();
        assert!(!spring.variants_withheld);
        assert_eq!(spring.variants.len(), 1);
        assert_eq!(spring.variants[0].content, "banner");
        let empty = ch
            .query_report::<CampaignAnalytics>(&campaign_query("empty-site", 30))
            .await
            .unwrap();
        let empty = campaign_response("empty-site".into(), 30, vec![], empty);
        assert_eq!(
            (empty.totals.views, empty.totals.visitors, empty.items.len()),
            (0, 0, 0)
        );
        ch.shutdown().await;
        url.query_pairs_mut().clear();
        execute(&client, &url, format!("DROP DATABASE {db}")).await;
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
        // Exercise the same tables and compatibility view as a deployed API.
        let clickhouse = ClickHouse::new(url.to_string()).unwrap();
        clickhouse.init().await.unwrap();
        execute(&client, &url, "INSERT INTO owleye_events (site_id, occurred_at, retention_active_until, event_type, event_name, duration_ms, payload_json, url_path) SELECT 'site', now64(3)-INTERVAL 1 HOUR, now64(3)+INTERVAL 10 DAY, 'performance', 'web_vital_lcp', 1200, '{}', '/pricing' FROM numbers(3)".into()).await;
        execute(&client, &url, "INSERT INTO owleye_performance (site_id, occurred_at, retention_active_until, event_type, event_name, duration_ms, payload_json, url_path) SELECT 'site', now64(3)-INTERVAL 1 HOUR, now64(3)+INTERVAL 10 DAY, 'performance', 'web_vital_lcp', 1200, '{}', '/pricing' FROM numbers(3)".into()).await;
        execute(&client, &url, r#"INSERT INTO owleye_performance (site_id, occurred_at, retention_active_until, event_type, event_name, duration_ms, payload_json, url_path) VALUES ('site',now64(3)-INTERVAL 1 HOUR,now64(3)+INTERVAL 10 DAY,'performance','web_vital_cls',NULL,'{"value":0.125}','/pricing'), ('site',now64(3)-INTERVAL 1 HOUR,now64(3)+INTERVAL 10 DAY,'performance','web_vital_ttfb',250,'{}','/pricing'), ('other',now64(3)-INTERVAL 1 HOUR,now64(3)+INTERVAL 10 DAY,'performance','web_vital_lcp',9999,'{}','/private'), ('site',now64(3)-INTERVAL 1 HOUR,now64(3)-INTERVAL 1 MINUTE,'performance','web_vital_lcp',9999,'{}','/expired')"#.into()).await;
        // The real server may default to quoted UInt64 JSON; query settings must
        // make all three result shapes deserialize without narrowing counters.
        clickhouse.shutdown().await;
        url.query_pairs_mut()
            .append_pair("output_format_json_quote_64bit_integers", "1");
        let clickhouse = ClickHouse::new(url.to_string()).unwrap();
        let outcome = async {
            let summary = clickhouse
                .query_report::<PerformanceMetricSummary>(&performance_summary_query("site", 30))
                .await?;
            assert_eq!(summary.len(), 3);
            let lcp = summary.iter().find(|m| m.metric == "LCP").unwrap();
            assert_eq!((lcp.samples, lcp.p75), (6, 1200.0));
            assert_eq!(
                summary.iter().find(|m| m.metric == "CLS").unwrap().p75,
                0.125
            );
            let series = clickhouse
                .query_report::<PerformancePoint>(&performance_series_query("site", 30, "TTFB"))
                .await?;
            assert_eq!(series.len(), 1);
            assert_eq!((series[0].samples, series[0].p75), (1, 250.0));
            let pages = clickhouse
                .query_report::<PerformancePage>(&performance_pages_query("site", 30, "LCP"))
                .await?;
            assert_eq!(pages.len(), 1);
            assert_eq!((pages[0].path.as_str(), pages[0].samples), ("/pricing", 6));
            assert!(clickhouse
                .query_report::<PerformancePage>(&performance_pages_query("site", 30, "TTFB"))
                .await?
                .is_empty());
            assert!(clickhouse
                .query_report::<PerformancePoint>(&performance_series_query("site", 30, "INP"))
                .await?
                .is_empty());
            assert!(clickhouse
                .query_report::<PerformanceMetricSummary>(&performance_summary_query("empty", 30))
                .await?
                .is_empty());
            Ok::<_, anyhow::Error>(())
        }
        .await;
        clickhouse.shutdown().await;
        execute(&client, &url, format!("DROP DATABASE {db}")).await;
        outcome.unwrap();
    }

    #[test]
    fn web_vital_thresholds_match_field_guidance() {
        assert_eq!(metric_rating("LCP", 2_500.0), "good");
        assert_eq!(metric_rating("LCP", 3_000.0), "needs_improvement");
        assert_eq!(metric_rating("CLS", 0.3), "poor");
    }
    #[tokio::test]
    #[ignore = "requires disposable loopback OWLEYE_TEST_CLICKHOUSE_URL; synthetic SELECT only"]
    async fn real_clickhouse_vital_bands_use_actual_samples_and_withhold_sparse_residuals() {
        let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("disposable ClickHouse URL");
        assert!(url.starts_with("http://127.0.0.1:"));
        let client = crate::storage::clickhouse::ClickHouse::new(url).unwrap();
        let fixture="(SELECT 'vitals-bands' AS site_id, now64(3)-INTERVAL 1 HOUR AS occurred_at, now64(3)+INTERVAL 1 DAY AS retention_active_until, 'performance' AS event_type, 'web_vital_lcp' AS event_name, toNullable(toUInt64(multiIf(number%3=0,1000,number%3=1,3000,5000))) AS duration_ms, '{}' AS payload_json, concat('v',toString(number)) AS visitor_id FROM numbers(30))";
        let sql = performance_summary_query("vitals-bands", 30).replace("owleye_events", fixture);
        let rows = client
            .query_report::<PerformanceMetricSummary>(&sql)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (
                rows[0].good_samples,
                rows[0].needs_improvement_samples,
                rows[0].poor_samples
            ),
            (Some(10), Some(10), Some(10))
        );
        let sparse = fixture.replace(
            "concat('v',toString(number)) AS visitor_id",
            "if(number%3=2,'one-person',concat('v',toString(number))) AS visitor_id",
        );
        let rows = client
            .query_report::<PerformanceMetricSummary>(
                &performance_summary_query("vitals-bands", 30).replace("owleye_events", &sparse),
            )
            .await
            .unwrap();
        assert_eq!(
            (
                rows[0].good_samples,
                rows[0].needs_improvement_samples,
                rows[0].poor_samples
            ),
            (None, None, None)
        );
        let unknown = fixture.replace(
            "concat('v',toString(number)) AS visitor_id",
            "'' AS visitor_id",
        );
        let rows = client
            .query_report::<PerformanceMetricSummary>(
                &performance_summary_query("vitals-bands", 30).replace("owleye_events", &unknown),
            )
            .await
            .unwrap();
        assert!(rows[0].good_samples.is_none());
        // The bounded aggregate must preserve the exact >=5 release boundary.
        for distinct in [4, 5, 6] {
            let bounded = fixture.replace(
                "concat('v',toString(number)) AS visitor_id",
                &format!("if(number%3=2,concat('poor',toString(intDiv(number,3)%{distinct})),concat('v',toString(number))) AS visitor_id"),
            );
            let rows = client
                .query_report::<PerformanceMetricSummary>(
                    &performance_summary_query("vitals-bands", 30)
                        .replace("owleye_events", &bounded),
                )
                .await
                .unwrap();
            let expected = if distinct >= 5 {
                (Some(10), Some(10), Some(10))
            } else {
                (None, None, None)
            };
            assert_eq!(
                (
                    rows[0].good_samples,
                    rows[0].needs_improvement_samples,
                    rows[0].poor_samples
                ),
                expected,
                "distinct known audience {distinct}"
            );
        }
        let empty_bands = fixture.replace("multiIf(number%3=0,1000,number%3=1,3000,5000)", "1000");
        let rows = client
            .query_report::<PerformanceMetricSummary>(
                &performance_summary_query("vitals-bands", 30)
                    .replace("owleye_events", &empty_bands),
            )
            .await
            .unwrap();
        assert_eq!(
            (
                rows[0].good_samples,
                rows[0].needs_improvement_samples,
                rows[0].poor_samples
            ),
            (Some(30), Some(0), Some(0))
        );
        let mixed_unknown = fixture.replace(
            "concat('v',toString(number)) AS visitor_id",
            "if(number%3=2,'',concat('v',toString(number))) AS visitor_id",
        );
        let rows = client
            .query_report::<PerformanceMetricSummary>(
                &performance_summary_query("vitals-bands", 30)
                    .replace("owleye_events", &mixed_unknown),
            )
            .await
            .unwrap();
        assert_eq!(
            (
                rows[0].good_samples,
                rows[0].needs_improvement_samples,
                rows[0].poor_samples
            ),
            (None, None, None)
        );
    }
}
