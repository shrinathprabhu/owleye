use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    auth,
    demo::{self, DemoSiteDefinition, DemoTrafficProfile},
    models::clickhouse_string,
    retention::ACTIVE_ROW_PREDICATE,
    sites, ApiError, AppState,
};

const DEFAULT_DAYS: u16 = 30;
const MAX_DAYS: u16 = 365;
const DEFAULT_LIMIT: u16 = 50;
const MAX_LIMIT: u16 = 100;

#[derive(Debug, Deserialize)]
pub(crate) struct EventExplorerQuery {
    #[serde(default)]
    days: Option<u16>,
    #[serde(default)]
    event_name: Option<String>,
    #[serde(default)]
    event_type: Option<String>,
    #[serde(default)]
    group_by: Option<String>,
    #[serde(default)]
    limit: Option<u16>,
    #[serde(default)]
    order: Option<String>,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    sort_by: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct EventExplorerItem {
    count: u64,
    event_name: String,
    event_type: String,
    first_seen_at: String,
    key: String,
    last_seen_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct EventExplorerResponse {
    available_groupings: &'static [&'static str],
    days: u16,
    group_by: String,
    items: Vec<EventExplorerItem>,
    read_only: bool,
    site_id: String,
}

#[derive(Clone, Copy)]
enum GroupBy {
    Browser,
    Country,
    Date,
    Device,
    EventName,
    EventType,
    OperatingSystem,
    Page,
}

impl GroupBy {
    fn parse(value: Option<&str>) -> Result<Self, ApiError> {
        match value.unwrap_or("event_name") {
            "browser" => Ok(Self::Browser),
            "country" => Ok(Self::Country),
            "date" => Ok(Self::Date),
            "device" => Ok(Self::Device),
            "event_name" => Ok(Self::EventName),
            "event_type" => Ok(Self::EventType),
            "operating_system" => Ok(Self::OperatingSystem),
            "page" => Ok(Self::Page),
            _ => Err(ApiError::BadRequest(
                "Unsupported event grouping".to_owned(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Country => "country",
            Self::Date => "date",
            Self::Device => "device",
            Self::EventName => "event_name",
            Self::EventType => "event_type",
            Self::OperatingSystem => "operating_system",
            Self::Page => "page",
        }
    }

    fn expression(self) -> &'static str {
        match self {
            Self::Browser => "if(browser_name = '', 'Unknown', browser_name)",
            Self::Country => "if(country = '', 'Unknown', country)",
            Self::Date => "toString(toDate(occurred_at))",
            Self::Device => "if(device_type = '', 'Unknown', device_type)",
            Self::EventName => "event_name",
            Self::EventType => "event_type",
            Self::OperatingSystem => "if(os_name = '', 'Unknown', os_name)",
            Self::Page => "if(url_path = '', '/', url_path)",
        }
    }
}

pub(crate) async fn explore_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Query(query): Query<EventExplorerQuery>,
) -> Result<Json<EventExplorerResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    let days = query.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);
    let group_by = GroupBy::parse(query.group_by.as_deref())?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    validate_optional_filter(query.event_name.as_deref(), "event_name", 128)?;
    validate_optional_filter(query.event_type.as_deref(), "event_type", 32)?;
    validate_optional_filter(query.search.as_deref(), "search", 128)?;
    let sort = normalize_sort(query.sort_by.as_deref(), query.order.as_deref())?;

    let (site_id, mut items) = if auth::is_demo_session(&session) {
        let site = demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        (
            site.tracking_id.to_owned(),
            demo_event_items(site, group_by, days),
        )
    } else {
        let site =
            sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
        let sql = explorer_query(&site.tracking_id, days, group_by, &query, sort, limit);
        let items = state
            .clickhouse
            .query_json_each_row::<EventExplorerItem>(&sql)
            .await?;
        (site.tracking_id, items)
    };

    if auth::is_demo_session(&session) {
        filter_demo_items(&mut items, &query);
        sort_demo_items(&mut items, sort);
        items.truncate(usize::from(limit));
    }

    Ok(Json(EventExplorerResponse {
        available_groupings: &[
            "event_name",
            "event_type",
            "date",
            "page",
            "country",
            "browser",
            "operating_system",
            "device",
        ],
        days,
        group_by: group_by.as_str().to_owned(),
        items,
        read_only: true,
        site_id,
    }))
}

#[derive(Clone, Copy)]
enum SortBy {
    CountAsc,
    CountDesc,
    FirstSeenAsc,
    FirstSeenDesc,
    KeyAsc,
    KeyDesc,
    LastSeenAsc,
    LastSeenDesc,
}

fn normalize_sort(sort_by: Option<&str>, order: Option<&str>) -> Result<SortBy, ApiError> {
    let descending = match order.unwrap_or("desc") {
        "asc" => false,
        "desc" => true,
        _ => return Err(ApiError::BadRequest("order must be asc or desc".to_owned())),
    };
    match (sort_by.unwrap_or("count"), descending) {
        ("count", false) => Ok(SortBy::CountAsc),
        ("count", true) => Ok(SortBy::CountDesc),
        ("first_seen_at", false) => Ok(SortBy::FirstSeenAsc),
        ("first_seen_at", true) => Ok(SortBy::FirstSeenDesc),
        ("key", false) => Ok(SortBy::KeyAsc),
        ("key", true) => Ok(SortBy::KeyDesc),
        ("last_seen_at", false) => Ok(SortBy::LastSeenAsc),
        ("last_seen_at", true) => Ok(SortBy::LastSeenDesc),
        _ => Err(ApiError::BadRequest("Unsupported event sort".to_owned())),
    }
}

fn sort_sql(sort: SortBy) -> &'static str {
    match sort {
        SortBy::CountAsc => "count ASC, key ASC",
        SortBy::CountDesc => "count DESC, key ASC",
        SortBy::FirstSeenAsc => "first_seen_at ASC, key ASC",
        SortBy::FirstSeenDesc => "first_seen_at DESC, key ASC",
        SortBy::KeyAsc => "key ASC",
        SortBy::KeyDesc => "key DESC",
        SortBy::LastSeenAsc => "last_seen_at ASC, key ASC",
        SortBy::LastSeenDesc => "last_seen_at DESC, key ASC",
    }
}

fn explorer_query(
    site_id: &str,
    days: u16,
    group_by: GroupBy,
    query: &EventExplorerQuery,
    sort: SortBy,
    limit: u16,
) -> String {
    let site_id = clickhouse_string(site_id);
    let mut filters = vec![
        format!("site_id = {site_id}"),
        format!("occurred_at >= now() - INTERVAL {days} DAY"),
        ACTIVE_ROW_PREDICATE.to_owned(),
    ];
    if let Some(event_type) = query.event_type.as_deref() {
        filters.push(format!("event_type = {}", clickhouse_string(event_type)));
    }
    if let Some(event_name) = query.event_name.as_deref() {
        filters.push(format!("event_name = {}", clickhouse_string(event_name)));
    }
    if let Some(search) = query.search.as_deref() {
        filters.push(format!(
            "positionCaseInsensitiveUTF8(event_name, {}) > 0",
            clickhouse_string(search)
        ));
    }
    // Keep representative aliases distinct from input column names. ClickHouse
    // substitutes same-named aliases into GROUP BY and WHERE expressions.
    let expression = group_by.expression();
    let order = sort_sql(sort);
    format!(
        r#"
        SELECT
            key,
            representative_type AS event_type,
            representative_name AS event_name,
            count, first_seen_at, last_seen_at
        FROM (
            SELECT
                {expression} AS key,
                any(event_type) AS representative_type,
                any(event_name) AS representative_name,
                toUInt64(count()) AS count,
                toString(min(occurred_at)) AS first_seen_at,
                toString(max(occurred_at)) AS last_seen_at
            FROM owleye_events
            WHERE {filters}
            GROUP BY key
        )
        ORDER BY {order}
        LIMIT {limit}
        SETTINGS output_format_json_quote_64bit_integers = 0
        "#,
        filters = filters.join(" AND ")
    )
}

fn validate_optional_filter(
    value: Option<&str>,
    field: &str,
    max_bytes: usize,
) -> Result<(), ApiError> {
    if let Some(value) = value {
        if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control)
        {
            return Err(ApiError::BadRequest(format!(
                "{field} must be between 1 and {max_bytes} characters"
            )));
        }
    }
    Ok(())
}

fn demo_event_items(
    site: &DemoSiteDefinition,
    group_by: GroupBy,
    days: u16,
) -> Vec<EventExplorerItem> {
    let scale = match site.profile {
        DemoTrafficProfile::Massive => 82_400,
        DemoTrafficProfile::Campaign => 37_600,
        DemoTrafficProfile::Mixed => 11_200,
        DemoTrafficProfile::LowVolume => 410,
        DemoTrafficProfile::New => 64,
    } * u64::from(days.min(if site.profile == DemoTrafficProfile::New {
        7
    } else {
        days
    }));
    let groups: &[(&str, &str, &str, u64)] = match group_by {
        GroupBy::EventName => &[
            ("page_viewed", "pageview", "page_viewed", 100),
            ("signup_clicked", "external", "signup_clicked", 31),
            ("pricing_cta_clicked", "rule", "pricing_cta_clicked", 18),
            ("checkout_completed", "external", "checkout_completed", 7),
        ],
        GroupBy::EventType => &[
            ("pageview", "pageview", "page_viewed", 100),
            ("external", "external", "signup_clicked", 39),
            ("rule", "rule", "pricing_cta_clicked", 21),
            ("performance", "performance", "web_vital", 12),
        ],
        GroupBy::Page => &[
            ("/", "pageview", "page_viewed", 100),
            ("/pricing", "pageview", "page_viewed", 72),
            ("/docs", "pageview", "page_viewed", 51),
            ("/signup", "external", "signup_clicked", 24),
        ],
        GroupBy::Country => &[
            ("United States", "pageview", "page_viewed", 100),
            ("India", "pageview", "page_viewed", 72),
            ("United Kingdom", "pageview", "page_viewed", 44),
            ("Germany", "pageview", "page_viewed", 27),
        ],
        GroupBy::Browser => &[
            ("Chrome", "pageview", "page_viewed", 100),
            ("Safari", "pageview", "page_viewed", 61),
            ("Firefox", "pageview", "page_viewed", 21),
            ("Edge", "pageview", "page_viewed", 17),
        ],
        GroupBy::OperatingSystem => &[
            ("macOS", "pageview", "page_viewed", 100),
            ("Windows", "pageview", "page_viewed", 73),
            ("iOS", "pageview", "page_viewed", 39),
            ("Android", "pageview", "page_viewed", 35),
        ],
        GroupBy::Device => &[
            ("Desktop", "pageview", "page_viewed", 100),
            ("Mobile", "pageview", "page_viewed", 68),
            ("Tablet", "pageview", "page_viewed", 11),
        ],
        GroupBy::Date => &[
            ("today", "pageview", "page_viewed", 100),
            ("yesterday", "pageview", "page_viewed", 91),
            ("2 days ago", "pageview", "page_viewed", 86),
        ],
    };
    let now = Utc::now();
    groups
        .iter()
        .enumerate()
        .map(
            |(index, (key, event_type, event_name, weight))| EventExplorerItem {
                count: scale.saturating_mul(*weight) / 100,
                event_name: (*event_name).to_owned(),
                event_type: (*event_type).to_owned(),
                first_seen_at: (now - Duration::days(i64::from(days)))
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
                key: if matches!(group_by, GroupBy::Date) {
                    (now - Duration::days(index as i64))
                        .date_naive()
                        .to_string()
                } else {
                    (*key).to_owned()
                },
                last_seen_at: (now - Duration::minutes(index as i64 * 11))
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            },
        )
        .collect()
}

fn filter_demo_items(items: &mut Vec<EventExplorerItem>, query: &EventExplorerQuery) {
    items.retain(|item| {
        query
            .event_type
            .as_deref()
            .is_none_or(|value| item.event_type == value)
            && query
                .event_name
                .as_deref()
                .is_none_or(|value| item.event_name == value)
            && query.search.as_deref().is_none_or(|value| {
                item.event_name
                    .to_ascii_lowercase()
                    .contains(&value.to_ascii_lowercase())
            })
    });
}

fn sort_demo_items(items: &mut [EventExplorerItem], sort: SortBy) {
    items.sort_by(|left, right| match sort {
        SortBy::CountAsc => left.count.cmp(&right.count).then(left.key.cmp(&right.key)),
        SortBy::CountDesc => right.count.cmp(&left.count).then(left.key.cmp(&right.key)),
        SortBy::FirstSeenAsc => left
            .first_seen_at
            .cmp(&right.first_seen_at)
            .then(left.key.cmp(&right.key)),
        SortBy::FirstSeenDesc => right
            .first_seen_at
            .cmp(&left.first_seen_at)
            .then(left.key.cmp(&right.key)),
        SortBy::KeyAsc => left.key.cmp(&right.key),
        SortBy::KeyDesc => right.key.cmp(&left.key),
        SortBy::LastSeenAsc => left
            .last_seen_at
            .cmp(&right.last_seen_at)
            .then(left.key.cmp(&right.key)),
        SortBy::LastSeenDesc => right
            .last_seen_at
            .cmp(&left.last_seen_at)
            .then(left.key.cmp(&right.key)),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_contract_only_uses_allowlisted_sql_fragments() {
        let query = EventExplorerQuery {
            days: Some(7),
            event_name: Some("signup'clicked".to_owned()),
            event_type: Some("external".to_owned()),
            group_by: Some("country".to_owned()),
            limit: Some(10),
            order: Some("desc".to_owned()),
            search: Some("signup".to_owned()),
            sort_by: Some("count".to_owned()),
        };
        let sql = explorer_query(
            "owl'quoted",
            7,
            GroupBy::Country,
            &query,
            SortBy::CountDesc,
            10,
        );
        assert!(sql.contains("site_id = 'owl\\'quoted'"));
        assert!(sql.contains("event_name = 'signup\\'clicked'"));
        assert!(sql.contains("GROUP BY key"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains(ACTIVE_ROW_PREDICATE));
    }

    #[tokio::test]
    #[ignore = "requires an isolated OWLEYE_TEST_CLICKHOUSE_URL; run with --ignored"]
    async fn real_clickhouse_explorer_groups_filters_and_sorts_without_alias_collisions() {
        use crate::storage::clickhouse::ClickHouse;
        let url =
            std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("set an isolated ClickHouse URL");
        let clickhouse = ClickHouse::new(url.clone()).unwrap();
        clickhouse.init().await.unwrap();
        let site = format!("explorer-test-{}", uuid::Uuid::new_v4());
        let rows = [
            (site.as_str(), "pageview", "page_view", 0, 1),
            (site.as_str(), "pageview", "page_view", 1, 1),
            (site.as_str(), "external", "signup'clicked", 2, 1),
            (site.as_str(), "external", "expired", 0, -1),
            (site.as_str(), "external", "outside_range", 40, 1),
            ("other-explorer-site", "external", "other_account", 0, 1),
        ];
        let values = rows.iter().map(|(id, kind, name, days, active)| {
            format!("({}, {}, {}, {}, now64(3) - INTERVAL {} DAY, now64(3) + INTERVAL {} DAY, '/pricing', 'IN', 'Firefox', 'Linux', 'Desktop')",
                clickhouse_string(&uuid::Uuid::new_v4().to_string()), clickhouse_string(id), clickhouse_string(kind), clickhouse_string(name), days, active)
        }).collect::<Vec<_>>().join(",");
        reqwest::Client::new().post(&url).body(format!("INSERT INTO owleye_events (event_id,site_id,event_type,event_name,occurred_at,retention_active_until,url_path,country,browser_name,os_name,device_type) VALUES {values}"))
            .send().await.unwrap().error_for_status().unwrap();
        for group in [
            GroupBy::EventName,
            GroupBy::EventType,
            GroupBy::Date,
            GroupBy::Page,
            GroupBy::Country,
            GroupBy::Browser,
            GroupBy::OperatingSystem,
            GroupBy::Device,
        ] {
            for sort in [
                SortBy::CountAsc,
                SortBy::CountDesc,
                SortBy::FirstSeenAsc,
                SortBy::FirstSeenDesc,
                SortBy::LastSeenAsc,
                SortBy::LastSeenDesc,
                SortBy::KeyAsc,
                SortBy::KeyDesc,
            ] {
                let query: EventExplorerQuery =
                    serde_json::from_value(serde_json::json!({})).unwrap();
                let items = clickhouse
                    .query_json_each_row::<EventExplorerItem>(&explorer_query(
                        &site, 30, group, &query, sort, 100,
                    ))
                    .await
                    .unwrap();
                assert_eq!(
                    items.iter().map(|item| item.count).sum::<u64>(),
                    3,
                    "{} must exclude other sites, inactive rows, and out-of-range events",
                    group.as_str()
                );
                assert!(items
                    .iter()
                    .all(|item| !item.first_seen_at.is_empty() && !item.last_seen_at.is_empty()));
                let mut expected = serde_json::to_value(&items).unwrap();
                let mut sorted: Vec<EventExplorerItem> =
                    serde_json::from_value(expected.clone()).unwrap();
                sort_demo_items(&mut sorted, sort);
                assert_eq!(expected, serde_json::to_value(sorted).unwrap());
                // Exact filters and substring search used to substitute aggregate
                // aliases into WHERE, including when grouping by another field.
                for filter in [
                    serde_json::json!({"event_type":"external"}),
                    serde_json::json!({"event_name":"signup'clicked"}),
                    serde_json::json!({"search":"SIGNUP"}),
                ] {
                    let query = serde_json::from_value(filter).unwrap();
                    let items = clickhouse
                        .query_json_each_row::<EventExplorerItem>(&explorer_query(
                            &site, 30, group, &query, sort, 100,
                        ))
                        .await
                        .unwrap();
                    assert_eq!(items.len(), 1);
                    assert_eq!(items[0].count, 1);
                    assert_eq!(items[0].event_name, "signup'clicked");
                }
                let query =
                    serde_json::from_value(serde_json::json!({"search":"no-such-event"})).unwrap();
                expected = serde_json::to_value(
                    clickhouse
                        .query_json_each_row::<EventExplorerItem>(&explorer_query(
                            &site, 30, group, &query, sort, 1,
                        ))
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(expected, serde_json::json!([]));
            }
        }
        let query = serde_json::from_value(serde_json::json!({})).unwrap();
        let items = clickhouse
            .query_json_each_row::<EventExplorerItem>(&explorer_query(
                &site,
                30,
                GroupBy::EventName,
                &query,
                SortBy::CountDesc,
                1,
            ))
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].count, 2);
    }

    #[test]
    fn demo_explorer_is_nonempty_for_every_grouping() {
        let site = &demo::DEMO_SITES[0];
        for group in [
            GroupBy::Browser,
            GroupBy::Country,
            GroupBy::Date,
            GroupBy::Device,
            GroupBy::EventName,
            GroupBy::EventType,
            GroupBy::OperatingSystem,
            GroupBy::Page,
        ] {
            assert!(!demo_event_items(site, group, 30).is_empty());
        }
    }
}
