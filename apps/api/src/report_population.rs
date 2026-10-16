//! Report populations over the classification already stored at ingestion.
//! This policy excludes self-reported/recognized crawler UAs; it does not
//! verify humans, identify unknown automation, or reclassify historical rows.

pub(crate) const LEGACY_ALL_AGENTS_V1: &str = "legacy_all_agents_v1";
pub(crate) const KNOWN_CRAWLERS_EXCLUDED_V1: &str = "known_crawlers_excluded_v1";
pub(crate) const EXCLUDE_KNOWN_CRAWLERS_SQL: &str = "device_type != 'crawler'";
pub(crate) const UNKNOWN_AGENTS_CONTEXT: &str = "included_not_verified_human";
pub(crate) const CLASSIFICATION_CONTEXT: &str = "stored_ingestion_ua_classification_not_recomputed";

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(crate) struct PopulationContext {
    pub policy: &'static str,
    pub unknown_agents: &'static str,
    pub classification_context: &'static str,
}

pub(crate) fn context(definition_version: u16) -> PopulationContext {
    PopulationContext {
        policy: policy_for_version(definition_version),
        unknown_agents: UNKNOWN_AGENTS_CONTEXT,
        classification_context: CLASSIFICATION_CONTEXT,
    }
}

/// Saved v1 definitions retain their original population. New v2 definitions
/// use the shared known-crawler policy; callers must expose that version.
pub(crate) fn predicate(definition_version: u16) -> &'static str {
    if definition_version >= 2 {
        EXCLUDE_KNOWN_CRAWLERS_SQL
    } else {
        "1"
    }
}

pub(crate) fn policy_for_version(definition_version: u16) -> &'static str {
    if definition_version >= 2 {
        KNOWN_CRAWLERS_EXCLUDED_V1
    } else {
        LEGACY_ALL_AGENTS_V1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_populations_are_explicit_and_unknown_agents_are_not_claimed_human() {
        assert_eq!(predicate(1), "1");
        assert_eq!(policy_for_version(1), LEGACY_ALL_AGENTS_V1);
        assert_eq!(predicate(2), EXCLUDE_KNOWN_CRAWLERS_SQL);
        assert_eq!(policy_for_version(2), KNOWN_CRAWLERS_EXCLUDED_V1);
        assert_eq!(UNKNOWN_AGENTS_CONTEXT, "included_not_verified_human");
    }

    #[tokio::test]
    #[ignore = "requires OWLEYE_TEST_CLICKHOUSE_URL; synthetic SELECT only"]
    async fn stored_crawler_policy_preserves_unknown_and_custom_only_populations() {
        let Ok(url) = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL") else {
            return;
        };
        let parsed = reqwest::Url::parse(&url).unwrap();
        assert!(matches!(
            parsed.host_str(),
            Some("127.0.0.1" | "localhost" | "::1")
        ));
        let ch = crate::storage::clickhouse::ClickHouse::new(url).unwrap();
        let fixture = "(SELECT tupleElement(row,1) AS device_type, tupleElement(row,2) AS event_type, tupleElement(row,3) AS visitor_id FROM (SELECT arrayJoin([('crawler','pageview','crawler'),('','pageview','unknown'),('desktop','pageview','browser'),('crawler','external','crawler-custom'),('','external','unknown-custom'),('desktop','page_session','passive')]) AS row))";
        let query = format!("SELECT count() AS legacy_rows, countIf({}) AS policy_rows, uniqExactIf(visitor_id,event_type='pageview' AND {}) AS pageview_visitors, countIf(event_type='external' AND {}) AS custom_events, countIf(event_type='page_session' AND {}) AS passive_rows FROM {fixture}",predicate(2),predicate(2),predicate(2),predicate(2));
        let rows: Vec<serde_json::Value> = ch.query_report(&query).await.unwrap();
        assert_eq!(rows[0]["legacy_rows"], 6);
        assert_eq!(rows[0]["policy_rows"], 4);
        assert_eq!(rows[0]["pageview_visitors"], 2);
        assert_eq!(rows[0]["custom_events"], 1);
        assert_eq!(rows[0]["passive_rows"], 1);
    }
}
