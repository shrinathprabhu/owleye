use std::{borrow::Cow, sync::LazyLock};

use woothee::{
    parser::{Parser, WootheeResult},
    woothee::VALUE_UNKNOWN,
};

// Parse the versioned upstream rules once, never once per event.
static PARSER: LazyLock<ua_parser::Extractor<'static>> = LazyLock::new(|| {
    let rules: ua_parser::Regexes<'static> =
        serde_json::from_str(include_str!("../../vendor/uap-core/regexes.json"))
            .expect("valid bundled UA parser rules");
    ua_parser::Extractor::try_from(rules).expect("supported bundled UA parser rules")
});

// Shared by ingestion and SQL reporting so old labels merge before distinct counting.
const BROWSER_ALIASES: &[(&str, &[&str])] = &[
    (
        "Chrome",
        &[
            "chrome",
            "chromium",
            "google chrome",
            "chrome mobile",
            "chrome mobile ios",
            "chrome mobile webview",
            "crios",
        ],
    ),
    (
        "Edge",
        &[
            "edge",
            "edg",
            "microsoft edge",
            "edge mobile",
            "edge mobile ios",
            "edga",
            "edgios",
        ],
    ),
    (
        "Firefox",
        &["firefox", "firefox mobile", "firefox ios", "fxios"],
    ),
    (
        "Safari",
        &["safari", "mobile safari", "mobile safari ui/wkwebview"],
    ),
    (
        "Opera",
        &["opera", "opera mobile", "opera mini", "opr", "opios"],
    ),
    (
        "Samsung Internet",
        &[
            "samsung internet",
            "samsung internet for android",
            "samsungbrowser",
            "samsung browser",
        ],
    ),
    ("Arc", &["arc", "arc browser", "arc search", "arcmobile2"]),
    ("Comet", &["comet", "comet browser", "perplexity comet"]),
    ("Dia", &["dia", "dia browser"]),
    ("Zen", &["zen", "zen browser"]),
    ("Floorp", &["floorp", "floorp browser"]),
    (
        "Headless Chrome",
        &["headlesschrome", "headless chrome", "chrome headless"],
    ),
    ("Headless Edge", &["headlessedge", "headless edge"]),
    (
        "UC Browser",
        &["ucbrowser", "uc browser", "uc browser mobile", "ucweb"],
    ),
    ("Brave", &["brave", "brave browser"]),
    ("Vivaldi", &["vivaldi"]),
];

pub fn canonical_browser(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") || value == "Other" {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    Some(
        BROWSER_ALIASES
            .iter()
            .find(|(_, aliases)| aliases.contains(&lower.as_str()))
            .map(|(name, _)| *name)
            .unwrap_or(value)
            .to_owned(),
    )
}

pub(crate) fn browser_sql(column: &str) -> String {
    let cases = BROWSER_ALIASES
        .iter()
        .map(|(name, aliases)| {
            let aliases = aliases
                .iter()
                .map(|a| crate::models::clickhouse_string(a))
                .collect::<Vec<_>>()
                .join(",");
            format!("lowerUTF8(trimBoth({column})) IN ({aliases}), '{}'", name)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("multiIf({cases}, trimBoth({column}) = '', 'Unknown', {column})")
}

// Generic engine brands must not erase explicit fork or headless identities.
pub(crate) fn resolve_browser(
    parsed: &ParsedUserAgent,
    hint: Option<&str>,
    hint_version: Option<&str>,
) -> (String, String) {
    let from_hint = hint.and_then(canonical_browser);
    let from_ua = parsed.browser_name.as_deref().and_then(canonical_browser);
    let use_hint = from_hint.is_some()
        && !(matches!(from_hint.as_deref(), Some("Chrome" | "Firefox" | "Safari"))
            && from_ua.is_some()
            && from_ua != from_hint);
    if use_hint {
        let same = from_hint == from_ua;
        (
            from_hint.unwrap(),
            hint_version
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .or_else(|| same.then(|| parsed.browser_version.clone()).flatten())
                .unwrap_or_default(),
        )
    } else {
        (
            from_ua.unwrap_or_default(),
            parsed.browser_version.clone().unwrap_or_default(),
        )
    }
}

// An explicit product token can identify forks missing from the pinned upstream
// rules. Never infer a fork from its engine version or probe browser internals.
static PRODUCT_TOKEN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)(?:^|[\s;(])(?P<name>HeadlessChrome|HeadlessEdge|ArcMobile2|Arc|Comet|Dia|Zen|Floorp|Brave|Vivaldi|OPR|OPiOS|Opera|UCBrowser|UCWEB)/(?P<version>[0-9]+(?:\.[0-9]+)*)")
        .expect("valid browser product token pattern")
});

#[derive(Clone, Debug, Default)]
pub struct ParsedUserAgent {
    pub browser_name: Option<String>,
    pub browser_version: Option<String>,
    pub device_type: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
}

pub fn parse_user_agent(user_agent: Option<&str>) -> ParsedUserAgent {
    let Some(ua) = user_agent.filter(|value| !value.trim().is_empty()) else {
        return ParsedUserAgent::default();
    };

    let fallback = Parser::new().parse(ua);
    let (browser, os, device) = PARSER.extract(ua);
    let product = PRODUCT_TOKEN.captures(ua);
    let versionless = ua
        .split([' ', ';', '(', ')'])
        .find_map(|token| match token {
            "ArcMobile2" => Some("Arc"),
            "Brave" => Some("Brave"),
            _ => None,
        });
    let browser_name = versionless
        .map(str::to_owned)
        .or_else(|| product.as_ref().and_then(|p| canonical_browser(&p["name"])))
        .or_else(|| browser.as_ref().and_then(|v| canonical_browser(&v.family)))
        .or_else(|| fallback.as_ref().and_then(|v| canonical_browser(v.name)));
    let browser_version = if versionless.is_some() {
        None
    } else if let Some(product) = &product {
        Some(product["version"].to_owned())
    } else {
        browser
            .as_ref()
            .and_then(|v| version(&[v.major, v.minor, v.patch, v.patch_minor]))
            .or_else(|| fallback.as_ref().and_then(|v| known(v.version)))
    };
    let device_family = device
        .as_ref()
        .map(|d| d.device.as_ref())
        .unwrap_or_default();
    let device_type = if device_family == "Spider"
        || fallback.as_ref().is_some_and(|r| r.category == "crawler")
    {
        Some("crawler".into())
    } else if ua.contains("iPad") || (ua.contains("Android") && !ua.contains("Mobile")) {
        Some("tablet".into())
    } else if ua.contains("Mobi") || ua.contains("iPhone") {
        Some("mobile".into())
    } else {
        fallback.as_ref().and_then(device_type)
    };
    ParsedUserAgent {
        browser_name,
        browser_version,
        device_type,
        os_name: os
            .as_ref()
            .and_then(|v| known(&v.os))
            .or_else(|| fallback.as_ref().and_then(|r| known(r.os))),
        os_version: os
            .as_ref()
            .and_then(|v| {
                version(&[
                    v.major.as_deref(),
                    v.minor.as_deref(),
                    v.patch.as_deref(),
                    v.patch_minor.as_deref(),
                ])
            })
            .or_else(|| fallback.and_then(|r| known_cow(r.os_version))),
    }
}

fn version(parts: &[Option<&str>]) -> Option<String> {
    let value = parts
        .iter()
        .copied()
        .flatten()
        .collect::<Vec<_>>()
        .join(".");
    (!value.is_empty()).then_some(value)
}

fn known(value: &str) -> Option<String> {
    (!value.is_empty() && value != VALUE_UNKNOWN).then(|| value.to_owned())
}

fn known_cow(value: Cow<'_, str>) -> Option<String> {
    (!value.is_empty() && value.as_ref() != VALUE_UNKNOWN).then(|| value.into_owned())
}

fn device_type(result: &WootheeResult<'_>) -> Option<String> {
    match result.category {
        "pc" => Some("desktop".to_owned()),
        "smartphone" | "mobilephone" => Some("mobile".to_owned()),
        "appliance" => Some("appliance".to_owned()),
        "crawler" => Some("crawler".to_owned()),
        "misc" => Some("other".to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chrome_on_macos() {
        let parsed = parse_user_agent(Some(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        ));

        assert_eq!(parsed.browser_name.as_deref(), Some("Chrome"));
        assert_eq!(parsed.device_type.as_deref(), Some("desktop"));
        assert_eq!(parsed.os_name.as_deref(), Some("Mac OS X"));
    }

    #[test]
    fn parses_firefox_on_linux() {
        let parsed = parse_user_agent(Some(
            "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0",
        ));

        assert_eq!(parsed.browser_name.as_deref(), Some("Firefox"));
        assert_eq!(parsed.browser_version.as_deref(), Some("121.0"));
        assert_eq!(parsed.device_type.as_deref(), Some("desktop"));
        assert_eq!(parsed.os_name.as_deref(), Some("Linux"));
    }

    #[test]
    fn parses_mobile_safari() {
        let parsed = parse_user_agent(Some(
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1",
        ));

        assert_eq!(parsed.browser_name.as_deref(), Some("Safari"));
        assert_eq!(parsed.device_type.as_deref(), Some("mobile"));
        assert_eq!(parsed.os_name.as_deref(), Some("iOS"));
    }

    #[test]
    fn classifies_crawlers() {
        let parsed = parse_user_agent(Some(
            "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Googlebot/2.1; \
             +http://www.google.com/bot.html) Chrome/W.X.Y.Z Safari/537.36",
        ));

        assert_eq!(parsed.device_type.as_deref(), Some("crawler"));
    }
}

#[cfg(test)]
mod normalization_tests {
    use super::*;
    #[test]
    fn canonicalizes_hints_and_mobile_families() {
        for alias in [
            "Chrome",
            "Chromium",
            "Google Chrome",
            "Chrome Mobile iOS",
            " chrome ",
        ] {
            assert_eq!(canonical_browser(alias).as_deref(), Some("Chrome"));
        }
        assert_eq!(canonical_browser("Microsoft Edge").as_deref(), Some("Edge"));
        assert_eq!(canonical_browser("Brave").as_deref(), Some("Brave"));
        assert_eq!(canonical_browser("UNKNOWN"), None);
    }
    #[test]
    fn specific_browser_survives_generic_hint() {
        let edge = parse_user_agent(Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.2849.80"));
        assert_eq!(
            resolve_browser(&edge, Some("Chromium"), Some("130")).0,
            "Edge"
        );
        assert_eq!(
            resolve_browser(&edge, Some("Chromium"), Some("130")).1,
            "130.0.2849.80"
        );
        let chrome = parse_user_agent(Some("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36"));
        assert_eq!(
            resolve_browser(&chrome, Some("Brave"), Some("130")).0,
            "Brave"
        );
        assert_eq!(resolve_browser(&chrome, Some(""), None).0, "Chrome");
    }
    #[test]
    fn recognizes_ios_browsers_and_tablets() {
        let ios = parse_user_agent(Some("Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/120.0.6099.119 Mobile/15E148 Safari/604.1"));
        assert_eq!(ios.browser_name.as_deref(), Some("Chrome"));
        let ipad = parse_user_agent(Some("Mozilla/5.0 (iPad; CPU OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1"));
        assert_eq!(ipad.device_type.as_deref(), Some("tablet"));
    }
}

#[cfg(test)]
mod modern_browser_tests {
    use super::*;

    #[test]
    fn shared_browser_identity_fixtures() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../packages/analytics/test/browser-fixtures.json"
        ))
        .unwrap();
        for case in fixtures.as_array().unwrap() {
            let ua = case["ua"].as_str().unwrap();
            let parsed = parse_user_agent(Some(ua));
            assert_eq!(
                parsed.browser_name.as_deref(),
                case["name"].as_str(),
                "{ua}"
            );
            assert_eq!(
                parsed.browser_version.as_deref(),
                case["version"].as_str(),
                "{ua}"
            );
            for generic in ["Chromium", "Google Chrome", "Firefox", "Safari"] {
                if !matches!(
                    parsed.browser_name.as_deref(),
                    Some("Chrome" | "Firefox" | "Safari")
                ) {
                    assert_eq!(
                        resolve_browser(&parsed, Some(generic), Some("130")).0,
                        case["name"].as_str().unwrap(),
                        "generic hint overwrote {ua}"
                    );
                }
            }
        }
    }

    #[test]
    fn canonical_product_names_are_shared_with_reporting() {
        let sql = browser_sql("browser_name");
        for (alias, expected) in [
            ("Arc Browser", "Arc"),
            ("Arc Search", "Arc"),
            ("Perplexity Comet", "Comet"),
            ("Dia Browser", "Dia"),
            ("Zen Browser", "Zen"),
            ("Floorp Browser", "Floorp"),
            ("Brave Browser", "Brave"),
            ("Samsung Browser", "Samsung Internet"),
            ("UCBrowser", "UC Browser"),
            ("HeadlessChrome", "Headless Chrome"),
        ] {
            assert_eq!(canonical_browser(alias).as_deref(), Some(expected));
            assert!(sql.contains(&format!("'{}'", alias.to_lowercase())));
            assert!(sql.contains(&format!("'{}'", expected)));
        }
    }
}
