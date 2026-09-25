//! Defense in depth for free text. Arbitrary prose can still identify a person;
//! the UI explicitly asks users not to submit personal information.
use regex::Regex;
use std::sync::LazyLock;

pub(super) fn redact(input: &str) -> String {
    static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [
        r"(?i)\b(?:https?://|www\.)[^\s<>]+",
        r"(?i)\b[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b",
        r"(?i)\b(?:authorization|password|passwd|secret|token|api[_-]?key)\s*[:=]\s*(?:bearer\s+)?[^\s,;]+",
        r"(?i)\bbearer\s+[^\s,;]+",
        r"\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b",
        r"(?i)\b[0-9a-f]{0,4}:[0-9a-f:]{2,}(?:%\w+)?",
        r"\b[0-9a-fA-F]{8}-[0-9a-fA-F-]{27,}\b",
        r"\b[A-Za-z0-9_/-]{32,}\b",
        r"(?:\+\d[\d ().-]{7,}\d|\b\d[\d ().-]{10,}\d\b)",
    ].into_iter().map(|p| Regex::new(p).expect("valid redaction pattern")).collect()
    });
    PATTERNS.iter().fold(input.to_owned(), |text, pattern| {
        pattern.replace_all(&text, "[redacted]").into_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_known_sensitive_patterns_and_preserves_questions() {
        let input = "Traffic yesterday for me@example.com https://x.test/p?token=abc 192.168.1.1 2001:db8::1 API_KEY=private bearer abcdef +1 (555) 123-4567";
        let output = redact(input);
        for secret in [
            "me@", "x.test", "192.168", "2001:", "private", "abcdef", "555",
        ] {
            assert!(!output.contains(secret), "{output}");
        }
        assert!(output.starts_with("Traffic yesterday"));
        assert_eq!(
            redact("Show traffic over 30 days"),
            "Show traffic over 30 days"
        );
    }
}
