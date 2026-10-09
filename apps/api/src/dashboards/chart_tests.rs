use super::*;

fn chart() -> WidgetRequest {
    serde_json::from_value(json!({
        "kind":"chart", "title":"Exports", "visualization":"bar",
        "source":{"id":"export_completed","name":"Exports","kind":"event"},
        "breakdown":"property", "breakdown_property":"format", "metric":"visitors",
        "property_filters":{"logic":"and","filters":[{"id":"p","key":"plan","operator":"equals","value":"pro"}]}
    })).unwrap()
}

#[test]
fn chart_controls_validate_and_round_trip_without_changing_legacy_widgets() {
    let request = validate_widget(chart()).unwrap();
    let stored = stored_widget_config(&request);
    let restored: StoredWidgetConfig =
        serde_json::from_value(serde_json::to_value(stored).unwrap()).unwrap();
    assert_eq!(restored.metric, "visitors");
    assert_eq!(restored.breakdown_property.as_deref(), Some("format"));
    assert_eq!(
        restored.property_filters.unwrap().filters[0]
            .value
            .as_deref(),
        Some("pro")
    );
    let mut legacy = serde_json::to_value(&request).unwrap();
    legacy.as_object_mut().unwrap().remove("metric");
    assert_eq!(
        serde_json::from_value::<WidgetRequest>(legacy)
            .unwrap()
            .metric,
        "events"
    );
    for (field, value) in [
        ("metric", json!("raw_sql")),
        ("breakdown_property", json!("x' OR 1=1")),
        ("breakdown_property", Value::Null),
        ("property_filters", json!({"logic":"and","filters":[]})),
        (
            "property_filters",
            json!({"logic":"xor","filters":[{"id":"a","key":"format","operator":"equals","value":"pdf"}]}),
        ),
    ] {
        let mut invalid = serde_json::to_value(chart()).unwrap();
        invalid[field] = value;
        assert!(
            validate_widget(serde_json::from_value(invalid).unwrap()).is_err(),
            "{field}"
        );
    }
}

#[test]
fn chart_queries_use_metric_distincts_and_escape_property_values() {
    let mut request = chart();
    request.property_filters.as_mut().unwrap().filters[0].value = Some("pro' OR 1=1".into());
    let range = FunnelPeriodRange {
        start: Utc::now() - Duration::days(1),
        end: Utc::now(),
        label: "current".into(),
    };
    let sql = query::category_query("site'1", &request, &range, None);
    assert!(sql.contains("site\\'1"));
    assert!(sql.contains("pro\\' OR 1=1"));
    assert!(sql.contains(crate::retention::ACTIVE_ROW_PREDICATE));
    assert!(sql.contains("uniqCombined64If(visitor_id"));
    assert!(sql.contains("category NOT IN"));
    assert!(!sql.contains("sum(current_value)"));
    request.metric = "sessions".into();
    let sql = query::category_query("test", &request, &range, None);
    assert!(sql.contains("uniqCombined64If(anon_session_id"));
}

#[tokio::test]
#[ignore = "requires loopback OWLEYE_TEST_CLICKHOUSE_URL; synthetic SELECT queries only"]
async fn real_clickhouse_property_metrics_deduplicate_other_and_compare() {
    let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").unwrap();
    assert!(matches!(
        reqwest::Url::parse(&url).unwrap().host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]")
    ));
    let ch = crate::storage::clickhouse::ClickHouse::new(url).unwrap();
    let now = Utc::now();
    let current = FunnelPeriodRange {
        start: now - Duration::days(1),
        end: now,
        label: "current".into(),
    };
    let previous = FunnelPeriodRange {
        start: now - Duration::days(2),
        end: current.start,
        label: "previous".into(),
    };
    // One visitor/session appears in all 25 categories, twice in current and once
    // in previous. Other must be one visitor, not five; occurrence counts are 10/5.
    let fixture = r#"(SELECT 'test' AS site_id, 'external' AS event_type, 'export_completed' AS event_name, '' AS rule_id, '/' AS url_path, 'IN' AS country, 'v1' AS visitor_id, 's1' AS anon_session_id, now64(3) - toIntervalHour(if(number < 50, 1, 25)) AS occurred_at, now64(3)+toIntervalDay(1) AS retention_active_until, concat('{"records":{"plan":"pro","format":"f',leftPad(toString(number % 25),2,'0'),'"}}') AS payload_json FROM numbers(75))"#;
    for (metric, current_other, previous_other) in
        [("events", 10, 5), ("visitors", 1, 1), ("sessions", 1, 1)]
    {
        let mut request = chart();
        request.metric = metric.into();
        let sql = query::category_query("test", &request, &current, Some(&previous))
            .replace("FROM owleye_events", &format!("FROM {fixture}"));
        let rows = ch.query_json_each_row::<Value>(&sql).await.unwrap();
        assert_eq!(rows.len(), 21);
        let other = rows.iter().find(|r| r["group_label"] == "Other").unwrap();
        assert_eq!(other["current_value"], current_other);
        assert_eq!(other["previous_value"], previous_other);
        let sql = query::category_query("different-site", &request, &current, Some(&previous))
            .replace("FROM owleye_events", &format!("FROM {fixture}"));
        assert!(ch
            .query_json_each_row::<Value>(&sql)
            .await
            .unwrap()
            .is_empty());
        request.property_filters.as_mut().unwrap().filters[0].value = Some("free".into());
        let sql = query::category_query("test", &request, &current, None)
            .replace("FROM owleye_events", &format!("FROM {fixture}"));
        assert!(ch
            .query_json_each_row::<Value>(&sql)
            .await
            .unwrap()
            .is_empty());
    }
}
