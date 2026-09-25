use std::borrow::Cow;

use woothee::{
    parser::{Parser, WootheeResult},
    woothee::VALUE_UNKNOWN,
};

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

    let Some(result) = Parser::new().parse(ua) else {
        return ParsedUserAgent::default();
    };

    ParsedUserAgent {
        browser_name: known(result.name),
        browser_version: known(result.version),
        device_type: device_type(&result),
        os_name: known(result.os),
        os_version: known_cow(result.os_version),
    }
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
        assert_eq!(parsed.os_name.as_deref(), Some("Mac OSX"));
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
        assert_eq!(parsed.os_name.as_deref(), Some("iPhone"));
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
