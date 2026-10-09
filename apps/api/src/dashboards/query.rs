use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::{
    FunnelCondition, FunnelDefinition, FunnelPeriodRange, FunnelPropertyGroup, WidgetRequest,
    WidgetSource,
};
use crate::{models::clickhouse_string, retention::ACTIVE_ROW_PREDICATE};

pub(super) fn chart_conditions(
    site_id: &str,
    source: &WidgetSource,
    filters: &Value,
    range: &FunnelPeriodRange,
) -> String {
    let mut conditions = vec![
        format!("site_id = {}", clickhouse_string(site_id)),
        format!("occurred_at >= {}", clickhouse_datetime(range.start)),
        format!("occurred_at < {}", clickhouse_datetime(range.end)),
        ACTIVE_ROW_PREDICATE.to_owned(),
    ];
    conditions.push(if source.kind == "rule" {
        format!(
            "event_type = 'rule' AND rule_id = {}",
            clickhouse_string(&source.id)
        )
    } else {
        format!("event_name = {}", clickhouse_string(&source.id))
    });
    if let Some(page) = filters.get("page").and_then(Value::as_str) {
        if !page.is_empty() {
            conditions.push(format!("url_path = {}", clickhouse_string(page)));
        }
    }
    if let Some(country) = filters.get("country").and_then(Value::as_str) {
        if !country.is_empty() {
            conditions.push(format!("country = {}", clickhouse_string(country)));
        }
    }
    conditions.join(" AND ")
}

// Every chart shape and comparison uses the same metric and property scope.
pub(super) fn chart_scope(
    site_id: &str,
    request: &WidgetRequest,
    range: &FunnelPeriodRange,
) -> String {
    let mut scope = chart_conditions(
        site_id,
        request.source.as_ref().expect("validated chart source"),
        &request.filters,
        range,
    );
    if let Some(group) = &request.property_filters {
        scope.push_str(&format!(" AND ({})", property_group_sql(group, "")));
    }
    scope
}

pub(super) fn chart_metric(metric: &str, condition: &str) -> String {
    match metric {
        "visitors" => format!("uniqCombined64If(visitor_id, ({condition}) AND visitor_id != '')"),
        "sessions" => {
            format!("uniqCombined64If(anon_session_id, ({condition}) AND anon_session_id != '')")
        }
        _ => format!("countIf({condition})"),
    }
}

pub(super) fn preview_bucket_count(range: &FunnelPeriodRange, granularity: &str) -> usize {
    let unit_ms = if granularity == "hour" {
        3_600_000
    } else {
        86_400_000
    };
    let duration_ms = (range.end - range.start).num_milliseconds().max(1);
    usize::try_from((duration_ms + unit_ms - 1) / unit_ms)
        .unwrap_or(1)
        .clamp(1, 366)
}

pub(super) fn category_query(
    site_id: &str,
    request: &WidgetRequest,
    current: &FunnelPeriodRange,
    previous: Option<&FunnelPeriodRange>,
) -> String {
    let browser = crate::privacy::user_agent::browser_sql("browser_name");
    let referrer = crate::privacy::referrer::referrer_sql("referrer_host");
    let property = request
        .breakdown_property
        .as_deref()
        .map(|key| {
            format!(
                "if({0} = '', '(not set)', {0})",
                property_column_sql(key, "")
            )
        })
        .unwrap_or_default();
    let expression = match request.breakdown.as_str() {
        "browser" => browser.as_str(),
        "country" => "if(country = '', 'Unknown country', country)",
        "os" => "if(os_name = '', 'Unknown OS', os_name)",
        "city" => "if(city = '', 'Unknown city', concat(city, ' (', if(country = '', 'Unknown country', country), ')'))",
        "device" => "if(device_type = '', 'Unknown device', device_type)",
        "campaign" => "if(utm_campaign = '', 'No campaign', utm_campaign)",
        "source" => "if(utm_source = '', 'No source', utm_source)",
        "medium" => "if(utm_medium = '', 'No medium', utm_medium)",
        "referrer" => referrer.as_str(),
        "page" => "if(url_path = '', '/', url_path)",
        "property" => property.as_str(),
        _ => unreachable!("validated categorical breakdown"),
    };
    let span = FunnelPeriodRange {
        start: previous.map_or(current.start, |p| p.start),
        end: current.end,
        label: String::new(),
    };
    let scope = chart_scope(site_id, request, &span);
    let condition = |period: &FunnelPeriodRange| {
        format!(
            "occurred_at >= {} AND occurred_at < {}",
            clickhouse_datetime(period.start),
            clickhouse_datetime(period.end)
        )
    };
    let prior = previous.map(&condition).unwrap_or_else(|| "0".into());
    let current_metric = chart_metric(&request.metric, &condition(current));
    let previous_metric = chart_metric(&request.metric, &prior);
    // Compute the tail from source rows, so distinct visitors/sessions are
    // deduplicated across tail categories instead of adding category counts.
    format!(
        r#"WITH scoped AS (
        SELECT occurred_at, visitor_id, anon_session_id, {expression} AS category FROM owleye_events WHERE {scope}
    ), top_groups AS (
        SELECT category FROM scoped GROUP BY category
        ORDER BY {current_metric} + {previous_metric} DESC, category ASC LIMIT 20
    )
    SELECT if(is_other, 'Other', grouped_category) AS group_label,
        toUInt64({current_metric}) AS current_value,
        toUInt64({previous_metric}) AS previous_value
    FROM (
        SELECT occurred_at, visitor_id, anon_session_id, category, category NOT IN (SELECT category FROM top_groups) AS is_other FROM scoped
    )
    GROUP BY is_other, if(is_other, '', category) AS grouped_category
    ORDER BY is_other ASC, current_value + previous_value DESC, group_label ASC
    SETTINGS output_format_json_quote_64bit_integers = 0"#
    )
}

pub(super) fn preview_bucket_label(
    range: &FunnelPeriodRange,
    granularity: &str,
    bucket: usize,
    aligned: bool,
) -> String {
    if aligned {
        let unit = if granularity == "hour" { "Hour" } else { "Day" };
        return format!("{unit} {}", bucket + 1);
    }
    let value = if granularity == "hour" {
        range.start + Duration::hours(bucket as i64)
    } else {
        range.start + Duration::days(bucket as i64)
    };
    if granularity == "hour" {
        value.format("%H:%M").to_string()
    } else {
        value.format("%b %d").to_string()
    }
}

pub(super) fn funnel_query(
    site_id: &str,
    funnel: &FunnelDefinition,
    period: &FunnelPeriodRange,
    now: DateTime<Utc>,
) -> String {
    let cohort = if funnel.conversion_window == "same_session" {
        "anon_session_id"
    } else {
        "visitor_id"
    };
    let window_days = match funnel.conversion_window.as_str() {
        "30d" => 30,
        "7d" => 7,
        _ => 1,
    };
    let base_end = std::cmp::min(period.end + Duration::days(window_days), now);
    let start = clickhouse_datetime(period.start);
    let end = clickhouse_datetime(period.end);
    let base_end = clickhouse_datetime(base_end);
    let site_id = clickhouse_string(site_id);
    let epoch = "toDateTime64(0, 3, 'UTC')";
    let first_condition = funnel_condition_sql(&funnel.steps[0].condition, None);

    let mut ctes = vec![format!(
        r#"base_raw AS (
            SELECT
                {cohort} AS cohort_id,
                event_id,
                occurred_at,
                event_type,
                event_name,
                rule_id,
                url_path,
                utm_source,
                utm_medium,
                utm_campaign,
                country,
                region,
                browser_name,
                os_name,
                device_type,
                payload_json
            FROM owleye_events
            WHERE site_id = {site_id}
                AND {cohort} != ''
                AND occurred_at >= {start}
                AND occurred_at < {base_end}
                AND {ACTIVE_ROW_PREDICATE}
        )"#
    )];
    ctes.push(
        r#"base AS (
            SELECT
                *,
                row_number() OVER (
                    PARTITION BY cohort_id ORDER BY occurred_at ASC, event_id ASC
                ) AS sequence_index
            FROM base_raw
        )"#
        .to_owned(),
    );
    ctes.push(format!(
        r#"step_1 AS (
            SELECT
                cohort_id,
                minIf(sequence_index, ({first_condition}) AND occurred_at >= {start} AND occurred_at < {end}) AS step_1_index,
                minIf(occurred_at, ({first_condition}) AND occurred_at >= {start} AND occurred_at < {end}) AS step_1_at
            FROM base
            GROUP BY cohort_id
        )"#
    ));

    for index in 2..=funnel.steps.len() {
        let previous = index - 1;
        let prior_fields = (1..index)
            .flat_map(|step| {
                [
                    format!("previous.step_{step}_index"),
                    format!("previous.step_{step}_at"),
                ]
            })
            .collect::<Vec<_>>();
        let group_fields = prior_fields.join(", ");
        let condition = funnel_condition_sql(&funnel.steps[index - 1].condition, Some("events"));
        let progression = if funnel.entry_mode == "open" {
            format!(
                "events.occurred_at >= {start} AND events.occurred_at < {end} AND ({condition})"
            )
        } else {
            let order = if funnel.order_mode == "exact" {
                format!("events.sequence_index = previous.step_{previous}_index + 1")
            } else {
                format!("events.sequence_index > previous.step_{previous}_index")
            };
            format!(
                "previous.step_{previous}_at > {epoch} AND {order} \
                 AND events.occurred_at <= previous.step_1_at + {} AND ({condition})",
                window_interval_sql(&funnel.conversion_window)
            )
        };
        ctes.push(format!(
            r#"step_{index} AS (
                SELECT
                    previous.cohort_id,
                    {group_fields},
                    minIf(events.sequence_index, {progression}) AS step_{index}_index,
                    minIf(
                        events.occurred_at,
                        {progression}
                    ) AS step_{index}_at
                FROM step_{previous} AS previous
                LEFT JOIN base AS events ON events.cohort_id = previous.cohort_id
                GROUP BY previous.cohort_id, {group_fields}
            )"#
        ));
    }

    let mut metrics = (1..=funnel.steps.len())
        .map(|index| format!("toUInt64(countIf(step_{index}_at > {epoch})) AS step_{index}_count"))
        .collect::<Vec<_>>();
    metrics.extend((1..funnel.steps.len()).flat_map(|index| {
        let next = index + 1;
        let condition = format!("step_{index}_at > {epoch} AND step_{next}_at > {epoch}");
        [
            format!(
                "toUInt64(ifNotFinite(avgIf(toFloat64(dateDiff('millisecond', step_{index}_at, step_{next}_at)), {condition}), 0)) AS step_{index}_avg_ms"
            ),
            format!(
                "toUInt64(quantileExactIf(0.5)(toUInt64(dateDiff('millisecond', step_{index}_at, step_{next}_at)), {condition})) AS step_{index}_median_ms"
            ),
        ]
    }));
    let metrics = metrics.join(",\n            ");
    format!(
        "WITH\n{}\nSELECT\n            {metrics}\nFROM step_{}\nSETTINGS output_format_json_quote_64bit_integers = 0",
        ctes.join(",\n"),
        funnel.steps.len()
    )
}

fn window_interval_sql(window: &str) -> &'static str {
    match window {
        "1h" => "toIntervalHour(1)",
        "7d" => "toIntervalDay(7)",
        "30d" => "toIntervalDay(30)",
        _ => "toIntervalDay(1)",
    }
}

fn funnel_condition_sql(condition: &FunnelCondition, alias: Option<&str>) -> String {
    let prefix = alias.map(|alias| format!("{alias}.")).unwrap_or_default();
    let (column, type_filter) = match condition.kind.as_str() {
        "page" => (
            format!("{prefix}url_path"),
            Some(format!("{prefix}event_type = 'pageview'")),
        ),
        "rule" => (
            format!("{prefix}rule_id"),
            Some(format!("{prefix}event_type = 'rule'")),
        ),
        _ => (format!("{prefix}event_name"), None),
    };
    let value = clickhouse_string(&condition.id);
    let matcher = if condition.operator == "starts_with" {
        format!("startsWith({column}, {value})")
    } else {
        format!("{column} = {value}")
    };
    let base = type_filter
        .map(|filter| format!("{filter} AND {matcher}"))
        .unwrap_or(matcher);
    condition
        .property_filters
        .as_ref()
        .map(|properties| format!("{base} AND ({})", property_group_sql(properties, &prefix)))
        .unwrap_or(base)
}

fn property_group_sql(group: &FunnelPropertyGroup, prefix: &str) -> String {
    let joiner = if group.logic == "or" { " OR " } else { " AND " };
    group
        .filters
        .iter()
        .map(|filter| {
            let column = property_column_sql(&filter.key, prefix);
            let raw = filter.value.as_deref().unwrap_or_default();
            let normalized = (filter.key == "browser"
                && matches!(filter.operator.as_str(), "equals" | "not_equals"))
            .then(|| crate::privacy::user_agent::canonical_browser(raw))
            .flatten();
            let value = clickhouse_string(normalized.as_deref().unwrap_or(raw));
            // Normalize only numeric JSON properties, leaving strings such as
            // "001.50" untouched. This also makes numeric filter 1.50 match 1.5.
            let value = if matches!(filter.operator.as_str(), "equals" | "not_equals") && !matches!(filter.key.as_str(), "campaign"|"utm_campaign"|"source"|"utm_source"|"medium"|"utm_medium"|"country"|"region"|"browser"|"os"|"device"|"page"|"path") {
                format!("if(JSONType({prefix}payload_json, 'records', {}) = 'Double', ifNull(toString(toFloat64OrNull({value})), {value}), {value})",clickhouse_string(&filter.key))
            } else { value };
            match filter.operator.as_str() {
                "contains" => format!("positionCaseInsensitiveUTF8({column}, {value}) > 0"),
                "exists" => format!("{column} != ''"),
                "not_equals" => format!("{column} != {value}"),
                _ => format!("{column} = {value}"),
            }
        })
        .collect::<Vec<_>>()
        .join(joiner)
}

fn property_column_sql(key: &str, prefix: &str) -> String {
    if key == "browser" {
        return format!(
            "if({prefix}browser_name = '', '', {})",
            crate::privacy::user_agent::browser_sql(&format!("{prefix}browser_name"))
        );
    }
    let column = match key {
        "campaign" | "utm_campaign" => Some("utm_campaign"),
        "source" | "utm_source" => Some("utm_source"),
        "medium" | "utm_medium" => Some("utm_medium"),
        "country" => Some("country"),
        "region" => Some("region"),
        "browser" => Some("browser_name"),
        "os" => Some("os_name"),
        "device" => Some("device_type"),
        "page" | "path" => Some("url_path"),
        _ => None,
    };
    column
        .map(|column| format!("{prefix}{column}"))
        .unwrap_or_else(|| {
            format!(
                "multiIf(JSONType({prefix}payload_json, 'records', {key}) = 'String', JSONExtractString({prefix}payload_json, 'records', {key}), JSONType({prefix}payload_json, 'records', {key}) = 'Double', toString(JSONExtractFloat({prefix}payload_json, 'records', {key})), JSONType({prefix}payload_json, 'records', {key}) IN ('Int64', 'UInt64', 'Bool'), JSONExtractRaw({prefix}payload_json, 'records', {key}), '')",
                key = clickhouse_string(key)
            )
        })
}

pub(super) fn clickhouse_datetime(value: DateTime<Utc>) -> String {
    format!(
        "toDateTime64({}, 3, 'UTC')",
        clickhouse_string(&value.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboards::{FunnelPropertyFilter, FunnelStep};

    fn period() -> FunnelPeriodRange {
        FunnelPeriodRange {
            start: "2026-08-01T00:00:00Z".parse().unwrap(),
            end: "2026-08-08T00:00:00Z".parse().unwrap(),
            label: "test".to_owned(),
        }
    }

    #[test]
    fn chart_conditions_escape_every_dynamic_value() {
        let sql = chart_conditions(
            "owl'site",
            &WidgetSource {
                has_location: false,
                id: "signup'event".to_owned(),
                kind: "event".to_owned(),
                name: "Signup".to_owned(),
            },
            &serde_json::json!({
                "country": "U'S",
                "page": "/price's"
            }),
            &period(),
        );

        for escaped in [
            "'owl\\'site'",
            "'signup\\'event'",
            "'U\\'S'",
            "'/price\\'s'",
        ] {
            assert!(
                sql.contains(escaped),
                "missing escaped value {escaped}: {sql}"
            );
        }
    }

    #[test]
    fn funnel_query_escapes_condition_ids_property_keys_and_values() {
        let funnel = FunnelDefinition {
            conversion_window: "1d".to_owned(),
            entry_mode: "closed".to_owned(),
            order_mode: "ordered".to_owned(),
            steps: vec![
                FunnelStep {
                    condition: FunnelCondition {
                        id: "page'one".to_owned(),
                        kind: "page".to_owned(),
                        name: "Page one".to_owned(),
                        operator: "exact".to_owned(),
                        property_filters: Some(FunnelPropertyGroup {
                            filters: vec![FunnelPropertyFilter {
                                id: "filter".to_owned(),
                                key: "custom'key".to_owned(),
                                operator: "equals".to_owned(),
                                value: Some("value'quoted".to_owned()),
                            }],
                            logic: "and".to_owned(),
                        }),
                    },
                    id: "step-1".to_owned(),
                    name: "Step one".to_owned(),
                },
                FunnelStep {
                    condition: FunnelCondition {
                        id: "signup'complete".to_owned(),
                        kind: "event".to_owned(),
                        name: "Signup".to_owned(),
                        operator: "starts_with".to_owned(),
                        property_filters: None,
                    },
                    id: "step-2".to_owned(),
                    name: "Step two".to_owned(),
                },
            ],
        };

        let sql = funnel_query(
            "owl'site",
            &funnel,
            &period(),
            "2026-08-09T00:00:00Z".parse().unwrap(),
        );
        for escaped in [
            "'owl\\'site'",
            "'page\\'one'",
            "'custom\\'key'",
            "'value\\'quoted'",
            "'signup\\'complete'",
        ] {
            assert!(
                sql.contains(escaped),
                "missing escaped value {escaped}: {sql}"
            );
        }
    }
}

#[cfg(test)]
mod property_value_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires loopback OWLEYE_TEST_CLICKHOUSE_URL; synthetic property values only"]
    async fn real_clickhouse_decimal_boolean_and_string_properties() {
        let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").unwrap();
        assert!(matches!(
            reqwest::Url::parse(&url).unwrap().host_str(),
            Some("localhost" | "127.0.0.1")
        ));
        let ch = crate::storage::clickhouse::ClickHouse::new(url).unwrap();
        for (json_value, expected) in [
            ("9.99", "9.99"),
            ("1.50", "1.5"),
            ("true", "true"),
            ("false", "false"),
            ("\"001.50\"", "001.50"),
            ("null", ""),
        ] {
            let payload = clickhouse_string(&format!("{{\"records\":{{\"price\":{json_value}}}}}"));
            let expression = property_column_sql("price", "");
            let sql =
                format!("SELECT {expression} AS value FROM (SELECT {payload} AS payload_json)");
            let rows = ch.query_json_each_row::<Value>(&sql).await.unwrap();
            assert_eq!(rows[0]["value"], expected, "{json_value}");
        }
        for value in ["1.5", "1.50"] {
            let group = FunnelPropertyGroup {
                logic: "and".into(),
                filters: vec![super::super::FunnelPropertyFilter {
                    id: "p".into(),
                    key: "price".into(),
                    operator: "equals".into(),
                    value: Some(value.into()),
                }],
            };
            let predicate = property_group_sql(&group, "");
            let rows = ch.query_json_each_row::<Value>(&format!("SELECT count() AS count FROM (SELECT '{{\"records\":{{\"price\":1.50}}}}' AS payload_json) WHERE {predicate}")).await.unwrap();
            assert_eq!(rows[0]["count"], 1, "{value}");
        }
        for value in ["true", "false"] {
            let group = FunnelPropertyGroup {
                logic: "and".into(),
                filters: vec![super::super::FunnelPropertyFilter {
                    id: "p".into(),
                    key: "active".into(),
                    operator: "equals".into(),
                    value: Some(value.into()),
                }],
            };
            let predicate = property_group_sql(&group, "");
            let rows = ch.query_json_each_row::<Value>(&format!("SELECT count() AS count FROM (SELECT '{{\"records\":{{\"active\":{value}}}}}' AS payload_json) WHERE {predicate}")).await.unwrap();
            assert_eq!(rows[0]["count"], 1, "{value}");
        }
        ch.shutdown().await;
    }
}
