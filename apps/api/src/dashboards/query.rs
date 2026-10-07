use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use super::{
    FunnelCondition, FunnelDefinition, FunnelPeriodRange, FunnelPropertyGroup, WidgetSource,
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
    source: &WidgetSource,
    filters: &Value,
    breakdown: &str,
    current: &FunnelPeriodRange,
    previous: Option<&FunnelPeriodRange>,
) -> String {
    let browser = crate::privacy::user_agent::browser_sql("browser_name");
    let referrer = crate::privacy::referrer::referrer_sql("referrer_host");
    let expression = match breakdown {
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
        _ => unreachable!("validated categorical breakdown"),
    };
    let span = FunnelPeriodRange {
        start: previous.map_or(current.start, |p| p.start),
        end: current.end,
        label: String::new(),
    };
    let scope = chart_conditions(site_id, source, filters, &span);
    let condition = |period: &FunnelPeriodRange| {
        format!(
            "occurred_at >= {} AND occurred_at < {}",
            clickhouse_datetime(period.start),
            clickhouse_datetime(period.end)
        )
    };
    let prior = previous.map(&condition).unwrap_or_else(|| "0".into());
    format!("SELECT {expression} AS group_label, toUInt64(countIf({})) AS current_value, toUInt64(countIf({prior})) AS previous_value FROM owleye_events WHERE {scope} GROUP BY group_label ORDER BY current_value + previous_value DESC, group_label ASC LIMIT 20 SETTINGS output_format_json_quote_64bit_integers = 0",condition(current))
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
                "JSONExtractString(JSONExtractRaw({prefix}payload_json, 'records'), {})",
                clickhouse_string(key)
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
