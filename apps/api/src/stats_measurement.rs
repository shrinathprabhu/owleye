//! Additional aggregate measurements over existing facts. These fixed buckets
//! are observed cohorts, never rolling sessions or identified visitor journeys.
use super::{DimensionStats, StatsRange};
use crate::{
    models::clickhouse_string, report_population, retention::ACTIVE_ROW_PREDICATE,
    storage::clickhouse::ClickHouse,
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};

const GRACE_SECONDS: i64 = 300;

pub(super) const CHANNEL_SQL: &str = r#"multiIf(
 lowerUTF8(trimBoth(utm_medium)) IN ('cpc','ppc','cpm','cpv','paid','paid_search','paid_social','paid-social','paidsearch','display','retargeting'), 'Paid',
 lowerUTF8(trimBoth(utm_medium)) IN ('email','newsletter'), 'Email',
 referrer_host!='' AND lowerUTF8(referrer_host)=lowerUTF8(url_host), 'Direct / unknown',
 lowerUTF8(referrer_host) IN ('chatgpt.com','chat.openai.com','claude.ai','perplexity.ai','www.perplexity.ai','gemini.google.com','copilot.microsoft.com','grok.com','chat.deepseek.com','phind.com','www.phind.com','you.com','poe.com'), 'AI',
 lowerUTF8(referrer_host) IN ('mail.google.com','outlook.live.com','outlook.office.com'), 'Email',
 lowerUTF8(trimBoth(utm_medium)) IN ('social','social_media','social-network','organic_social') OR lowerUTF8(referrer_host) IN ('x.com','www.x.com','twitter.com','t.co','facebook.com','www.facebook.com','m.facebook.com','l.facebook.com','instagram.com','www.instagram.com','linkedin.com','www.linkedin.com','lnkd.in','reddit.com','www.reddit.com','old.reddit.com','redd.it','youtube.com','www.youtube.com','youtu.be','threads.net','www.threads.net','threads.com','www.threads.com','bsky.app','mastodon.social','tiktok.com','www.tiktok.com','pinterest.com','www.pinterest.com','pin.it'), 'Social',
 lowerUTF8(trimBoth(utm_medium)) IN ('organic','organic_search') OR lowerUTF8(referrer_host) IN ('google.com','www.google.com','google.co.in','www.google.co.in','google.co.uk','www.google.co.uk','google.co.jp','google.com.au','google.com.br','google.de','google.fr','google.ca','google.es','google.it','google.nl','bing.com','www.bing.com','duckduckgo.com','search.yahoo.com','search.brave.com','ecosia.org','www.ecosia.org','yandex.com','yandex.ru','baidu.com','naver.com','kagi.com','startpage.com','qwant.com'), 'Organic',
 referrer_host != '' AND lowerUTF8(referrer_host) != lowerUTF8(url_host), 'Referral',
 'Direct / unknown')"#;

#[derive(Debug, Serialize)]
pub(super) struct MeasurementContext {
    pub definition_version: &'static str,
    pub channel_version: &'static str,
    pub timezone: &'static str,
    pub as_of: String,
    pub cohort_assignment: &'static str,
    pub delivery_grace_seconds: i64,
    pub retention_context: &'static str,
    pub late_data_may_revise_results: bool,
    pub acquisition_values: &'static str,
    pub acquisition_status: &'static str,
    pub legacy_totals_policy: &'static str,
    pub new_metrics_policy: &'static str,
    pub unknown_agents: &'static str,
    pub classification_context: &'static str,
    pub demo: bool,
}
impl MeasurementContext {
    pub(super) fn new(as_of: DateTime<Utc>, demo: bool) -> Self {
        Self {
            definition_version: "fixed-cohort-v1",
            channel_version: "channel-v1",
            timezone: "UTC",
            as_of: as_of.to_rfc3339(),
            cohort_assignment: "fixed_30_minute_utc_bucket_start",
            delivery_grace_seconds: GRACE_SECONDS,
            retention_context: "active_rows_only_historical_completeness_unknown",
            late_data_may_revise_results: true,
            acquisition_values:
                "recognized_source_medium_codes_only_other_deduplicated_small_groups_withheld",
            acquisition_status: if demo { "unavailable_in_demo" } else { "ready" },
            legacy_totals_policy: report_population::LEGACY_ALL_AGENTS_V1,
            new_metrics_policy: report_population::KNOWN_CRAWLERS_EXCLUDED_V1,
            unknown_agents: report_population::UNKNOWN_AGENTS_CONTEXT,
            classification_context: report_population::CLASSIFICATION_CONTEXT,
            demo,
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub(super) struct CohortMeasurements {
    pub status: &'static str,
    pub observed_pageview_cohorts: u64,
    pub eligible_cohorts: u64,
    pub excluded_open_or_boundary_cohorts: u64,
    pub single_page_cohorts: u64,
    pub estimated_bounce_rate: Option<f64>,
    pub observed_span_samples: u64,
    pub avg_observed_span_ms: Option<f64>,
    pub median_observed_span_ms: Option<f64>,
    pub entry_pages_withheld: bool,
    pub last_pages_withheld: bool,
}
impl CohortMeasurements {
    pub(super) fn unavailable_demo() -> Self {
        Self {
            status: "unavailable_in_demo",
            ..Default::default()
        }
    }
}
#[derive(Debug, Serialize)]
pub(super) struct CohortSeriesPoint {
    pub date: String,
    pub eligible_cohorts: u64,
    pub single_page_cohorts: u64,
    pub estimated_bounce_rate: Option<f64>,
    pub observed_span_samples: u64,
    pub avg_observed_span_ms: Option<f64>,
}
#[derive(Debug, Serialize)]
pub(super) struct CohortPage {
    pub path: String,
    pub cohorts: u64,
}
pub(super) struct CohortData {
    pub totals: CohortMeasurements,
    pub series: Vec<CohortSeriesPoint>,
    pub entry_pages: Vec<CohortPage>,
    pub last_pages: Vec<CohortPage>,
}
impl CohortData {
    pub(super) fn unavailable() -> Self {
        Self {
            totals: CohortMeasurements {
                status: "query_failed",
                ..Default::default()
            },
            series: vec![],
            entry_pages: vec![],
            last_pages: vec![],
        }
    }
}
#[derive(Debug, Default, Deserialize)]
struct CohortRow {
    kind: u64,
    date: String,
    entry_page: String,
    last_page: String,
    observed: u64,
    eligible: u64,
    bounces: u64,
    span_samples: u64,
    avg_span: Option<f64>,
    median_span: Option<f64>,
    audience: u64,
    entry_withheld: u64,
    last_withheld: u64,
}

fn range_instants(range: StatsRange) -> (DateTime<Utc>, DateTime<Utc>) {
    match range.rolling_end {
        Some(end) => (end - Duration::hours(24), end),
        None => (
            Utc.from_utc_datetime(&range.start.and_hms_opt(0, 0, 0).unwrap()),
            Utc.from_utc_datetime(&range.end.and_hms_opt(0, 0, 0).unwrap()) + Duration::days(1),
        ),
    }
}

fn cohort_query(site: &str, range: StatsRange, interval: &str, as_of: DateTime<Utc>) -> String {
    let population = report_population::EXCLUDE_KNOWN_CRAWLERS_SQL;
    let (start, end) = range_instants(range);
    let context_start = start.timestamp().div_euclid(1800) * 1800;
    let context_end = ((end.timestamp() + 1799).div_euclid(1800)) * 1800;
    let start = start.timestamp();
    let end = end.timestamp();
    let closed_before = as_of.timestamp() - GRACE_SECONDS;
    let as_of = as_of.timestamp();
    let bucket = if range.rolling_end.is_some() {
        range.hour_bucket().replace("occurred_at", "bucket_start")
    } else {
        match interval {
            "week" => format!(
                "toString(greatest(toDate('{}'), toStartOfWeek(bucket_start, 1, 'UTC')))",
                range.start
            ),
            "month" => format!(
                "toString(greatest(toDate('{}'), toStartOfMonth(toTimeZone(bucket_start, 'UTC'))))",
                range.start
            ),
            _ => "toString(toDate(bucket_start, 'UTC'))".into(),
        }
    };
    // Filter raw context by site/active retention before grouping; filter the
    // *cohort* only afterwards. Passive packets neither create a denominator
    // nor extend observed activity. Chronological page choices are stable.
    format!(
        r#"WITH cohorts AS (
      SELECT anon_session_id,
        toDateTime(intDiv(toInt64(toUnixTimestamp(occurred_at)),1800)*1800,'UTC') AS bucket_start,
        countIf(event_type='pageview') AS pageviews,
        count() AS observations,
        argMin(visitor_id,tuple(occurred_at,event_id)) AS audience_id,
        argMinIf(url_path,tuple(occurred_at,event_id),event_type='pageview') AS entry_page,
        argMaxIf(url_path,tuple(occurred_at,event_id),event_type='pageview') AS last_page,
        toFloat64(dateDiff('millisecond',min(occurred_at),max(occurred_at))) AS span_ms
      FROM owleye_events
      WHERE site_id={site} AND anon_session_id!=''
        AND {population}
        AND event_type IN ('pageview','external','rule')
        AND occurred_at>=toDateTime({context_start},'UTC') AND occurred_at<toDateTime({context_end},'UTC')
        AND occurred_at<=toDateTime({as_of},'UTC')
        AND retention_active_until IS NOT NULL AND retention_active_until>toDateTime({as_of},'UTC')
        AND {ACTIVE_ROW_PREDICATE}
      GROUP BY anon_session_id,bucket_start
      HAVING pageviews>0
    ), measured AS (
      SELECT *, {bucket} AS date,
        bucket_start>=toDateTime({start},'UTC') AND bucket_start+INTERVAL 30 MINUTE<=toDateTime({end},'UTC')
        AND bucket_start+INTERVAL 30 MINUTE<=toDateTime({closed_before},'UTC') AS is_eligible
      FROM cohorts
    ), aggregated AS (
    SELECT toUInt64(grouping(date,entry_page,last_page)) AS kind,date,entry_page,last_page,
      toUInt64(count()) AS observed,toUInt64(countIf(is_eligible)) AS eligible,
      toUInt64(countIf(is_eligible AND pageviews=1)) AS bounces,
      toUInt64(countIf(is_eligible AND observations>=2)) AS span_samples,
      if(span_samples=0,NULL,avgIf(span_ms,is_eligible AND observations>=2)) AS avg_span,
      if(span_samples=0,NULL,quantileTDigestIf(0.5)(span_ms,is_eligible AND observations>=2)) AS median_span,
      toUInt64(uniqExactIf(audience_id,is_eligible AND audience_id!='')) AS audience
    FROM measured GROUP BY GROUPING SETS ((),(date),(entry_page),(last_page))
    ), labeled AS (
      SELECT *,toUInt64(maxIf(audience<5 AND eligible>0,kind=5) OVER ()) AS entry_withheld,
        toUInt64(maxIf(audience<5 AND eligible>0,kind=6) OVER ()) AS last_withheld,
        row_number() OVER (PARTITION BY kind ORDER BY eligible DESC,entry_page,last_page,date) AS position
      FROM aggregated
    ) SELECT kind,date,entry_page,last_page,observed,eligible,bounces,span_samples,avg_span,median_span,audience,entry_withheld,last_withheld
    FROM labeled WHERE kind IN (3,7) OR (kind=5 AND entry_withheld=0 AND position<=10 AND eligible>0)
      OR (kind=6 AND last_withheld=0 AND position<=10 AND eligible>0)
    ORDER BY kind,date,entry_page,last_page"#,
        site = clickhouse_string(site)
    )
}

fn bounce(bounces: u64, eligible: u64) -> Option<f64> {
    (eligible > 0).then(|| bounces as f64 / eligible as f64 * 100.0)
}

fn safe_page_label(path: &str) -> bool {
    use std::sync::LazyLock;
    static SENSITIVE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?i)(%40|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|[0-9a-f]{16,}|[0-9]{6,}|/(?:token|secret|password-reset|reset-password)/)").unwrap()
    });
    path.starts_with('/')
        && path.len() <= 256
        && !path
            .chars()
            .any(|c| c.is_control() || matches!(c, '@' | '?' | '#'))
        && !SENSITIVE.is_match(path)
}

fn decode_cohorts(rows: Vec<CohortRow>) -> CohortData {
    let mut data = CohortData {
        totals: CohortMeasurements {
            status: "no_eligible_cohorts",
            ..Default::default()
        },
        series: vec![],
        entry_pages: vec![],
        last_pages: vec![],
    };
    for row in rows {
        match row.kind {
            7 => {
                data.totals.status = if row.eligible > 0 {
                    "observed_estimate"
                } else {
                    "no_eligible_cohorts"
                };
                data.totals.observed_pageview_cohorts = row.observed;
                data.totals.eligible_cohorts = row.eligible;
                data.totals.excluded_open_or_boundary_cohorts =
                    row.observed.saturating_sub(row.eligible);
                data.totals.single_page_cohorts = row.bounces;
                data.totals.estimated_bounce_rate = bounce(row.bounces, row.eligible);
                data.totals.observed_span_samples = row.span_samples;
                data.totals.avg_observed_span_ms = row.avg_span;
                data.totals.median_observed_span_ms = row.median_span;
                data.totals.entry_pages_withheld |= row.entry_withheld != 0;
                data.totals.last_pages_withheld |= row.last_withheld != 0;
            }
            3 => data.series.push(CohortSeriesPoint {
                date: row.date,
                eligible_cohorts: row.eligible,
                single_page_cohorts: row.bounces,
                estimated_bounce_rate: bounce(row.bounces, row.eligible),
                observed_span_samples: row.span_samples,
                avg_observed_span_ms: row.avg_span,
            }),
            5 if row.eligible > 0 => {
                if row.audience < 5 || !safe_page_label(&row.entry_page) {
                    data.totals.entry_pages_withheld = true;
                } else {
                    data.entry_pages.push(CohortPage {
                        path: row.entry_page,
                        cohorts: row.eligible,
                    });
                }
            }
            6 if row.eligible > 0 => {
                if row.audience < 5 || !safe_page_label(&row.last_page) {
                    data.totals.last_pages_withheld = true;
                } else {
                    data.last_pages.push(CohortPage {
                        path: row.last_page,
                        cohorts: row.eligible,
                    });
                }
            }
            _ => (),
        }
    }
    // Conservative release: if one page group is sparse, do not publish its
    // complement beside the overall count. A row limit is not privacy proof.
    if data.totals.entry_pages_withheld {
        data.entry_pages.clear();
    }
    if data.totals.last_pages_withheld {
        data.last_pages.clear();
    }
    let rank = |pages: &mut Vec<CohortPage>| {
        pages.sort_by(|a, b| b.cohorts.cmp(&a.cohorts).then(a.path.cmp(&b.path)));
        pages.truncate(10);
    };
    rank(&mut data.entry_pages);
    rank(&mut data.last_pages);
    data
}

pub(super) async fn load_cohorts(
    ch: &ClickHouse,
    site: &str,
    range: StatsRange,
    interval: &str,
    as_of: DateTime<Utc>,
) -> anyhow::Result<CohortData> {
    Ok(decode_cohorts(
        ch.query_report(&cohort_query(site, range, interval, as_of))
            .await?,
    ))
}

pub(super) struct AcquisitionData {
    pub channels: Vec<DimensionStats>,
    pub sources: Vec<DimensionStats>,
    pub mediums: Vec<DimensionStats>,
}
impl AcquisitionData {
    pub(super) fn unavailable() -> Self {
        Self {
            channels: vec![],
            sources: vec![],
            mediums: vec![],
        }
    }
}
#[derive(Deserialize)]
struct AcquisitionRow {
    kind: String,
    name: String,
    count: u64,
    visitors: u64,
}
fn acquisition_query(filter: &str) -> String {
    // Campaign values are labels, not evidence that an ad was purchased. A
    // character allowlist alone would still expose recipient/customer codes.
    // Until a separately approved owner registry exists, only these static
    // marketing labels are exposed; custom/free-form codes stay in Other.
    let channel = super::acquisition_channel_sql();
    let population = report_population::EXCLUDE_KNOWN_CRAWLERS_SQL;
    format!(
        r#"WITH scoped AS (
      SELECT {channel} AS channel,
        if(lowerUTF8(trimBoth(utm_source)) IN ('google','google_ads','adwords','bing','duckduckgo','yahoo','baidu','yandex','ecosia','brave','x','twitter','facebook','instagram','linkedin','reddit','youtube','threads','bsky','bluesky','tiktok','mastodon','newsletter','email','chatgpt','openai','claude','perplexity','gemini','copilot','github','producthunt','hackernews','ycombinator'),lowerUTF8(trimBoth(utm_source)),'Other') AS source,
        if(lowerUTF8(trimBoth(utm_medium)) IN ('organic','organic_search','cpc','ppc','cpm','cpv','paid','paid_search','paid_social','paid-social','paidsearch','display','retargeting','email','newsletter','social','social_media','social-network','organic_social','referral','affiliate','none','direct'),lowerUTF8(trimBoth(utm_medium)),'Other') AS medium,
        visitor_id FROM owleye_events WHERE {filter} AND event_type='pageview' AND {population}
    ), source_names AS (
      SELECT source FROM scoped GROUP BY source HAVING uniqExactIf(visitor_id,visitor_id!='')>=5 ORDER BY count() DESC,source LIMIT 10
    ), medium_names AS (
      SELECT medium FROM scoped GROUP BY medium HAVING uniqExactIf(visitor_id,visitor_id!='')>=5 ORDER BY count() DESC,medium LIMIT 10
    )
    SELECT 'channel' AS kind,channel AS name,toUInt64(count()) AS count,toUInt64(uniqCombined64(visitor_id)) AS visitors FROM scoped GROUP BY name
    UNION ALL
    SELECT 'source',if(source IN (SELECT source FROM source_names),source,'Other') AS name,toUInt64(count()),toUInt64(uniqCombined64(visitor_id)) FROM scoped GROUP BY name HAVING uniqExactIf(visitor_id,visitor_id!='')>=5
    UNION ALL
    SELECT 'medium',if(medium IN (SELECT medium FROM medium_names),medium,'Other') AS name,toUInt64(count()),toUInt64(uniqCombined64(visitor_id)) FROM scoped GROUP BY name HAVING uniqExactIf(visitor_id,visitor_id!='')>=5"#
    )
}
pub(super) async fn load_acquisition(
    ch: &ClickHouse,
    filter: &str,
) -> anyhow::Result<AcquisitionData> {
    let rows: Vec<AcquisitionRow> = ch.query_report(&acquisition_query(filter)).await?;
    let mut data = AcquisitionData {
        channels: vec![],
        sources: vec![],
        mediums: vec![],
    };
    for row in rows {
        let target = match row.kind.as_str() {
            "channel" => &mut data.channels,
            "source" => &mut data.sources,
            _ => &mut data.mediums,
        };
        target.push(DimensionStats {
            name: row.name,
            count: row.count,
            visitors: row.visitors,
        });
    }
    for list in [&mut data.channels, &mut data.sources, &mut data.mediums] {
        list.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_denominators_and_single_observation_are_not_zero_duration() {
        assert_eq!(bounce(0, 0), None);
        assert_eq!(bounce(1, 2), Some(50.0));
        let data = decode_cohorts(vec![]);
        assert!(data.totals.avg_observed_span_ms.is_none());
        assert!(data.totals.estimated_bounce_rate.is_none());
    }
    #[test]
    fn new_page_labels_do_not_expose_obvious_legacy_secrets_or_identifiers() {
        for path in ["/docs/getting-started", "/blog/october-release", "/pricing"] {
            assert!(safe_page_label(path));
        }
        for path in [
            "/reset-password/example-code",
            "/users/person@example.com",
            "/users/person%40example.com",
            "/orders/12345678",
            "/token/longsecret",
            "/path?secret=x",
            "/users/550e8400-e29b-41d4-a716-446655440000",
        ] {
            assert!(!safe_page_label(path), "{path}");
        }
    }
    #[test]
    fn unsafe_page_withholding_survives_the_later_total_row() {
        let data = decode_cohorts(vec![
            CohortRow {
                kind: 5,
                entry_page: "/token/private".into(),
                eligible: 6,
                audience: 6,
                ..Default::default()
            },
            CohortRow {
                kind: 5,
                entry_page: "/safe".into(),
                eligible: 6,
                audience: 6,
                ..Default::default()
            },
            CohortRow {
                kind: 7,
                observed: 12,
                eligible: 12,
                ..Default::default()
            },
        ]);
        assert!(data.totals.entry_pages_withheld);
        assert!(data.entry_pages.is_empty());
        assert_eq!(data.totals.eligible_cohorts, 12);
    }
    #[test]
    fn cohort_query_groups_before_filters_and_discloses_unknown_retention() {
        let now = Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap();
        let range = StatsRange::recent(7, now.date_naive());
        let query = cohort_query("site'unsafe", range, "day", now);
        assert!(
            query.contains("argMinIf(url_path,tuple(occurred_at,event_id),event_type='pageview')")
        );
        assert!(query.contains("event_type IN ('pageview','external','rule')"));
        assert!(query.contains("HAVING pageviews>0"));
        assert!(query.contains(report_population::EXCLUDE_KNOWN_CRAWLERS_SQL));
        assert!(query.contains("AND retention_active_until IS NOT NULL"));
        assert!(MeasurementContext::new(now, false)
            .retention_context
            .contains("unknown"));
        let context = MeasurementContext::new(now, false);
        assert_eq!(
            context.legacy_totals_policy,
            report_population::LEGACY_ALL_AGENTS_V1
        );
        assert_eq!(
            context.new_metrics_policy,
            report_population::KNOWN_CRAWLERS_EXCLUDED_V1
        );
        assert!(acquisition_query("1").contains(report_population::EXCLUDE_KNOWN_CRAWLERS_SQL));
    }

    async fn fixture_rows<T: serde::de::DeserializeOwned>(sql: String, fixture: &str) -> Vec<T> {
        let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL")
            .expect("set OWLEYE_TEST_CLICKHOUSE_URL to an isolated ClickHouse server");
        let ch = ClickHouse::new(url).unwrap();
        ch.query_report(&sql.replace("FROM owleye_events", &format!("FROM {fixture}")))
            .await
            .unwrap()
    }

    #[tokio::test]
    #[ignore = "requires OWLEYE_TEST_CLICKHOUSE_URL; synthetic SELECT only"]
    async fn real_clickhouse_closed_cohorts_preserve_bounce_span_and_unknowns() {
        let now = Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap();
        let fixture = r#"(SELECT tupleElement(r,1) AS site_id,tupleElement(r,2) AS anon_session_id,tupleElement(r,3) AS visitor_id,
          parseDateTime64BestEffort(tupleElement(r,4),3,'UTC') AS occurred_at,tupleElement(r,5) AS event_type,
          tupleElement(r,6) AS url_path,toString(rowNumberInAllBlocks()) AS event_id,
          parseDateTime64BestEffort(tupleElement(r,7),3,'UTC') AS retention_active_until,
          if(tupleElement(r,3)='crawler','crawler','') AS device_type
          FROM (SELECT arrayJoin([
          ('fixture','crawler','crawler','2026-10-15 09:01:00','pageview','/bot','2026-12-01'),
          ('fixture','s1','v1','2026-10-15 09:01:00','pageview','/one','2026-12-01'),
          ('fixture','s1','v1','2026-10-15 09:02:00','external','/one','2026-12-01'),
          ('fixture','s2','v2','2026-10-15 09:31:00','pageview','/entry','2026-12-01'),
          ('fixture','s2','v2','2026-10-15 09:40:00','pageview','/last','2026-12-01'),
          ('fixture','s2','v2','2026-10-15 09:59:00','performance','/last','2026-12-01'),
          ('fixture','custom-only','v3','2026-10-15 10:01:00','external','/custom','2026-12-01'),
          ('fixture','open','v4','2026-10-15 11:31:00','pageview','/open','2026-12-01'),
          ('fixture','expired','v5','2026-10-15 00:01:00','pageview','/expired','2026-10-15 11:59:00'),
          ('other','other','other','2026-10-15 09:01:00','pageview','/private','2026-12-01'),
          ('fixture','single','v8','2026-10-15 09:05:00','pageview','/single','2026-12-01'),
          ('fixture','single','v8','2026-10-15 09:29:00','page_session','/single','2026-12-01'),
          ('fixture','tie','v9','2026-10-15 10:01:00','pageview','/tie','2026-12-01'),
          ('fixture','tie','v9','2026-10-15 10:01:00','rule','/tie','2026-12-01'),
          ('fixture','future','v10','2026-10-15 12:01:00','pageview','/future','2026-12-01')]) AS r))"#;
        let range = StatsRange::recent(1, now.date_naive());
        let rows = fixture_rows(cohort_query("fixture", range, "day", now), fixture).await;
        let data = decode_cohorts(rows);
        assert_eq!(data.totals.observed_pageview_cohorts, 5);
        assert_eq!(data.totals.eligible_cohorts, 4);
        assert_eq!(data.totals.excluded_open_or_boundary_cohorts, 1);
        assert_eq!(data.totals.single_page_cohorts, 3);
        assert_eq!(data.totals.estimated_bounce_rate, Some(75.0));
        assert_eq!(data.totals.observed_span_samples, 3);
        assert_eq!(data.totals.avg_observed_span_ms, Some(200_000.0));
        assert_eq!(data.totals.median_observed_span_ms, Some(60_000.0));
        assert!(data.totals.entry_pages_withheld);
        assert!(data.entry_pages.is_empty());
        assert_eq!(data.series[0].eligible_cohorts, 4);
    }

    #[tokio::test]
    #[ignore = "requires OWLEYE_TEST_CLICKHOUSE_URL; synthetic SELECT only"]
    async fn real_clickhouse_channels_are_exclusive_and_values_are_minimized() {
        let fixture = r#"(SELECT tupleElement(r,1) AS utm_medium,tupleElement(r,2) AS referrer_host,
          tupleElement(r,3) AS utm_source,'site.example' AS url_host,'pageview' AS event_type,
          if(tupleElement(r,3)='crawler-source','crawler','') AS device_type,
          toString(rowNumberInAllBlocks()) AS visitor_id FROM (SELECT arrayJoin([
          ('cpc','chatgpt.com','crawler-source'),('cpc','chatgpt.com','paid-code'),('email','chatgpt.com','newsletter'),
          ('','chatgpt.com',''),('','t.co',''),('','google.com',''),('','blog.example',''),
          ('','site.example','person@example.com'),('','','recipient-id-999')]) AS r))"#;
        let rows: Vec<AcquisitionRow> = fixture_rows(acquisition_query("1"), fixture).await;
        let channels = rows
            .iter()
            .filter(|r| r.kind == "channel")
            .collect::<Vec<_>>();
        assert_eq!(channels.iter().map(|r| r.count).sum::<u64>(), 8);
        for name in [
            "Paid",
            "Email",
            "AI",
            "Social",
            "Organic",
            "Referral",
            "Direct / unknown",
        ] {
            assert!(channels.iter().any(|r| r.name == name), "{name}");
        }
        assert_eq!(
            channels
                .iter()
                .find(|r| r.name == "Direct / unknown")
                .unwrap()
                .count,
            2
        );
        assert!(rows.iter().all(|r| !r.name.contains('@')));
        assert!(rows
            .iter()
            .filter(|r| r.kind != "channel")
            .all(|r| r.name == "Other"));
    }

    #[tokio::test]
    #[ignore = "requires OWLEYE_TEST_CLICKHOUSE_URL; synthetic SELECT only"]
    async fn real_clickhouse_custom_utm_codes_stay_other_even_at_high_volume() {
        let fixture="(SELECT 'customer-identifier-42' AS utm_source,'private-medium' AS utm_medium,'' AS referrer_host,'site.example' AS url_host,'pageview' AS event_type,'' AS device_type,toString(number) AS visitor_id FROM numbers(6))";
        let rows: Vec<AcquisitionRow> = fixture_rows(acquisition_query("1"), fixture).await;
        let sources = rows
            .iter()
            .filter(|row| row.kind != "channel")
            .collect::<Vec<_>>();
        assert_eq!(sources.len(), 2);
        assert!(sources
            .iter()
            .all(|row| row.name == "Other" && row.count == 6 && row.visitors == 6));
    }

    #[tokio::test]
    #[ignore = "requires OWLEYE_TEST_CLICKHOUSE_URL; synthetic SELECT only"]
    async fn real_clickhouse_page_privacy_counts_visitors_not_sessions() {
        let now = Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap();
        let fixture="(SELECT 'fixture' AS site_id,toString(number) AS anon_session_id,'same-visitor' AS visitor_id,toDateTime64('2026-10-15 09:01:00',3,'UTC') AS occurred_at,'pageview' AS event_type,'' AS device_type,'/same' AS url_path,toString(number) AS event_id,toDateTime64('2026-12-01',3,'UTC') AS retention_active_until FROM numbers(6))";
        let data = decode_cohorts(
            fixture_rows(
                cohort_query(
                    "fixture",
                    StatsRange::recent(1, now.date_naive()),
                    "day",
                    now,
                ),
                fixture,
            )
            .await,
        );
        assert_eq!(data.totals.eligible_cohorts, 6);
        assert_eq!(data.totals.estimated_bounce_rate, Some(100.0));
        assert_eq!(data.totals.observed_span_samples, 0);
        assert_eq!(data.totals.avg_observed_span_ms, None);
        assert!(data.totals.entry_pages_withheld && data.entry_pages.is_empty());
        assert!(data.totals.last_pages_withheld && data.last_pages.is_empty());
    }

    #[test]
    fn rolling_range_reads_full_context_and_excludes_partial_buckets() {
        let end = Utc.with_ymd_and_hms(2026, 10, 15, 12, 17, 9).unwrap();
        let range = StatsRange {
            rolling_end: Some(end),
            start: (end - Duration::hours(24)).date_naive(),
            end: end.date_naive(),
        };
        let sql = cohort_query("scope", range, "hour", end);
        let context_start = (end - Duration::hours(24)).timestamp().div_euclid(1800) * 1800;
        assert!(sql.contains(&format!("occurred_at>=toDateTime({context_start},'UTC')")));
        assert!(sql.contains(&format!(
            "bucket_start>=toDateTime({},'UTC')",
            (end - Duration::hours(24)).timestamp()
        )));
        assert!(sql.contains("bucket_start+INTERVAL 30 MINUTE<="));
    }

    #[tokio::test]
    async fn optional_measurement_failure_does_not_replace_existing_overview() {
        use axum::{http::StatusCode, routing::post, Router};
        let app = Router::new().route(
            "/",
            post(|body: String| async move {
                if body.contains("WITH cohorts AS") || body.contains(" AS channel") {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "report budget exceeded".to_owned(),
                    );
                }
                if body.contains("AS avg_duration_ms") {
                    let totals = super::super::StatsTotals {
                        pageviews: 7,
                        visitors: 3,
                        pageview_visitors: 3,
                        sessions: 4,
                        events: 7,
                        ..Default::default()
                    };
                    return (
                        StatusCode::OK,
                        format!("{}\n", serde_json::to_string(&totals).unwrap()),
                    );
                }
                (StatusCode::OK, String::new())
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ch = ClickHouse::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let range = StatsRange::recent(7, Utc::now().date_naive());
        let report = super::super::live_overview_range(&ch, "authorized".into(), range, "day")
            .await
            .unwrap();
        assert_eq!(report.totals.pageviews, 7);
        assert_eq!(report.cohorts.status, "query_failed");
        assert_eq!(report.measurement.acquisition_status, "query_failed");
        assert!(report.cohorts.estimated_bounce_rate.is_none());
        assert!(report.channels.is_empty());
        server.abort();
    }
}
