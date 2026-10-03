//! Bounded, aggregate-only recent activity. No presence heartbeat or raw event stream.
use crate::{
    auth, models::clickhouse_string, retention::ACTIVE_ROW_PREDICATE, sites, ApiError, AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap},
    Json,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const CACHE_TTL: Duration = Duration::from_secs(5);
const CACHE_LIMIT: usize = 128;
type Slot = Arc<tokio::sync::Mutex<Option<(Instant, LiveResponse)>>>;
type CacheEntries = HashMap<(String, String), (Instant, Slot)>;
#[derive(Clone)]
pub(crate) struct LiveCache {
    entries: Arc<Mutex<CacheEntries>>,
    queries: Arc<tokio::sync::Semaphore>,
}
impl Default for LiveCache {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            queries: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
}
impl LiveCache {
    fn slot(&self, site: &str, event: &str) -> Result<Slot, ApiError> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let key = (site.to_owned(), event.to_owned());
        if let Some((used, slot)) = entries.get_mut(&key) {
            *used = Instant::now();
            return Ok(slot.clone());
        }
        if entries.len() >= CACHE_LIMIT {
            let oldest = entries
                .iter()
                .filter(|(_, (_, slot))| Arc::strong_count(slot) == 1)
                .min_by_key(|(_, (used, _))| *used)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                entries.remove(&oldest);
            } else {
                return Err(ApiError::TooManyRequests {
                    retry_after_seconds: 10,
                });
            }
        }
        let slot = Arc::new(tokio::sync::Mutex::new(None));
        entries.insert(key, (Instant::now(), slot.clone()));
        Ok(slot)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LiveQuery {
    event: Option<String>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub(crate) struct Counts {
    events: u64,
    pageviews: u64,
    errors: u64,
    custom_events: u64,
    selected_events: u64,
    active_visitors: u64,
}
#[derive(Deserialize)]
struct Row {
    kind: String,
    label: String,
    #[serde(flatten)]
    counts: Counts,
}
#[derive(Clone, Serialize)]
struct Item {
    name: String,
    count: u64,
}
#[derive(Clone, Serialize)]
struct Minute {
    at: String,
    #[serde(flatten)]
    counts: Counts,
}
#[derive(Clone, Serialize)]
pub(crate) struct LiveResponse {
    as_of: String,
    start_at: String,
    selected_event: Option<String>,
    totals: Counts,
    minutes: Vec<Minute>,
    breakdowns: HashMap<String, Vec<Item>>,
}
fn selected_event(value: Option<String>) -> Result<Option<String>, ApiError> {
    let value = value
        .map(|value| value.trim().to_owned())
        .filter(|v| !v.is_empty());
    if value
        .as_ref()
        .is_some_and(|value| value.len() > 120 || value.chars().any(char::is_control))
    {
        return Err(ApiError::BadRequest(
            "Event name must be at most 120 bytes with no control characters".into(),
        ));
    }
    Ok(value)
}
pub(crate) async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(identifier): Path<String>,
    Query(query): Query<LiveQuery>,
) -> Result<([(header::HeaderName, &'static str); 1], Json<LiveResponse>), ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        return Err(ApiError::BadRequest(
            "Live activity is available for connected apps, not the static demo".into(),
        ));
    }
    // Membership and soft-deletion checks happen before every cache hit.
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &identifier).await?;
    let event = selected_event(query.event)?;
    let slot = state
        .live_cache
        .slot(&site.tracking_id, event.as_deref().unwrap_or(""))?;
    let mut cached = slot.lock().await;
    if let Some((stored, response)) = &*cached {
        if stored.elapsed() < CACHE_TTL {
            return Ok((
                [(header::CACHE_CONTROL, "private, no-store")],
                Json(response.clone()),
            ));
        }
    }
    let _permit =
        state
            .live_cache
            .queries
            .try_acquire()
            .map_err(|_| ApiError::TooManyRequests {
                retry_after_seconds: 10,
            })?;
    let now = Utc::now();
    let rows = state
        .clickhouse
        .query_json_each_row::<Row>(&live_sql(&site.tracking_id, event.as_deref(), now))
        .await?;
    let response = response(rows, event, now);
    *cached = Some((Instant::now(), response.clone()));
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(response),
    ))
}
fn datetime(value: DateTime<Utc>) -> String {
    format!(
        "toDateTime64({}, 3, 'UTC')",
        clickhouse_string(&value.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
    )
}
fn live_sql(site: &str, event: Option<&str>, now: DateTime<Utc>) -> String {
    let start = datetime(now - ChronoDuration::minutes(30));
    let end = datetime(now);
    let active = datetime(now - ChronoDuration::minutes(5));
    let selected = event
        .map(|event| format!("event_name = {}", clickhouse_string(event)))
        .unwrap_or_else(|| "0".into());
    format!(
        r#"SELECT bucket.1 AS kind, bucket.2 AS label,
      toUInt64(count()) AS events, toUInt64(countIf(event_type = 'pageview')) AS pageviews,
      toUInt64(countIf(event_type = 'error' OR (event_type = 'external' AND event_name IN ('error','exception','unhandledrejection')))) AS errors,
      toUInt64(countIf(event_type IN ('external','rule'))) AS custom_events,
      toUInt64(countIf({selected})) AS selected_events,
      toUInt64(uniqIf(visitor_id, bucket.1 = 'summary' AND occurred_at >= {active} AND visitor_id != '')) AS active_visitors
    FROM owleye_events
    ARRAY JOIN [('summary',''),('minute',toString(intDiv(dateDiff('millisecond', {start}, occurred_at),60000))),
      ('pages',if(url_path='','/',url_path)), ('countries',if(country='','Unknown country',country)),
      ('campaigns',if(utm_campaign='','No campaign',utm_campaign)), ('referrers',if(referrer_host='','Direct',referrer_host)),
      ('events',if(event_name='',event_type,event_name))] AS bucket
    WHERE site_id = {} AND occurred_at >= {start} AND occurred_at < {end} AND event_type != 'performance' AND {ACTIVE_ROW_PREDICATE}
    GROUP BY bucket ORDER BY kind, events DESC, label LIMIT 30 BY kind
    SETTINGS max_execution_time=5, max_threads=2, output_format_json_quote_64bit_integers=0"#,
        clickhouse_string(site)
    )
}
fn response(rows: Vec<Row>, selected_event: Option<String>, now: DateTime<Utc>) -> LiveResponse {
    let start = now - ChronoDuration::minutes(30);
    let mut result = LiveResponse {
        as_of: now.to_rfc3339(),
        start_at: start.to_rfc3339(),
        selected_event,
        totals: Counts::default(),
        minutes: (0..30)
            .map(|i| Minute {
                at: (start + ChronoDuration::minutes(i)).to_rfc3339(),
                counts: Counts::default(),
            })
            .collect(),
        breakdowns: HashMap::new(),
    };
    for kind in ["pages", "countries", "campaigns", "referrers", "events"] {
        result.breakdowns.insert(kind.into(), vec![]);
    }
    for row in rows {
        match row.kind.as_str() {
            "summary" => result.totals = row.counts,
            "minute" => {
                if let Ok(index) = row.label.parse::<usize>() {
                    if let Some(minute) = result.minutes.get_mut(index) {
                        minute.counts = row.counts;
                    }
                }
            }
            _ => {
                if let Some(items) = result.breakdowns.get_mut(&row.kind) {
                    if items.len() < 10 {
                        items.push(Item {
                            name: row.label,
                            count: row.counts.events,
                        });
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_query_is_bounded_scoped_and_escaped() {
        let sql = live_sql("owl'other", Some("signup' OR 1=1"), Utc::now());
        assert!(sql.contains("owl\\'other"));
        assert!(sql.contains("signup\\' OR 1=1"));
        assert!(sql.contains(ACTIVE_ROW_PREDICATE));
        assert!(sql.contains("max_execution_time=5"));
        assert!(selected_event(Some("x".repeat(121))).is_err());
        let empty = response(vec![], None, Utc::now());
        assert_eq!(empty.totals.events, 0);
        assert_eq!(empty.minutes.len(), 30);
        assert!(empty.breakdowns.values().all(Vec::is_empty));
    }
    #[tokio::test]
    async fn cache_is_shared_scoped_and_bounded() {
        let cache = LiveCache::default();
        let one = cache.slot("one", "").unwrap();
        assert!(Arc::ptr_eq(&one, &cache.clone().slot("one", "").unwrap()));
        assert!(!Arc::ptr_eq(&one, &cache.slot("two", "").unwrap()));
        assert!(!Arc::ptr_eq(&one, &cache.slot("one", "signup").unwrap()));
        for i in 0..200 {
            cache.slot(&i.to_string(), "").unwrap();
        }
        assert_eq!(cache.entries.lock().unwrap().len(), CACHE_LIMIT);
        assert!(Arc::ptr_eq(&one, &cache.slot("one", "").unwrap()));
    }
    #[tokio::test]
    async fn cached_live_activity_still_requires_current_membership() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let state = crate::self_hosted_tests::test_state().await;
        for sql in [
            "INSERT INTO users(id,email) VALUES ('owner','owner@example.test'),('outsider','outsider@example.test')",
            "INSERT INTO sites(id,tracking_id,name,domain,created_by_user_id) VALUES ('site','owl_live','Live','example.test','owner')",
            "INSERT INTO site_memberships(site_id,user_id,role) VALUES ('site','owner','owner')"
        ] { sqlx::query(sql).execute(&state.sqlite).await.unwrap(); }
        let slot = state.live_cache.slot("owl_live", "").unwrap();
        *slot.lock().await = Some((Instant::now(), response(vec![], None, Utc::now())));
        let owner = auth::create_test_session_cookie(&state, "owner").await;
        let outsider = auth::create_test_session_cookie(&state, "outsider").await;
        let app = crate::routes::router(state.clone());
        let request = |cookie: String| {
            Request::builder()
                .uri("/v1/sites/site/live")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap()
        };
        let allowed = app.clone().oneshot(request(owner.clone())).await.unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
        assert_eq!(allowed.headers()["cache-control"], "private, no-store");
        assert_eq!(
            app.clone()
                .oneshot(request(outsider))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        sqlx::query("UPDATE sites SET deleted_at=CURRENT_TIMESTAMP WHERE id='site'")
            .execute(&state.sqlite)
            .await
            .unwrap();
        assert_eq!(
            app.oneshot(request(owner)).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    #[ignore = "requires local ClickHouse; read-only synthetic rows"]
    async fn real_clickhouse_live_counts_recent_events_without_cross_site_or_expired_data() {
        let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").unwrap();
        assert!(matches!(
            reqwest::Url::parse(&url).unwrap().host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]")
        ));
        let ch = crate::storage::clickhouse::ClickHouse::new(url).unwrap();
        let now = Utc::now();
        let source = r#"(SELECT site_id,event_name,event_type,visitor_id,now64(3)-toIntervalSecond(age) AS occurred_at, now64(3)+toIntervalDay(active) AS retention_active_until, '/' AS url_path, '' AS country, '' AS utm_campaign, '' AS referrer_host FROM values('site_id String,event_name String,event_type String,visitor_id String,age UInt16,active Int8', ('test','pageview','pageview','v1',10,1),('test','signup','external','v1',30,1),('test','error','external','v2',400,1),('test','signup','external','old',1900,1),('other','signup','external','v3',10,1),('test','signup','external','v4',10,-1)))"#;
        let sql = live_sql("test", Some("signup"), now)
            .replace("FROM owleye_events", &format!("FROM {source}"));
        let data = response(
            ch.query_json_each_row::<Row>(&sql).await.unwrap(),
            Some("signup".into()),
            now,
        );
        assert_eq!(data.totals.events, 3);
        assert_eq!(data.totals.pageviews, 1);
        assert_eq!(data.totals.errors, 1);
        assert_eq!(data.totals.selected_events, 1);
        assert_eq!(data.totals.active_visitors, 1);
        assert_eq!(data.minutes.iter().map(|m| m.counts.events).sum::<u64>(), 3);
        assert_eq!(data.breakdowns["campaigns"][0].name, "No campaign");
        assert_eq!(data.breakdowns["referrers"][0].name, "Direct");
    }
}
