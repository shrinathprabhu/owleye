//! Bounded marketing labels, never recipient identifiers or secrets.
use regex::Regex;
use std::sync::LazyLock;

pub(crate) const UTM_KEYS: [&str; 5] = [
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
];

pub(crate) fn campaign_value(raw: &str) -> Option<String> {
    let value = raw.trim();
    static UNSAFE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [
        r"[\x00-\x1f\x7f\x{200b}-\x{200f}\x{202a}-\x{202e}\x{2066}-\x{2069}\x{feff}@<>%]",
        r"(?i)[a-z][a-z\d+.-]*://|www\.|(?:\d{1,3}\.){3}\d{1,3}|(?:[a-f\d]{0,4}:){2,}[a-f\d]{0,4}",
        r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|\d{7}|[a-z\d]{32}|[a-z\d_-]{8,}\.[a-z\d_-]{8,}\.[a-z\d_-]{8,}",
        r"(?i)(?:^|[\s_-])(?:user|customer|recipient|contact|subscriber|member|account)[_-]?id(?:[\s_:=.-]|\d|$)|(?:^|[\s_-])(?:user|customer|recipient|contact|subscriber|member|account)[_-]?\d{3,}|(?:^|[\s_-])(?:token|secret|password|session|auth|api[_-]?key)[\s_:=.-]|(?:^|[\s_-])(?:sk|pk)_(?:live|test)_[a-z\d]+|\bbearer\s+[a-z\d._-]+",
    ].into_iter().map(|p|Regex::new(p).expect("attribution privacy pattern")).collect()
    });
    if value.is_empty() || UNSAFE.iter().any(|p| p.is_match(value)) {
        return None;
    }
    let mut end = value.len().min(200);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    Some(value[..end].to_owned())
}

/// Remove unsafe attribution parameters even from an explicit full-query
/// capture. Other query parameters retain their existing opt-in semantics.
pub(crate) fn sanitize_search(raw: &str) -> Option<String> {
    let mut first = std::collections::HashMap::new();
    let mut retained = Vec::new();
    for pair in raw.trim_start_matches('?').split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let normalized = crate::models::decode_query_component(key)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if UTM_KEYS.contains(&normalized.as_str()) {
            first.entry(normalized).or_insert_with(|| {
                crate::models::decode_query_component(value).unwrap_or_default()
            });
        } else if !pair.is_empty() {
            retained.push(pair.to_owned());
        }
    }
    let mut normalized = reqwest::Url::parse("https://attribution.invalid/").expect("fixed URL");
    {
        let mut query = normalized.query_pairs_mut();
        for key in UTM_KEYS {
            if let Some(value) = first.get(key).and_then(|v| campaign_value(v)) {
                query.append_pair(key, &value);
            }
        }
    }
    if let Some(query) = normalized.query().filter(|q| !q.is_empty()) {
        for pair in query.split('&') {
            let bytes = 1
                + retained.iter().map(String::len).sum::<usize>()
                + retained.len().saturating_sub(1)
                + usize::from(!retained.is_empty())
                + pair.len();
            if bytes <= 2_048 {
                retained.push(pair.to_owned());
            }
        }
    }
    (!retained.is_empty()).then(|| format!("?{}", retained.join("&")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn campaign_values_are_bounded_labels_not_identifiers_or_secrets() {
        for value in [
            "hello@example.test",
            "user%40example.test",
            "https://example.test/post",
            "192.168.1.1",
            "2001:db8::1",
            "1234567",
            "user_id-42",
            "customer1234",
            "token=secret",
            "aaabbbcccdddeeefffggghhhiiijjjkkkll",
            "a\u{202e}b",
        ] {
            assert!(campaign_value(value).is_none(), "{value}");
        }
        assert_eq!(
            campaign_value(" privacy analytics ").as_deref(),
            Some("privacy analytics")
        );
        assert_eq!(campaign_value(&"é".repeat(150)).unwrap().len(), 200);
    }
    #[test]
    fn first_utm_occurrence_is_checked_before_truncation_and_unknown_opt_in_query_is_preserved() {
        assert_eq!(sanitize_search("?UTM_TERM=user%2540example.test&utm_term=safe&utm_content=hero&plan=pro&ref=producthunt"),Some("?plan=pro&ref=producthunt&utm_content=hero".into()));
        assert!(sanitize_search(&format!(
            "?utm_content={}person%40example.test",
            "é".repeat(150)
        ))
        .is_none());
        let raw = UTM_KEYS
            .into_iter()
            .map(|key| format!("{key}={}", "é".repeat(100)))
            .collect::<Vec<_>>()
            .join("&");
        let bounded = sanitize_search(&raw).unwrap();
        assert!(bounded.len() <= 2_048);
        assert!(bounded.split('&').all(|pair| !pair.ends_with('%')));
        for value in ["sk_live_abc123", "pk_test_abc123", "Bearer abc123"] {
            assert!(campaign_value(value).is_none());
        }
    }
}
