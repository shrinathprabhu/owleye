//! Version 2 closed funnels use a bounded set of attempts instead of anchoring
//! every cohort to its first entry. Only aggregate metrics leave ClickHouse.
use super::*;

pub(super) fn query(
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
    let start = clickhouse_datetime(period.start);
    let end = clickhouse_datetime(period.end);
    let base_end =
        clickhouse_datetime(std::cmp::min(period.end + Duration::days(window_days), now));
    let site_id = clickhouse_string(site_id);
    let flags = funnel
        .steps
        .iter()
        .map(|step| format!("toUInt8({})", funnel_condition_sql(&step.condition, None)))
        .collect::<Vec<_>>()
        .join(", ");
    let step_count = funnel.steps.len();
    // Native tables use UUID event IDs; normalize the bounded sequence's tie
    // breaker to the String type in the attempt accumulator (legacy fixtures
    // and development tables may already expose String IDs).
    let order = if funnel.order_mode == "exact" {
        "i = arrayElement(state.2, -1) + 1"
    } else {
        "i > arrayElement(state.2, -1)"
    };
    let interval = window_interval_sql(&funnel.conversion_window);
    let mut metrics = Vec::new();
    for step in 1..=step_count {
        metrics.push(format!(
            "toUInt64(countIf(selected.1 >= {step})) AS step_{step}_count"
        ));
        metrics.push(format!(
            "toUInt64(uniqExactIf(events[arrayElement(selected.2, {step})].3, selected.1 >= {step} AND events[arrayElement(selected.2, {step})].3 != '')) AS step_{step}_visitors"
        ));
        if step < step_count {
            let next = step + 1;
            let elapsed = format!(
                "dateDiff('millisecond', events[arrayElement(selected.2, {step})].1, events[arrayElement(selected.2, {next})].1)"
            );
            metrics.push(format!(
                "toUInt64(ifNotFinite(avgIf(toFloat64({elapsed}), selected.1 >= {next}), 0)) AS step_{step}_avg_ms"
            ));
            metrics.push(format!(
                "toUInt64(quantileExactIf(0.5)(toUInt64(greatest(0, {elapsed})), selected.1 >= {next})) AS step_{step}_median_ms"
            ));
        }
    }
    let metrics = metrics.join(",\n    ");
    let population = crate::report_population::EXCLUDE_KNOWN_CRAWLERS_SQL;
    format!(
        r#"WITH
eligible AS (
    SELECT {cohort} AS cohort_id, event_id,
        min(tuple(occurred_at, visitor_id, [{flags}])) AS action
    FROM owleye_events
    WHERE site_id = {site_id}
        AND {cohort} != ''
        AND occurred_at >= {start} AND occurred_at < {base_end}
        AND {ACTIVE_ROW_PREDICATE}
        AND {population}
        AND event_type IN ('pageview', 'external', 'rule')
    GROUP BY cohort_id, event_id
),
sequences AS (
    SELECT cohort_id, count() AS action_count,
        countIf(action.3[1] = 1 AND action.1 >= {start} AND action.1 < {end}) AS start_count,
        arraySort(e -> (e.1, e.2), groupArray(4097)(tuple(action.1, toString(event_id), action.2, action.3))) AS events
    FROM eligible
    GROUP BY cohort_id
    HAVING countIf(action.3[1] = 1 AND action.1 >= {start} AND action.1 < {end}) > 0
),
bounded AS (
    SELECT *, sum(length(events) * start_count) OVER () AS candidate_work
    FROM sequences
),
attempts AS (
    SELECT events,
        arrayFold(
            (active, event, i) -> arrayConcat(
                arrayMap(state -> if(
                    state.1 < {step_count} AND {order}
                    AND event.1 <= state.3 + {interval}
                    AND arrayElement(event.4, state.1 + 1) = 1,
                    tuple(toUInt8(state.1 + 1), arrayPushBack(state.2, toUInt64(i)), state.3, state.4),
                    state
                ), active),
                if(event.4[1] = 1 AND event.1 >= {start} AND event.1 < {end},
                    [tuple(toUInt8(1), [toUInt64(i)], event.1, event.2)], [])
            ),
            if(throwIf(action_count > 4096 OR start_count > 128 OR candidate_work > 20000000,
                'Funnel attempt safety limit reached; use a shorter date range') = 0, events, []),
            arrayEnumerate(events),
            arraySlice([tuple(toUInt8(1), [toUInt64(0)], toDateTime64(0, 3, 'UTC'), '')], 1, 0)
        ) AS candidates
    FROM bounded
),
selected_attempts AS (
    SELECT events, arrayElement(arraySort(candidate -> (
        -toInt16(candidate.1), candidate.3, candidate.4
    ), candidates), 1) AS selected
    FROM attempts
)
SELECT
    {metrics}
FROM selected_attempts
SETTINGS output_format_json_quote_64bit_integers = 0"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboards::{decode_funnel_counts, funnel_audience_reportable, validation};
    use serde_json::{json, Value};

    fn definition(order: &str) -> FunnelDefinition {
        serde_json::from_value(json!({
            "semantic_version":2, "conversion_window":"1h", "entry_mode":"closed", "order_mode":order,
            "steps":[
                {"id":"a","name":"A","condition":{"kind":"event","id":"A","name":"A"}},
                {"id":"b","name":"B","condition":{"kind":"event","id":"B","name":"B"}},
                {"id":"c","name":"C","condition":{"kind":"event","id":"C","name":"C"}}
            ]
        })).unwrap()
    }

    fn period() -> FunnelPeriodRange {
        FunnelPeriodRange {
            start: "2026-10-01T00:00:00Z".parse().unwrap(),
            end: "2026-10-02T00:00:00Z".parse().unwrap(),
            label: "synthetic".into(),
        }
    }

    fn now() -> DateTime<Utc> {
        "2026-10-04T00:00:00Z".parse().unwrap()
    }

    #[test]
    fn legacy_definitions_remain_explicit_and_future_versions_are_rejected() {
        let mut value = serde_json::to_value(definition("exact")).unwrap();
        value.as_object_mut().unwrap().remove("semantic_version");
        let legacy: FunnelDefinition = serde_json::from_value(value).unwrap();
        assert_eq!(legacy.semantic_version, 1);
        assert_eq!(
            serde_json::to_value(&legacy).unwrap()["semantic_version"],
            1
        );
        assert!(
            super::super::funnel_query("test", &legacy, &period(), now())
                .contains("events.sequence_index = previous.step_1_index + 1")
        );
        let mut future = definition("ordered");
        future.semantic_version = 3;
        assert!(validation::validate_funnel(&future).is_err());
    }

    #[test]
    fn v2_query_bounds_attempt_work_deduplicates_and_excludes_passive_packets() {
        let sql =
            super::super::funnel_query("site'escaped", &definition("exact"), &period(), now());
        for fragment in [
            "site_id = 'site\\'escaped'",
            "GROUP BY cohort_id, event_id",
            "event_type IN ('pageview', 'external', 'rule')",
            "groupArray(4097)",
            "action_count > 4096",
            "start_count > 128",
            "candidate_work > 20000000",
            "-toInt16(candidate.1)",
        ] {
            assert!(sql.contains(fragment), "missing {fragment}");
        }
        assert!(sql.contains(ACTIVE_ROW_PREDICATE));
        assert!(!sql.contains("row_number()"));
    }

    #[test]
    fn switching_funnel_versions_cannot_disable_distinct_visitor_privacy() {
        for version in [1, 2] {
            let mut funnel = definition("ordered");
            funnel.semantic_version = version;
            for minimum_visitors in [4, 5] {
                let row = json!({"step_1_count":100,"step_2_count":80,"step_3_count":40,
                    "step_1_visitors":100,"step_2_visitors":20,"step_3_visitors":minimum_visitors,
                    "step_1_avg_ms":0,"step_1_median_ms":0,"step_2_avg_ms":0,"step_2_median_ms":0});
                let counts = decode_funnel_counts(Some(&row), 3).unwrap();
                let period =
                    crate::dashboards::protected_funnel_period(&funnel, "test".into(), counts);
                assert_eq!(
                    period.steps.unwrap().len(),
                    if minimum_visitors < 5 { 0 } else { 3 }
                );
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires isolated loopback OWLEYE_TEST_CLICKHOUSE_URL; synthetic rows only"]
    async fn real_clickhouse_v2_native_uuid_page_and_event_steps() {
        let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("isolated ClickHouse URL");
        assert!(matches!(
            reqwest::Url::parse(&url).unwrap().host_str(),
            Some("localhost" | "127.0.0.1")
        ));
        let ch = crate::storage::clickhouse::ClickHouse::new(url).unwrap();
        // Match the production UUID and LowCardinality types. The old String
        // fixtures hid an arrayFold accumulator type mismatch for every funnel.
        // Duplicate delivery and a passive packet must not alter progression.
        let source = r#"(SELECT 'test' AS site_id,
            concat('session-', toString(number)) AS anon_session_id,
            concat('visitor-', toString(number)) AS visitor_id,
            toUUID(concat('00000000-0000-0000-', leftPad(toString(number), 4, '0'), '-', leftPad(toString(id), 12, '0'))) AS event_id,
            toDateTime64('2026-10-01 00:00:00', 3, 'UTC') + toIntervalMillisecond(offset_ms) AS occurred_at,
            now64(3) + INTERVAL 10 DAY AS retention_active_until,
            toLowCardinality(kind) AS event_type, toLowCardinality(name) AS event_name,
            path AS url_path, '' AS rule_id, toLowCardinality('') AS device_type
            FROM numbers(5) AS visitors CROSS JOIN values('id UInt8, offset_ms Int64, name String, kind String, path String',
                (1,0,'page_view','pageview','/'),
                (2,5,'page_load','external','/'),
                (2,5,'page_load','external','/'),
                (3,1000,'page_session','page_session','/'),
                (4,2000,'page_view','pageview','/blog/')) AS actions)"#;
        let mut funnel: FunnelDefinition = serde_json::from_value(json!({
            "semantic_version":2, "conversion_window":"7d", "entry_mode":"closed", "order_mode":"ordered",
            "steps":[
                {"id":"landing","name":"Landing","condition":{"kind":"page","id":"/","name":"Landing"}},
                {"id":"next","name":"Next","condition":{"kind":"page","id":"/blog/","name":"Next"}}
            ]
        })).unwrap();
        for (order, kind, id, expected, elapsed) in [
            ("ordered", "page", "/blog/", [5, 5], Some(2_000)),
            ("exact", "page", "/blog/", [5, 0], None),
            ("ordered", "event", "page_load", [5, 5], Some(5)),
            ("exact", "event", "page_load", [5, 5], Some(5)),
        ] {
            funnel.order_mode = order.into();
            funnel.steps[1].condition.kind = kind.into();
            funnel.steps[1].condition.id = id.into();
            let sql = super::super::funnel_query("test", &funnel, &period(), now())
                .replace("FROM owleye_events", &format!("FROM {source}"));
            let rows = ch.query_report::<Value>(&sql).await.unwrap();
            let metrics = decode_funnel_counts(rows.first(), 2).unwrap();
            assert_eq!(metrics.counts, expected, "{order} {kind} {id}");
            assert_eq!(metrics.audience_counts, Some(expected.to_vec()));
            assert_eq!(metrics.average_time_to_next_ms, [elapsed]);
            assert_eq!(metrics.median_time_to_next_ms, [elapsed]);
            assert_eq!(funnel_audience_reportable(&metrics), expected[1] == 5);
        }
        // The same native UUID query cannot release a one-visitor audience.
        let sparse_source = source.replace("FROM numbers(5)", "FROM numbers(1)");
        let sql = super::super::funnel_query("test", &funnel, &period(), now())
            .replace("FROM owleye_events", &format!("FROM {sparse_source}"));
        let rows = ch.query_report::<Value>(&sql).await.unwrap();
        let metrics = decode_funnel_counts(rows.first(), 2).unwrap();
        assert_eq!(metrics.counts, [1, 1]);
        assert_eq!(metrics.audience_counts, Some(vec![1, 1]));
        assert!(!funnel_audience_reportable(&metrics));
        ch.shutdown().await;
    }

    type SyntheticRow<'a> = (&'a str, &'a str, &'a str, i64, &'a str, &'a str, bool);

    #[tokio::test]
    #[ignore = "requires isolated loopback OWLEYE_TEST_CLICKHOUSE_URL; synthetic rows only"]
    async fn real_clickhouse_v2_excludes_stored_crawlers_and_v1_retains_legacy_population() {
        let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("isolated ClickHouse URL");
        assert!(matches!(
            reqwest::Url::parse(&url).unwrap().host_str(),
            Some("localhost" | "127.0.0.1")
        ));
        let ch = crate::storage::clickhouse::ClickHouse::new(url).unwrap();
        let rows = [
            ("bot", "known-crawler", "a", 0, "A", "external", true),
            ("bot", "known-crawler", "b", 1_000, "B", "external", true),
            ("bot", "known-crawler", "c", 2_000, "C", "external", true),
            ("unknown", "unknown", "a", 0, "A", "external", true),
            ("unknown", "unknown", "b", 1_000, "B", "external", true),
            ("unknown", "unknown", "c", 2_000, "C", "external", true),
        ];
        let mut funnel = definition("ordered");
        let modern = run(&ch, &funnel, &rows).await;
        assert_eq!(
            decode_funnel_counts(Some(&modern), 3).unwrap().counts,
            [1, 1, 1]
        );
        funnel.semantic_version = 1;
        let legacy = run(&ch, &funnel, &rows).await;
        assert_eq!(
            decode_funnel_counts(Some(&legacy), 3).unwrap().counts,
            [2, 2, 2]
        );
    }

    async fn run(
        ch: &crate::storage::clickhouse::ClickHouse,
        definition: &FunnelDefinition,
        rows: &[SyntheticRow<'_>],
    ) -> Value {
        let values = rows
            .iter()
            .map(|(cohort, visitor, id, at, name, kind, retained)| {
                format!(
                    "({}, {}, {}, {at}, {}, {}, {})",
                    clickhouse_string(cohort),
                    clickhouse_string(visitor),
                    clickhouse_string(id),
                    clickhouse_string(name),
                    clickhouse_string(kind),
                    u8::from(*retained)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let source = format!(
            r#"(SELECT 'test' AS site_id, cohort AS anon_session_id, visitor AS visitor_id, id AS event_id,
            toDateTime64('2026-10-01 00:00:00', 3, 'UTC') + toIntervalMillisecond(offset_ms) AS occurred_at,
            if(retained = 1, now64(3) + INTERVAL 10 DAY, now64(3) - INTERVAL 1 DAY) AS retention_active_until,
            kind AS event_type, name AS event_name, '' AS rule_id, '/' AS url_path,
            '' AS utm_source, '' AS utm_medium, '' AS utm_campaign, '' AS country, '' AS region,
            '' AS browser_name, '' AS os_name, if(visitor='known-crawler','crawler','') AS device_type, '{{}}' AS payload_json
            FROM values('cohort String, visitor String, id String, offset_ms Int64, name String, kind String, retained UInt8', {values}))"#
        );
        let sql = super::super::funnel_query("test", definition, &period(), now())
            .replace("FROM owleye_events", &format!("FROM {source}"));
        ch.query_report::<Value>(&sql).await.unwrap().remove(0)
    }

    #[tokio::test]
    #[ignore = "requires isolated loopback OWLEYE_TEST_CLICKHOUSE_URL; synthetic rows only"]
    async fn real_clickhouse_v2_later_attempts_ties_duplicates_and_boundaries() {
        let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("isolated ClickHouse URL");
        assert!(matches!(
            reqwest::Url::parse(&url).unwrap().host_str(),
            Some("localhost" | "127.0.0.1")
        ));
        let ch = crate::storage::clickhouse::ClickHouse::new(url).unwrap();
        let rows = [
            ("one", "v", "01", 0, "A", "external", true),
            ("one", "v", "02", 1_000, "X", "external", true),
            ("one", "v", "03", 2_000, "A", "external", true),
            ("one", "v", "04", 3_000, "B", "external", true),
            ("one", "v", "05", 4_000, "C", "external", true),
        ];
        let mut funnel = definition("exact");
        let row = run(&ch, &funnel, &rows).await;
        let metrics = decode_funnel_counts(Some(&row), 3).unwrap();
        assert_eq!(metrics.counts, [1, 1, 1]);
        assert_eq!(metrics.average_time_to_next_ms, [Some(1_000), Some(1_000)]);
        funnel.semantic_version = 1;
        let legacy = run(&ch, &funnel, &rows).await;
        assert_eq!(
            decode_funnel_counts(Some(&legacy), 3).unwrap().counts,
            [1, 0, 0]
        );

        let expired_then_valid = [
            ("one", "v", "01", 0, "A", "external", true),
            ("one", "v", "02", 7_201_000, "B", "external", true),
            ("one", "v", "03", 7_202_000, "A", "external", true),
            ("one", "v", "04", 7_203_000, "B", "external", true),
            ("one", "v", "05", 7_204_000, "C", "external", true),
        ];
        for order in ["exact", "ordered"] {
            let row = run(&ch, &definition(order), &expired_then_valid).await;
            assert_eq!(
                decode_funnel_counts(Some(&row), 3).unwrap().counts,
                [1, 1, 1],
                "{order}"
            );
        }
        let stable_ties_and_passive = [
            ("one", "v", "a", 1_000, "A", "external", true),
            ("one", "v", "b", 1_000, "B", "external", true),
            ("one", "v", "b", 1_000, "B", "external", true),
            ("one", "v", "before-c", 1_000, "packet", "duration", true),
            (
                "one",
                "v",
                "before-c-perf",
                1_000,
                "packet",
                "performance",
                true,
            ),
            ("one", "v", "c", 1_000, "C", "external", true),
        ];
        let row = run(&ch, &definition("exact"), &stable_ties_and_passive).await;
        let metrics = decode_funnel_counts(Some(&row), 3).unwrap();
        assert_eq!(metrics.counts, [1, 1, 1]);
        assert_eq!(metrics.average_time_to_next_ms, [Some(0), Some(0)]);

        let edges = [
            ("expired", "v1", "a", 0, "A", "external", false),
            ("expired", "v1", "b", 1_000, "B", "external", true),
            ("before", "v2", "a", -1, "A", "external", true),
            ("before", "v2", "b", 1_000, "B", "external", true),
            ("at-end", "v3", "a", 86_400_000, "A", "external", true),
            ("at-end", "v3", "b", 86_401_000, "B", "external", true),
            ("window", "v4", "a", 0, "A", "external", true),
            ("window", "v4", "b", 3_600_000, "B", "external", true),
            ("window", "v4", "c", 3_600_001, "C", "external", true),
        ];
        let row = run(&ch, &definition("ordered"), &edges).await;
        assert_eq!(
            decode_funnel_counts(Some(&row), 3).unwrap().counts,
            [1, 1, 0]
        );
        let row = run(
            &ch,
            &definition("exact"),
            &[("none", "v", "x", 0, "X", "external", true)],
        )
        .await;
        let metrics = decode_funnel_counts(Some(&row), 3).unwrap();
        assert_eq!(metrics.counts, [0, 0, 0]);
        assert_eq!(metrics.average_time_to_next_ms, [None, None]);

        // Repeated intermediates are intervening actions in exact mode, and
        // cannot be silently skipped to invent an exact completion.
        let repeats = [
            ("one", "v", "a", 0, "A", "external", true),
            ("one", "v", "b1", 1_000, "B", "external", true),
            ("one", "v", "b2", 2_000, "B", "external", true),
            ("one", "v", "c", 3_000, "C", "external", true),
        ];
        for (order, expected) in [("exact", [1, 1, 0]), ("ordered", [1, 1, 1])] {
            let row = run(&ch, &definition(order), &repeats).await;
            assert_eq!(
                decode_funnel_counts(Some(&row), 3).unwrap().counts,
                expected
            );
        }
        // Completion wins over an earlier partial attempt, and the displayed
        // times must come from that same selected attempt.
        let two_attempts = [
            ("one", "v", "a1", 0, "A", "external", true),
            ("one", "v", "b1", 100, "B", "external", true),
            ("one", "v", "x", 200, "X", "external", true),
            ("one", "v", "a2", 300, "A", "external", true),
            ("one", "v", "b2", 350, "B", "external", true),
            ("one", "v", "c2", 400, "C", "external", true),
        ];
        let row = run(&ch, &definition("exact"), &two_attempts).await;
        let metrics = decode_funnel_counts(Some(&row), 3).unwrap();
        assert_eq!(metrics.counts, [1, 1, 1]);
        assert_eq!(metrics.average_time_to_next_ms, [Some(50), Some(50)]);
        let row = run(&ch, &definition("exact"), &two_attempts[..5]).await;
        let metrics = decode_funnel_counts(Some(&row), 3).unwrap();
        assert_eq!(metrics.counts, [1, 1, 0]);
        assert_eq!(metrics.average_time_to_next_ms, [Some(100), None]);

        // One visitor generating many sessions is still a one-person audience.
        // The threshold must use the selected step's distinct visitor count.
        let sessions: Vec<_> = (0..100).map(|n| format!("session-{n}")).collect();
        let repeated_sessions: Vec<_> = sessions
            .iter()
            .flat_map(|session| {
                [
                    (
                        session.as_str(),
                        "one-visitor",
                        "a",
                        0,
                        "A",
                        "external",
                        true,
                    ),
                    (
                        session.as_str(),
                        "one-visitor",
                        "b",
                        1_000,
                        "B",
                        "external",
                        true,
                    ),
                    (
                        session.as_str(),
                        "one-visitor",
                        "c",
                        2_000,
                        "C",
                        "external",
                        true,
                    ),
                ]
            })
            .collect();
        let mut same_session = definition("ordered");
        same_session.conversion_window = "same_session".into();
        let row = run(&ch, &same_session, &repeated_sessions).await;
        let metrics = decode_funnel_counts(Some(&row), 3).unwrap();
        assert_eq!(metrics.counts, [100, 100, 100]);
        assert_eq!(metrics.audience_counts, Some(vec![1, 1, 1]));
        assert!(!funnel_audience_reportable(&metrics));

        // Resource limits reject an incomplete cohort rather than truncating
        // its sequence and presenting the surviving prefix as a real result.
        for (size, entries_only, succeeds) in [
            (128, true, true),
            (129, true, false),
            (4096, false, true),
            (4097, false, false),
        ] {
            let name = if entries_only {
                "'A'"
            } else {
                "if(number = 0, 'A', 'X')"
            };
            let source = format!(
                r#"(SELECT 'test' AS site_id, 'one' AS anon_session_id,
                'one' AS visitor_id, toString(number) AS event_id,
                toDateTime64('2026-10-01 00:00:00', 3, 'UTC') + toIntervalMillisecond(number) AS occurred_at,
                now64(3) + INTERVAL 10 DAY AS retention_active_until,
                'external' AS event_type, '' AS device_type, {name} AS event_name FROM numbers({size}))"#
            );
            let sql = super::super::funnel_query("test", &definition("exact"), &period(), now())
                .replace("FROM owleye_events", &format!("FROM {source}"));
            let result = ch.query_report::<Value>(&sql).await;
            assert_eq!(
                result.is_ok(),
                succeeds,
                "limit boundary {size}: {result:?}"
            );
        }
        ch.shutdown().await;
    }
}
