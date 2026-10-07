use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    auth,
    demo::{self, DemoSiteDefinition, DemoTrafficProfile},
    errors::ApiError,
    models::{clickhouse_string, normalize_site_id},
    retention::ACTIVE_ROW_PREDICATE,
    sites,
    storage::clickhouse::ClickHouse,
    AppState,
};

const DEFAULT_STATS_DAYS: u16 = 7;
const MAX_STATS_DAYS: u16 = 365;
const REGION_SERIES_LIMIT: usize = 5;
const REGION_NAME_SQL: &str =
    "multiIf(region = '', 'Unknown', country = '', region, concat(region, ', ', country))";

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    #[serde(default)]
    days: Option<u16>,
    site_id: String,
    start_date: Option<String>,
    end_date: Option<String>,
    #[serde(default)]
    group_by: StatsGrouping,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum StatsGrouping {
    #[default]
    Day,
    Week,
    Month,
}

impl StatsGrouping {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct StatsRange {
    start: NaiveDate,
    end: NaiveDate,
}

impl StatsRange {
    fn resolve(query: &StatsQuery, today: NaiveDate) -> Result<Self, ApiError> {
        let range = match (&query.start_date, &query.end_date) {
            (None, None) => Self::recent(
                query
                    .days
                    .unwrap_or(DEFAULT_STATS_DAYS)
                    .clamp(1, MAX_STATS_DAYS),
                today,
            ),
            (Some(start), Some(end)) => {
                let parse = |value: &str| {
                    NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .ok()
                        .filter(|date| date.to_string() == value)
                        .ok_or_else(|| ApiError::BadRequest("Dates must use YYYY-MM-DD".to_owned()))
                };
                Self {
                    start: parse(start)?,
                    end: parse(end)?,
                }
            }
            _ => {
                return Err(ApiError::BadRequest(
                    "Provide both start_date and end_date".to_owned(),
                ))
            }
        };
        let days = (range.end - range.start).num_days() + 1;
        if !(1..=i64::from(MAX_STATS_DAYS)).contains(&days)
            || range.end > today
            || range.start < Self::recent(MAX_STATS_DAYS, today).start
        {
            return Err(ApiError::BadRequest(
                "Choose dates in order within the last 365 days, including today (UTC)".to_owned(),
            ));
        }
        Ok(range)
    }

    fn recent(days: u16, today: NaiveDate) -> Self {
        Self {
            start: today - Duration::days(i64::from(days.saturating_sub(1))),
            end: today,
        }
    }

    fn days(self) -> u16 {
        ((self.end - self.start).num_days() + 1) as u16
    }

    fn filter(self, site_id: &str) -> String {
        format!("site_id = {} AND occurred_at >= toDateTime('{}', 'UTC') AND occurred_at < toDateTime('{}', 'UTC') + INTERVAL 1 DAY AND {}",
            clickhouse_string(site_id), self.start, self.end, ACTIVE_ROW_PREDICATE)
    }

    fn bucket(self, date: NaiveDate, interval: &str) -> NaiveDate {
        let start = match interval {
            "week" => date - Duration::days(i64::from(date.weekday().num_days_from_monday())),
            "month" => date.with_day(1).expect("first day of month"),
            _ => date,
        };
        start.max(self.start)
    }

    fn fill(self, points: Vec<StatsTimeseriesPoint>, interval: &str) -> Vec<StatsTimeseriesPoint> {
        let mut by_date = points
            .into_iter()
            .map(|point| (point.date.clone(), point))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut result = Vec::new();
        let mut previous = None;
        for offset in 0..self.days() {
            let bucket = self.bucket(self.start + Duration::days(i64::from(offset)), interval);
            if previous == Some(bucket) {
                continue;
            }
            previous = Some(bucket);
            let date = bucket.to_string();
            result.push(by_date.remove(&date).unwrap_or(StatsTimeseriesPoint {
                date,
                events: 0,
                pageviews: 0,
                visitors: 0,
            }));
        }
        result
    }
}

#[derive(Debug, Serialize)]
pub struct StatsOverviewResponse {
    browsers: Vec<DimensionStats>,
    countries: Vec<DimensionStats>,
    days: u16,
    start_date: String,
    end_date: String,
    interval: String,
    devices: Vec<DimensionStats>,
    event_names: Vec<EventNameStats>,
    operating_systems: Vec<DimensionStats>,
    referrers: Vec<DimensionStats>,
    region_timeseries: Vec<RegionTimeseriesPoint>,
    regions: Vec<DimensionStats>,
    site_id: String,
    timeseries: Vec<StatsTimeseriesPoint>,
    top_pages: Vec<TopPageStats>,
    totals: StatsTotals,
    utm_campaigns: Vec<DimensionStats>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct StatsTotals {
    avg_duration_ms: u64,
    events: u64,
    external_events: u64,
    pageviews: u64,
    pageview_visitors: u64,
    performance_events: u64,
    rule_events: u64,
    sessions: u64,
    visitors: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct StatsTimeseriesPoint {
    date: String,
    events: u64,
    pageviews: u64,
    visitors: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct RegionTimeseriesPoint {
    count: u64,
    date: String,
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TopPageStats {
    path: String,
    title: String,
    views: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct DimensionStats {
    count: u64,
    #[serde(default)]
    visitors: u64,
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct EventNameStats {
    count: u64,
    event_name: String,
    event_type: String,
}

pub async fn stats_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<StatsQuery>,
) -> Result<Json<StatsOverviewResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    let requested_site_id = normalize_site_id(&query.site_id)?;
    let range = StatsRange::resolve(&query, Utc::now().date_naive())?;
    if auth::is_demo_session(&session) {
        let site = demo::demo_site(&requested_site_id)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Ok(Json(demo_overview_at(site, range.days(), range.end)));
    }

    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &requested_site_id).await?;
    let response = live_overview_range(
        &state.clickhouse,
        site.tracking_id,
        range,
        query.group_by.as_str(),
    )
    .await?;
    Ok(Json(response))
}

pub(crate) async fn live_overview(
    clickhouse: &ClickHouse,
    site_id: String,
    days: u16,
) -> Result<StatsOverviewResponse, ApiError> {
    // Keep the developer API's existing daily response contract.
    live_overview_range(
        clickhouse,
        site_id,
        StatsRange::recent(days, Utc::now().date_naive()),
        "day",
    )
    .await
}

async fn live_overview_range(
    clickhouse: &ClickHouse,
    site_id: String,
    range: StatsRange,
    interval: &str,
) -> Result<StatsOverviewResponse, ApiError> {
    let queries = LiveStatsQueries::new(&site_id, range, interval);

    let (
        totals_rows,
        timeseries,
        top_pages,
        browsers,
        countries,
        regions,
        region_timeseries,
        operating_systems,
        devices,
        referrers,
        event_names,
        utm_campaigns,
    ) = tokio::try_join!(
        clickhouse.query_json_each_row::<StatsTotals>(&queries.totals),
        clickhouse.query_json_each_row::<StatsTimeseriesPoint>(&queries.timeseries),
        clickhouse.query_json_each_row::<TopPageStats>(&queries.top_pages),
        clickhouse.query_json_each_row::<DimensionStats>(&queries.browsers),
        clickhouse.query_json_each_row::<DimensionStats>(&queries.countries),
        clickhouse.query_json_each_row::<DimensionStats>(&queries.regions),
        clickhouse.query_json_each_row::<RegionTimeseriesPoint>(&queries.region_timeseries),
        clickhouse.query_json_each_row::<DimensionStats>(&queries.operating_systems),
        clickhouse.query_json_each_row::<DimensionStats>(&queries.devices),
        clickhouse.query_json_each_row::<DimensionStats>(&queries.referrers),
        clickhouse.query_json_each_row::<EventNameStats>(&queries.event_names),
        clickhouse.query_json_each_row::<DimensionStats>(&queries.utm_campaigns),
    )?;

    Ok(StatsOverviewResponse {
        browsers,
        countries,
        days: range.days(),
        start_date: range.start.to_string(),
        end_date: range.end.to_string(),
        interval: interval.to_owned(),
        devices,
        event_names,
        operating_systems,
        referrers,
        region_timeseries,
        regions,
        site_id,
        timeseries: range.fill(timeseries, interval),
        top_pages,
        totals: totals_rows.into_iter().next().unwrap_or_default(),
        utm_campaigns,
    })
}

struct LiveStatsQueries {
    browsers: String,
    countries: String,
    devices: String,
    event_names: String,
    operating_systems: String,
    referrers: String,
    region_timeseries: String,
    regions: String,
    timeseries: String,
    top_pages: String,
    totals: String,
    utm_campaigns: String,
}

impl LiveStatsQueries {
    fn new(site_id: &str, range: StatsRange, interval: &str) -> Self {
        let filter = range.filter(site_id);
        let mut queries = Self::from_filter(&filter);
        let bucket = match interval {
            "week" => "toStartOfWeek(occurred_at, 1, 'UTC')",
            "month" => "toStartOfMonth(toTimeZone(occurred_at, 'UTC'))",
            _ => "toDate(occurred_at, 'UTC')",
        };
        queries.timeseries = timeseries_query(
            &filter,
            &format!("greatest(toDate('{}'), {bucket})", range.start),
        );
        queries
    }

    fn from_filter(filter: &str) -> Self {
        Self {
            browsers: dimension_query(
                filter,
                &crate::privacy::user_agent::browser_sql("browser_name"),
            ),
            countries: countries_query(filter),
            devices: dimension_query(
                filter,
                "multiIf(lowerUTF8(device_type) = 'desktop', 'Desktop', lowerUTF8(device_type) = 'mobile', 'Mobile', lowerUTF8(device_type) = 'tablet', 'Tablet', device_type = '', 'Unknown', device_type)",
            ),
            event_names: event_names_query(filter),
            operating_systems: dimension_query(
                filter,
                "multiIf(lowerUTF8(os_name) IN ('mac os x', 'mac osx', 'macos', 'mac os', 'os x'), 'macOS', os_name = '', 'Unknown', os_name)",
            ),
            referrers: dimension_query(
                filter,
                &crate::privacy::referrer::referrer_sql("referrer_host"),
            ),
            region_timeseries: region_timeseries_query(filter),
            regions: dimension_query(filter, REGION_NAME_SQL),
            timeseries: timeseries_query(filter, "toDate(occurred_at, 'UTC')"),
            top_pages: top_pages_query(filter),
            totals: totals_query(filter),
            utm_campaigns: utm_campaigns_query(filter),
        }
    }
}

// Unique visitor/session metrics are estimates by design. `uniqCombined64`
// keeps memory bounded at analytics scale while remaining deterministic enough
// for dashboards; access-control decisions must never use them.
fn totals_query(filter: &str) -> String {
    format!(
        r#"
        SELECT
            toUInt64(countIf(event_type NOT IN ('performance', 'page_session'))) AS events,
            toUInt64(countIf(event_type = 'pageview')) AS pageviews,
            toUInt64(uniqCombined64(visitor_id)) AS visitors,
            toUInt64(uniqCombined64If(visitor_id, event_type = 'pageview')) AS pageview_visitors,
            toUInt64(uniqCombined64If(anon_session_id, event_type = 'pageview')) AS sessions,
            toUInt64(countIf(event_type = 'external')) AS external_events,
            toUInt64(countIf(event_type = 'rule')) AS rule_events,
            toUInt64(countIf(event_type = 'performance')) AS performance_events,
            toUInt64(ifNull(round(avgIf(duration_ms, event_type = 'page_session')), 0)) AS avg_duration_ms
        FROM owleye_events
        WHERE {filter}
        "#
    )
}

fn timeseries_query(filter: &str, bucket: &str) -> String {
    format!(
        r#"
        SELECT
            toString({bucket}) AS date,
            toUInt64(countIf(event_type NOT IN ('performance', 'page_session'))) AS events,
            toUInt64(countIf(event_type = 'pageview')) AS pageviews,
            toUInt64(uniqCombined64(visitor_id)) AS visitors
        FROM owleye_events
        WHERE {filter}
        GROUP BY date
        ORDER BY date ASC
        "#
    )
}

fn top_pages_query(filter: &str) -> String {
    format!(
        r#"
        SELECT
            url_path AS path,
            argMax(page_title, tuple(occurred_at, event_id)) AS title,
            toUInt64(count()) AS views
        FROM owleye_events
        WHERE {filter} AND event_type = 'pageview'
        GROUP BY path
        ORDER BY views DESC, path ASC
        LIMIT 10
        "#
    )
}

fn dimension_query(filter: &str, name_expression: &str) -> String {
    // The tail is grouped before distinct counting, so Other preserves both
    // page-view totals and deduplication across its constituent categories.
    format!(
        r#"
        WITH pageviews AS (
            SELECT {name_expression} AS category, visitor_id
            FROM owleye_events WHERE {filter} AND event_type = 'pageview'
        ), top_names AS (
            SELECT category FROM pageviews GROUP BY category
            ORDER BY count() DESC, category ASC LIMIT 10
        )
        SELECT if(category IN (SELECT category FROM top_names), category, 'Other') AS name,
            toUInt64(count()) AS count,
            toUInt64(uniqCombined64(visitor_id)) AS visitors
        FROM pageviews GROUP BY name ORDER BY count DESC, name ASC
        "#
    )
}

fn countries_query(filter: &str) -> String {
    format!(
        "SELECT if(country = '', 'Unknown', country) AS name, toUInt64(count()) AS count, toUInt64(uniqCombined64(visitor_id)) AS visitors \
         FROM owleye_events WHERE {filter} AND event_type = 'pageview' \
         GROUP BY name ORDER BY count DESC, name ASC"
    )
}

fn region_timeseries_query(filter: &str) -> String {
    format!(
        r#"
        WITH top_regions AS (
            SELECT
                {REGION_NAME_SQL} AS name,
                count() AS total
            FROM owleye_events
            WHERE {filter} AND event_type = 'pageview'
            GROUP BY name
            ORDER BY total DESC, name ASC
            LIMIT {REGION_SERIES_LIMIT}
        )
        SELECT
            toString(toDate(occurred_at)) AS date,
            {REGION_NAME_SQL} AS name,
            toUInt64(count()) AS count
        FROM owleye_events
        WHERE {filter}
          AND event_type = 'pageview'
          AND {REGION_NAME_SQL} IN (SELECT name FROM top_regions)
        GROUP BY date, name
        ORDER BY date ASC, name ASC
        "#
    )
}

fn event_names_query(filter: &str) -> String {
    format!(
        r#"
        SELECT
            event_type,
            event_name,
            toUInt64(count()) AS count
        FROM owleye_events
        WHERE {filter} AND event_type NOT IN ('performance', 'page_session')
        GROUP BY event_type, event_name
        ORDER BY count DESC, event_type ASC, event_name ASC
        LIMIT 20
        "#
    )
}

fn utm_campaigns_query(filter: &str) -> String {
    dimension_query(&format!("{filter} AND utm_campaign != ''"), "utm_campaign")
}

fn demo_overview_at(
    site: &DemoSiteDefinition,
    days: u16,
    today: NaiveDate,
) -> StatsOverviewResponse {
    let available_days = if site.profile == DemoTrafficProfile::New {
        days.min(7)
    } else {
        days
    };
    let timeseries = demo_timeseries(site.profile, available_days, today);
    let totals = demo_totals(site.profile, &timeseries);
    let pageviews = totals.pageviews;

    StatsOverviewResponse {
        browsers: weighted_dimensions(
            pageviews,
            &[
                ("Chrome", 46),
                ("Safari", 28),
                ("Firefox", 12),
                ("Edge", 9),
                ("Other", 5),
            ],
        ),
        countries: weighted_dimensions(pageviews, demo_country_weights(site.profile)),
        days,
        start_date: (today - Duration::days(i64::from(days.saturating_sub(1)))).to_string(),
        end_date: today.to_string(),
        interval: "day".to_owned(),
        devices: weighted_dimensions(pageviews, &[("Desktop", 58), ("Mobile", 37), ("Tablet", 5)]),
        event_names: demo_event_names(&totals),
        operating_systems: weighted_dimensions(
            pageviews,
            &[
                ("macOS", 34),
                ("Windows", 29),
                ("iOS", 19),
                ("Android", 15),
                ("Linux", 3),
            ],
        ),
        referrers: weighted_dimensions(
            pageviews,
            &[
                ("Direct", 34),
                ("google.com", 26),
                ("linkedin.com", 14),
                ("github.com", 11),
                ("x.com", 8),
                ("newsletter", 7),
            ],
        ),
        region_timeseries: demo_region_timeseries(site.profile, &timeseries),
        regions: weighted_dimensions(pageviews, demo_region_weights(site.profile)),
        site_id: site.tracking_id.to_owned(),
        timeseries,
        top_pages: demo_top_pages(site.profile, pageviews),
        totals,
        utm_campaigns: weighted_dimensions(pageviews, demo_utm_campaign_weights(site.profile)),
    }
}

fn demo_timeseries(
    profile: DemoTrafficProfile,
    days: u16,
    today: NaiveDate,
) -> Vec<StatsTimeseriesPoint> {
    let mut timeseries = Vec::with_capacity(usize::from(days));

    for offset in (0..days).rev() {
        let date = today - Duration::days(i64::from(offset));
        let pageviews = demo_pageviews(profile, date);
        let visitors = pageviews.saturating_mul(57) / 100 + u64::from(date.ordinal() % 31);
        let events = pageviews
            + pageviews / 6
            + pageviews / 18
            + pageviews / 31
            + u64::from(date.ordinal() % 17);
        timeseries.push(StatsTimeseriesPoint {
            date: date.format("%Y-%m-%d").to_string(),
            events,
            pageviews,
            visitors,
        });
    }

    timeseries
}

fn demo_totals(profile: DemoTrafficProfile, timeseries: &[StatsTimeseriesPoint]) -> StatsTotals {
    let pageviews = timeseries.iter().map(|point| point.pageviews).sum::<u64>();
    let visitor_days = timeseries.iter().map(|point| point.visitors).sum::<u64>();
    let visitors = if timeseries.len() <= 1 {
        visitor_days
    } else {
        visitor_days.saturating_mul(68) / 100
    };
    let events = timeseries.iter().map(|point| point.events).sum::<u64>();
    let external_events = pageviews / 18;
    let rule_events = pageviews / 31;
    let performance_events = pageviews / 6;
    StatsTotals {
        avg_duration_ms: match profile {
            DemoTrafficProfile::Massive => 183,
            DemoTrafficProfile::LowVolume => 247,
            DemoTrafficProfile::Campaign => 94,
            DemoTrafficProfile::New => 138,
            DemoTrafficProfile::Mixed => 211,
        },
        events,
        external_events,
        pageviews,
        pageview_visitors: visitors,
        performance_events,
        rule_events,
        sessions: visitor_days.saturating_mul(118) / 100,
        visitors,
    }
}

fn demo_event_names(totals: &StatsTotals) -> Vec<EventNameStats> {
    vec![
        EventNameStats {
            count: totals.pageviews,
            event_name: "page_viewed".to_owned(),
            event_type: "pageview".to_owned(),
        },
        EventNameStats {
            count: totals.external_events,
            event_name: "signup_clicked".to_owned(),
            event_type: "external".to_owned(),
        },
        EventNameStats {
            count: totals.rule_events,
            event_name: "pricing_cta_clicked".to_owned(),
            event_type: "rule".to_owned(),
        },
        EventNameStats {
            count: totals.performance_events,
            event_name: "web_vital".to_owned(),
            event_type: "performance".to_owned(),
        },
        EventNameStats {
            count: totals.pageviews / 74,
            event_name: "checkout_completed".to_owned(),
            event_type: "external".to_owned(),
        },
    ]
}

fn demo_region_timeseries(
    profile: DemoTrafficProfile,
    timeseries: &[StatsTimeseriesPoint],
) -> Vec<RegionTimeseriesPoint> {
    let weights = demo_region_weights(profile);
    let series_count = weights
        .iter()
        .filter(|(name, _)| *name != "Other")
        .take(REGION_SERIES_LIMIT)
        .count();
    let mut points = Vec::with_capacity(timeseries.len().saturating_mul(series_count));

    for point in timeseries {
        for &(name, weight) in weights
            .iter()
            .filter(|(name, _)| *name != "Other")
            .take(REGION_SERIES_LIMIT)
        {
            points.push(RegionTimeseriesPoint {
                count: point.pageviews.saturating_mul(weight) / 100,
                date: point.date.clone(),
                name: name.to_owned(),
            });
        }
    }

    points
}

fn demo_pageviews(profile: DemoTrafficProfile, date: NaiveDate) -> u64 {
    let ordinal = u64::from(date.ordinal());
    let absolute_day = i64::from(date.num_days_from_ce()).unsigned_abs();

    match profile {
        DemoTrafficProfile::Massive => 42_000 + (absolute_day % 24) * 2_250,
        DemoTrafficProfile::LowVolume => 90 + (absolute_day * 137 + ordinal * 17) % 860,
        DemoTrafficProfile::Campaign => {
            if absolute_day % 9 == 0 {
                82_000 + (ordinal % 17) * 950
            } else if absolute_day % 5 == 0 {
                11_000 + (ordinal % 23) * 1_700
            } else {
                280 + (absolute_day * 83 + ordinal * 11) % 700
            }
        }
        DemoTrafficProfile::New => 48 + (absolute_day % 7) * 117,
        DemoTrafficProfile::Mixed => 2_400 + (absolute_day * 337 + ordinal * 29) % 6_800,
    }
}

fn demo_country_weights(profile: DemoTrafficProfile) -> &'static [(&'static str, u64)] {
    match profile {
        DemoTrafficProfile::Massive => &[
            ("United States", 35),
            ("India", 21),
            ("United Kingdom", 14),
            ("Canada", 11),
            ("Australia", 8),
            ("Germany", 6),
            ("Other", 5),
        ],
        DemoTrafficProfile::Campaign => &[
            ("India", 29),
            ("United States", 23),
            ("Brazil", 14),
            ("United Kingdom", 12),
            ("Singapore", 9),
            ("Other", 13),
        ],
        DemoTrafficProfile::LowVolume => &[
            ("United Kingdom", 31),
            ("France", 19),
            ("Netherlands", 17),
            ("United States", 15),
            ("Other", 18),
        ],
        DemoTrafficProfile::New => &[
            ("Australia", 38),
            ("New Zealand", 23),
            ("United States", 18),
            ("India", 12),
            ("Other", 9),
        ],
        DemoTrafficProfile::Mixed => &[
            ("United States", 24),
            ("Germany", 18),
            ("India", 17),
            ("Japan", 13),
            ("Canada", 11),
            ("Other", 17),
        ],
    }
}

fn demo_region_weights(profile: DemoTrafficProfile) -> &'static [(&'static str, u64)] {
    match profile {
        DemoTrafficProfile::Massive => &[
            ("California, US", 22),
            ("Maharashtra, IN", 18),
            ("England, GB", 15),
            ("Ontario, CA", 12),
            ("New South Wales, AU", 10),
            ("Other", 23),
        ],
        DemoTrafficProfile::Campaign => &[
            ("Maharashtra, IN", 25),
            ("California, US", 20),
            ("São Paulo, BR", 15),
            ("England, GB", 12),
            ("Singapore", 10),
            ("Other", 18),
        ],
        DemoTrafficProfile::LowVolume => &[
            ("England, GB", 27),
            ("Île-de-France, FR", 18),
            ("North Holland, NL", 16),
            ("New York, US", 13),
            ("Scotland, GB", 9),
            ("Other", 17),
        ],
        DemoTrafficProfile::New => &[
            ("New South Wales, AU", 30),
            ("Auckland, NZ", 21),
            ("California, US", 17),
            ("Karnataka, IN", 12),
            ("Victoria, AU", 9),
            ("Other", 11),
        ],
        DemoTrafficProfile::Mixed => &[
            ("California, US", 18),
            ("Berlin, DE", 16),
            ("Maharashtra, IN", 15),
            ("Tokyo, JP", 12),
            ("Ontario, CA", 10),
            ("Other", 29),
        ],
    }
}

fn demo_utm_campaign_weights(profile: DemoTrafficProfile) -> &'static [(&'static str, u64)] {
    match profile {
        DemoTrafficProfile::Massive => &[
            ("summer-social", 19),
            ("new-arrivals", 14),
            ("creator-partners", 8),
        ],
        DemoTrafficProfile::Campaign => &[
            ("summer-drop", 31),
            ("founder-launch", 18),
            ("newsletter-july", 11),
        ],
        DemoTrafficProfile::LowVolume => &[("portfolio-share", 9), ("tiny-newsletter", 5)],
        DemoTrafficProfile::New => &[("early-access", 22), ("friends-and-founders", 12)],
        DemoTrafficProfile::Mixed => &[
            ("docs-launch", 16),
            ("github-readme", 12),
            ("ai-builders", 7),
        ],
    }
}

fn weighted_dimensions(total: u64, weights: &[(&str, u64)]) -> Vec<DimensionStats> {
    weights
        .iter()
        .map(|(name, weight)| DimensionStats {
            count: total.saturating_mul(*weight) / 100,
            visitors: total.saturating_mul(*weight) / 100,
            name: (*name).to_owned(),
        })
        .collect()
}

fn demo_top_pages(profile: DemoTrafficProfile, pageviews: u64) -> Vec<TopPageStats> {
    let pages = match profile {
        DemoTrafficProfile::Massive => &[
            ("/", "Nova Commerce", 28),
            ("/collections/new", "New arrivals", 19),
            ("/products/cloud-runner", "Cloud Runner", 15),
            ("/checkout", "Checkout", 9),
            ("/journal", "Nova Journal", 7),
        ][..],
        DemoTrafficProfile::Campaign => &[
            ("/summer-drop", "Summer drop", 36),
            ("/", "Launchpad", 21),
            ("/pricing", "Plans", 14),
            ("/case-studies", "Customer stories", 9),
            ("/book-a-demo", "Book a demo", 7),
        ],
        DemoTrafficProfile::LowVolume => &[
            ("/", "Pocket Studio", 39),
            ("/work", "Selected work", 24),
            ("/about", "About", 15),
            ("/notes", "Notes", 10),
            ("/contact", "Contact", 7),
        ],
        DemoTrafficProfile::New => &[
            ("/", "Sprout Notes", 46),
            ("/early-access", "Early access", 27),
            ("/why-sprout", "Why Sprout", 14),
            ("/changelog", "Changelog", 8),
            ("/about", "About", 5),
        ],
        DemoTrafficProfile::Mixed => &[
            ("/docs", "Atlas Docs", 31),
            ("/docs/quickstart", "Quickstart", 22),
            ("/", "Atlas", 16),
            ("/docs/api", "API reference", 13),
            ("/blog", "Engineering notes", 8),
        ],
    };

    pages
        .iter()
        .map(|(path, title, weight)| TopPageStats {
            path: (*path).to_owned(),
            title: (*title).to_owned(),
            views: pageviews.saturating_mul(*weight) / 100,
        })
        .collect()
}

/// A separate allowlisted response contract. Do not serialize the authenticated
/// Overview response here: adding a private field must never publish it by default.
pub(crate) async fn public_overview(
    clickhouse: &ClickHouse,
    site_id: &str,
    days: u16,
    config: &crate::public_dashboard::ShareConfig,
) -> Result<serde_json::Value, ApiError> {
    use crate::public_dashboard::{Breakdown, Metric};
    use serde_json::{json, Map, Value};
    let range = StatsRange::recent(days, Utc::now().date_naive());
    let filter = range.filter(site_id);
    let metrics = config
        .metrics
        .iter()
        .map(|metric| {
            let expression = match metric {
                Metric::Visitors => "uniqCombined64(visitor_id)",
                Metric::Pageviews => "countIf(event_type = 'pageview')",
                Metric::Events => "countIf(event_type NOT IN ('performance', 'page_session'))",
                Metric::Sessions => "uniqCombined64If(anon_session_id, event_type = 'pageview')",
            };
            format!("toUInt64({expression}) AS {}", metric.key())
        })
        .collect::<Vec<_>>()
        .join(", ");
    let totals: Vec<Value> = clickhouse
        .query_public_overview(&format!(
            "SELECT {metrics} FROM owleye_events WHERE {filter}"
        ))
        .await?;
    let mut data = Map::new();
    data.insert("days".into(), json!(days));
    data.insert("start_date".into(), json!(range.start.to_string()));
    data.insert("end_date".into(), json!(range.end.to_string()));
    // Project again even though the query selects only authorized columns.
    let project = |row: &Value| -> Map<String, Value> {
        config
            .metrics
            .iter()
            .map(|metric| {
                (
                    metric.key().into(),
                    json!(row[metric.key()].as_u64().unwrap_or(0)),
                )
            })
            .collect()
    };
    data.insert(
        "totals".into(),
        Value::Object(project(totals.first().unwrap_or(&Value::Null))),
    );
    if config.traffic {
        let rows: Vec<Value> = clickhouse.query_public_overview(&format!("SELECT toString(toDate(occurred_at, 'UTC')) AS date, {metrics} FROM owleye_events WHERE {filter} GROUP BY date ORDER BY date")).await?;
        let by_date = rows
            .iter()
            .filter_map(|row| row["date"].as_str().map(|date| (date, row)))
            .collect::<std::collections::HashMap<_, _>>();
        let points: Vec<Value> = (0..days)
            .map(|offset| {
                let date = (range.start + Duration::days(i64::from(offset))).to_string();
                let mut point =
                    project(by_date.get(date.as_str()).copied().unwrap_or(&Value::Null));
                point.insert("date".into(), json!(date));
                Value::Object(point)
            })
            .collect();
        data.insert("traffic".into(), json!(points));
    }
    for dimension in &config.breakdowns {
        let expression = match dimension {
            Breakdown::Countries => "if(country = '', 'Unknown', country)",
            Breakdown::Browsers => &crate::privacy::user_agent::browser_sql("browser_name"),
            Breakdown::Devices => "multiIf(lowerUTF8(device_type) = 'desktop', 'Desktop', lowerUTF8(device_type) = 'mobile', 'Mobile', lowerUTF8(device_type) = 'tablet', 'Tablet', 'Unknown')",
            Breakdown::OperatingSystems => "multiIf(lowerUTF8(os_name) IN ('mac os x', 'mac osx', 'macos', 'mac os', 'os x'), 'Mac', os_name = '', 'Unknown', os_name)",
        };
        let rows: Vec<Value> = clickhouse.query_public_overview(&format!("SELECT {expression} AS name, toUInt64(count()) AS count FROM owleye_events WHERE {filter} AND event_type = 'pageview' GROUP BY name HAVING uniqCombined64(visitor_id) >= 5 ORDER BY count DESC, name ASC LIMIT 20")).await?;
        let safe_rows: Vec<Value> = rows
            .into_iter()
            .filter_map(|row| {
                let name = row["name"].as_str()?;
                if name.len() > 80 || name.contains(['@', '\n', '\r', '<', '>']) {
                    return None;
                }
                Some(json!({"name": name, "count": row["count"].as_u64().unwrap_or(0)}))
            })
            .collect();
        data.insert(dimension.key().into(), json!(safe_rows));
    }
    Ok(Value::Object(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid fixed test date")
    }

    #[test]
    fn demo_profiles_keep_their_promised_scale() {
        let today = fixed_today();
        let massive = demo_overview_at(&demo::DEMO_SITES[0], 30, today);
        assert!(massive
            .timeseries
            .iter()
            .all(|point| (10_000..=100_000).contains(&point.pageviews)));

        let low = demo_overview_at(&demo::DEMO_SITES[1], 30, today);
        assert!(low.timeseries.iter().all(|point| point.pageviews <= 1_000));

        let campaign = demo_overview_at(&demo::DEMO_SITES[2], 30, today);
        assert!(campaign
            .timeseries
            .iter()
            .any(|point| point.pageviews <= 1_000));
        assert!(campaign
            .timeseries
            .iter()
            .any(|point| point.pageviews > 10_000));

        let new_site = demo_overview_at(&demo::DEMO_SITES[3], 30, today);
        assert_eq!(new_site.timeseries.len(), 7);
        assert!(
            massive.totals.visitors < massive.timeseries.iter().map(|point| point.visitors).sum()
        );
    }

    #[test]
    fn demo_overview_includes_every_requested_dimension() {
        let overview = demo_overview_at(&demo::DEMO_SITES[4], 7, fixed_today());
        assert!(!overview.browsers.is_empty());
        assert!(!overview.countries.is_empty());
        assert!(!overview.devices.is_empty());
        assert!(!overview.operating_systems.is_empty());
        assert!(!overview.referrers.is_empty());
        assert!(!overview.regions.is_empty());
        assert!(!overview.region_timeseries.is_empty());
        assert!(!overview.top_pages.is_empty());
        assert!(!overview.event_names.is_empty());
        assert!(!overview.utm_campaigns.is_empty());
    }

    #[test]
    fn demo_overview_is_deterministic_for_a_given_day() {
        let site = &demo::DEMO_SITES[2];
        let first = demo_overview_at(site, 30, fixed_today());
        let second = demo_overview_at(site, 30, fixed_today());

        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap()
        );
    }

    #[test]
    fn demo_region_series_have_stable_names_and_bounded_size() {
        let overview = demo_overview_at(&demo::DEMO_SITES[0], 30, fixed_today());
        let first_day_names = overview
            .region_timeseries
            .iter()
            .take(REGION_SERIES_LIMIT)
            .map(|point| point.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            overview.region_timeseries.len(),
            overview.timeseries.len() * REGION_SERIES_LIMIT
        );
        assert!(overview
            .region_timeseries
            .chunks_exact(REGION_SERIES_LIMIT)
            .all(|day| day
                .iter()
                .map(|point| point.name.as_str())
                .eq(first_day_names.iter().copied())));
    }

    #[tokio::test]
    #[ignore = "Read-only integration check: requires local ClickHouse and OWLEYE_TEST_STATS_SITE"]
    async fn local_clickhouse_range_aggregation() {
        let site = std::env::var("OWLEYE_TEST_STATS_SITE").expect("set a seeded local site ID");
        let clickhouse = ClickHouse::new("http://localhost:8123".to_owned()).unwrap();
        let fixture = "(SELECT tupleElement(row, 1) AS event_type, tupleElement(row, 2) AS browser_name, tupleElement(row, 3) AS visitor_id FROM (SELECT arrayJoin([('pageview','Chrome','u1'),('pageview','Chrome','u1'),('pageview','Firefox','u1'),('external','Safari','u2'),('performance','Edge','u3')]) AS row))";
        let sql = dimension_query("1", "browser_name")
            .replace("FROM owleye_events", &format!("FROM {fixture}"));
        let fixture_rows = clickhouse
            .query_json_each_row::<DimensionStats>(&sql)
            .await
            .unwrap();
        assert_eq!(fixture_rows.len(), 2);
        assert_eq!(fixture_rows.iter().map(|row| row.count).sum::<u64>(), 3);
        assert!(fixture_rows.iter().all(|row| row.visitors == 1));
        assert_eq!(fixture_rows[0].name, "Chrome");
        assert_eq!(fixture_rows[0].count, 2);
        // End yesterday so an active load generator cannot change totals between queries.
        let end = Utc::now().date_naive() - Duration::days(1);
        for (days, interval) in [
            (7, "day"),
            (30, "day"),
            (90, "week"),
            (180, "week"),
            (365, "month"),
        ] {
            let range = StatsRange::recent(days, end);
            let grouped = live_overview_range(&clickhouse, site.clone(), range, interval)
                .await
                .unwrap();
            assert_eq!(
                grouped.timeseries.iter().map(|p| p.events).sum::<u64>(),
                grouped.totals.events
            );
            assert_eq!(
                grouped.timeseries.iter().map(|p| p.pageviews).sum::<u64>(),
                grouped.totals.pageviews
            );
            assert!(grouped.totals.events > 0);
            for dimensions in [
                &grouped.browsers,
                &grouped.devices,
                &grouped.operating_systems,
                &grouped.referrers,
            ] {
                assert_eq!(
                    dimensions.iter().map(|item| item.count).sum::<u64>(),
                    grouped.totals.pageviews
                );
                assert!(dimensions
                    .iter()
                    .all(|item| item.visitors > 0 && item.visitors <= item.count));
            }
            assert!(grouped.devices.iter().all(|item| item.name != "desktop"));
            assert!(grouped.totals.pageview_visitors > 0);
            assert_eq!(
                grouped
                    .countries
                    .iter()
                    .map(|country| country.count)
                    .sum::<u64>(),
                grouped.totals.pageviews
            );
            assert_eq!(
                grouped.timeseries.first().unwrap().date,
                range.start.to_string()
            );
            println!(
                "{days} days: {} {} buckets, {} events",
                grouped.timeseries.len(),
                grouped.interval,
                grouped.totals.events
            );
        }
        let range = StatsRange {
            start: end - Duration::days(67),
            end: end - Duration::days(7),
        };
        let weekly = live_overview_range(&clickhouse, site.clone(), range, "week")
            .await
            .unwrap();
        let daily = live_overview_range(&clickhouse, site, range, "day")
            .await
            .unwrap();
        assert_eq!(weekly.totals.events, daily.totals.events);
        let daily_visitors: u64 = daily.timeseries.iter().map(|p| p.visitors).sum();
        let weekly_visitors: u64 = weekly.timeseries.iter().map(|p| p.visitors).sum();
        // uniqCombined64 estimates at different bucket sizes can vary slightly.
        assert!(weekly_visitors as f64 <= daily_visitors as f64 * 1.02);
        println!("Custom 61 days: daily visitor sum {daily_visitors}, weekly visitor sum {weekly_visitors}");
    }

    #[test]
    fn custom_ranges_validate_calendar_bounds() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 19).unwrap();
        for (days, interval) in [
            (7, "day"),
            (30, "day"),
            (31, "week"),
            (90, "week"),
            (180, "week"),
            (181, "month"),
            (365, "month"),
        ] {
            let range = StatsRange::recent(days, today);
            assert_eq!(range.days(), days);
            assert_eq!(
                range.fill(vec![], interval).first().unwrap().date,
                range.start.to_string()
            );
        }
        let query = |start: Option<&str>, end: Option<&str>| StatsQuery {
            days: None,
            site_id: "site".to_owned(),
            start_date: start.map(str::to_owned),
            end_date: end.map(str::to_owned),
            group_by: StatsGrouping::Day,
        };
        for invalid in [
            query(Some("2026-02-30"), Some("2026-03-01")),
            query(Some("2026-09-20"), Some("2026-09-20")),
            query(Some("2026-09-19"), Some("2026-09-18")),
            query(Some("2025-09-19"), Some("2026-09-19")),
            query(Some("2025-01-01"), Some("2025-01-07")),
            query(Some("2026-09-19"), None),
        ] {
            assert!(StatsRange::resolve(&invalid, today).is_err());
        }
        assert_eq!(
            StatsRange::resolve(&query(Some("2025-09-20"), Some("2026-09-19")), today)
                .unwrap()
                .days(),
            365
        );
        let range =
            StatsRange::resolve(&query(Some("2026-05-01"), Some("2026-06-30")), today).unwrap();
        assert_eq!(range.days(), 61);
        let filter = range.filter("site'quoted");
        assert!(filter.contains("toDateTime('2026-05-01', 'UTC')"));
        assert!(filter.contains("toDateTime('2026-06-30', 'UTC') + INTERVAL 1 DAY"));
        assert!(filter.contains(ACTIVE_ROW_PREDICATE));
    }

    #[test]
    fn countries_include_the_full_pageview_distribution() {
        let sql = countries_query("site_id = 'site' AND retention_active_until > now()");
        assert!(sql.contains("event_type = 'pageview'"));
        assert!(sql.contains("site_id = 'site'"));
        assert!(!sql.contains("LIMIT"));
    }

    #[test]
    fn requested_grouping_is_explicit_and_validated() {
        for (group, expected) in [
            ("day", StatsGrouping::Day),
            ("week", StatsGrouping::Week),
            ("month", StatsGrouping::Month),
        ] {
            let query: StatsQuery = serde_json::from_value(
                serde_json::json!({"site_id": "site", "days": 7, "group_by": group}),
            )
            .unwrap();
            assert_eq!(query.group_by, expected);
            let range = StatsRange::recent(7, NaiveDate::from_ymd_opt(2026, 9, 19).unwrap());
            let queries = LiveStatsQueries::new("site", range, query.group_by.as_str());
            let expression = match expected {
                StatsGrouping::Day => "toDate(occurred_at, 'UTC')",
                StatsGrouping::Week => "toStartOfWeek(occurred_at, 1, 'UTC')",
                StatsGrouping::Month => "toStartOfMonth(toTimeZone(occurred_at, 'UTC'))",
            };
            assert!(queries.timeseries.contains(expression));
        }
        let default: StatsQuery =
            serde_json::from_value(serde_json::json!({"site_id": "site", "days": 365})).unwrap();
        assert_eq!(default.group_by, StatsGrouping::Day);
        assert!(serde_json::from_value::<StatsQuery>(
            serde_json::json!({"site_id": "site", "group_by": "year"})
        )
        .is_err());
    }

    #[test]
    fn calendar_buckets_include_partial_edges_and_empty_periods() {
        let range = StatsRange {
            start: NaiveDate::from_ymd_opt(2026, 1, 30).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 3, 2).unwrap(),
        };
        let points = range.fill(
            vec![StatsTimeseriesPoint {
                date: "2026-02-01".into(),
                events: 10,
                pageviews: 5,
                visitors: 3,
            }],
            "month",
        );
        assert_eq!(
            points.iter().map(|p| p.date.as_str()).collect::<Vec<_>>(),
            vec!["2026-01-30", "2026-02-01", "2026-03-01"]
        );
        assert_eq!(points.iter().map(|p| p.events).sum::<u64>(), 10);
        assert_eq!(points[1].visitors, 3);
        let weeks = range.fill(vec![], "week");
        assert_eq!(weeks.first().unwrap().date, "2026-01-30");
        assert_eq!(weeks[1].date, "2026-02-02");
        assert_eq!(weeks.last().unwrap().date, "2026-03-02");
        assert_eq!(range.fill(vec![], "day").len(), 32);
    }

    #[test]
    fn live_query_plan_uses_calendar_days_and_stable_region_selection() {
        let range = StatsRange::recent(7, NaiveDate::from_ymd_opt(2026, 9, 19).unwrap());
        let queries = LiveStatsQueries::new("site'quoted", range, "day");

        assert!(queries.totals.contains("site_id = 'site\\'quoted'"));
        assert!(queries.totals.contains("toDateTime('2026-09-13', 'UTC')"));
        assert!(queries
            .totals
            .contains("occurred_at < toDateTime('2026-09-19', 'UTC') + INTERVAL 1 DAY"));
        assert!(queries.region_timeseries.contains("WITH top_regions AS"));
        assert!(queries
            .region_timeseries
            .contains("IN (SELECT name FROM top_regions)"));
        assert!(!queries.region_timeseries.contains("LIMIT 5 BY date"));
        assert!(queries.top_pages.contains("argMax(page_title"));
        assert!(queries.totals.contains("uniqCombined64(visitor_id)"));
        assert!(queries
            .totals
            .contains("uniqCombined64If(anon_session_id, event_type = 'pageview')"));
        assert!(queries.utm_campaigns.contains("utm_campaign"));
        assert!(queries.utm_campaigns.contains("event_type = 'pageview'"));
        assert!(queries.totals.contains(ACTIVE_ROW_PREDICATE));
    }
}
