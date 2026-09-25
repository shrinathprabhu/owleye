use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    errors::ApiError,
    privacy::{geoip::GeoLocation, identity::AnonymousIds, user_agent::ParsedUserAgent},
};

const MAX_BATCH_EVENTS: usize = 100;
const MAX_DATA_BYTES: usize = 16 * 1024;
const MAX_EVENT_AGE_DAYS: i64 = 90;
const MAX_EVENT_FUTURE_MINUTES: i64 = 5;
const MAX_EVENT_NAME_BYTES: usize = 128;
const MAX_URL_BYTES: usize = 4 * 1024;
const MAX_UTM_VALUE_BYTES: usize = 255;

#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    pub event: SdkEvent,
    pub site_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum IngestPayload {
    Single(Box<IngestRequest>),
    Batch(BatchIngestRequest),
}

#[derive(Debug, Deserialize)]
pub struct BatchIngestRequest {
    pub events: Vec<SdkEvent>,
    pub site_id: String,
}

impl IngestPayload {
    pub(crate) fn site_id(&self) -> &str {
        match self {
            Self::Single(request) => &request.site_id,
            Self::Batch(request) => &request.site_id,
        }
    }

    pub(crate) fn into_requests(self) -> Result<Vec<IngestRequest>, ApiError> {
        match self {
            Self::Single(request) => Ok(vec![*request]),
            Self::Batch(request) => {
                if request.events.is_empty() {
                    return Err(ApiError::BadRequest(
                        "At least one event is required".to_owned(),
                    ));
                }
                if request.events.len() > MAX_BATCH_EVENTS {
                    return Err(ApiError::BadRequest(format!(
                        "A batch can contain at most {MAX_BATCH_EVENTS} events"
                    )));
                }

                Ok(request
                    .events
                    .into_iter()
                    .map(|event| IngestRequest {
                        event,
                        site_id: request.site_id.clone(),
                    })
                    .collect())
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SdkEvent {
    #[serde(default)]
    pub sdk: Option<SdkIdentity>,
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(default)]
    pub environment: SdkEnvironment,
    pub name: String,
    #[serde(default)]
    pub page: SdkPage,
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(rename = "type")]
    pub event_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SdkIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct SdkEnvironment {
    pub browser: Option<String>,
    pub browser_version: Option<String>,
    pub color_scheme: Option<String>,
    pub device_pixel_ratio: Option<f64>,
    pub language: Option<String>,
    pub screen: Option<SdkSize>,
    pub timezone: Option<String>,
    pub viewport: Option<SdkSize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SdkPage {
    pub hash: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
    pub referrer: Option<String>,
    pub referrer_host: Option<String>,
    pub search: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdkSize {
    pub height: Option<u32>,
    pub width: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventRow {
    pub(crate) anon_session_id: String,
    pub(crate) anon_user_id: String,
    pub(crate) browser_name: String,
    pub(crate) browser_version: String,
    pub(crate) city: String,
    pub(crate) color_scheme: String,
    pub(crate) continent: String,
    pub(crate) country: String,
    pub(crate) device_pixel_ratio: Option<f64>,
    pub(crate) device_type: String,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) event_id: String,
    pub(crate) event_name: String,
    pub(crate) event_type: String,
    pub(crate) latitude: Option<f64>,
    pub(crate) locale: String,
    pub(crate) longitude: Option<f64>,
    pub(crate) occurred_at: String,
    pub(crate) os_name: String,
    pub(crate) os_version: String,
    pub(crate) page_title: String,
    pub(crate) payload_json: String,
    pub(crate) received_at: String,
    pub(crate) referrer: String,
    pub(crate) referrer_host: String,
    pub(crate) region: String,
    pub(crate) retention_active_until: Option<String>,
    pub(crate) retention_days: u16,
    pub(crate) retention_policy_version: u16,
    pub(crate) retention_purge_after: Option<String>,
    pub(crate) retention_tier: String,
    pub(crate) rule_id: String,
    pub(crate) rule_type: String,
    pub(crate) screen_height: Option<u32>,
    pub(crate) screen_width: Option<u32>,
    pub(crate) sdk_name: String,
    pub(crate) sdk_version: String,
    pub(crate) site_id: String,
    pub(crate) timezone: String,
    pub(crate) tier_at_ingestion: String,
    pub(crate) url: String,
    pub(crate) url_hash: String,
    pub(crate) url_host: String,
    pub(crate) url_path: String,
    pub(crate) url_search: String,
    pub(crate) utm_campaign: String,
    pub(crate) utm_medium: String,
    pub(crate) utm_source: String,
    pub(crate) visitor_id: String,
    pub(crate) viewport_height: Option<u32>,
    pub(crate) viewport_width: Option<u32>,
}

pub(crate) struct EventRowContext {
    pub(crate) anonymous_ids: AnonymousIds,
    pub(crate) location: GeoLocation,
    pub(crate) occurred_at: DateTime<Utc>,
    pub(crate) received_at: DateTime<Utc>,
    pub(crate) retention_days: u16,
    pub(crate) retention_stamp: crate::retention::RetentionStamp,
    pub(crate) user_agent: ParsedUserAgent,
}

impl EventRow {
    pub(crate) fn from_request(
        request: IngestRequest,
        context: EventRowContext,
    ) -> Result<Self, ApiError> {
        let EventRowContext {
            anonymous_ids,
            location,
            occurred_at,
            received_at,
            retention_days,
            retention_stamp,
            user_agent,
        } = context;
        let IngestRequest { event, site_id } = request;
        let SdkEvent {
            sdk,
            data,
            environment,
            name,
            mut page,
            timestamp: _,
            event_type,
        } = event;
        normalize_page_urls(&mut page);
        let duration_ms = data
            .as_ref()
            .and_then(|value| value.get("duration_ms"))
            .and_then(json_u64);
        let rule_id = data
            .as_ref()
            .and_then(|value| value.get("rule_id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let rule_type = data
            .as_ref()
            .and_then(|value| value.get("rule_type"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let payload_json = serde_json::to_string(&data)?;
        let screen_width = environment.screen.as_ref().and_then(|size| size.width);
        let screen_height = environment.screen.as_ref().and_then(|size| size.height);
        let viewport_width = environment.viewport.as_ref().and_then(|size| size.width);
        let viewport_height = environment.viewport.as_ref().and_then(|size| size.height);
        let utm = utm_attribution(page.search.as_deref());

        Ok(Self {
            anon_session_id: anonymous_ids.session_id,
            anon_user_id: anonymous_ids.user_id,
            browser_name: environment
                .browser
                .or(user_agent.browser_name)
                .unwrap_or_default(),
            browser_version: environment
                .browser_version
                .or(user_agent.browser_version)
                .unwrap_or_default(),
            city: location.city.unwrap_or_default(),
            color_scheme: environment.color_scheme.unwrap_or_default(),
            continent: location.continent.unwrap_or_default(),
            country: location.country.unwrap_or_default(),
            device_pixel_ratio: environment.device_pixel_ratio,
            device_type: user_agent.device_type.unwrap_or_default(),
            duration_ms,
            event_id: Uuid::new_v4().to_string(),
            event_name: name.trim().to_owned(),
            event_type: event_type.trim().to_owned(),
            latitude: location.latitude,
            locale: environment.language.unwrap_or_default(),
            longitude: location.longitude,
            occurred_at: clickhouse_timestamp(occurred_at),
            os_name: user_agent.os_name.unwrap_or_default(),
            os_version: user_agent.os_version.unwrap_or_default(),
            page_title: page.title.unwrap_or_default(),
            payload_json,
            received_at: clickhouse_timestamp(received_at),
            referrer: page.referrer.unwrap_or_default(),
            referrer_host: page.referrer_host.unwrap_or_default(),
            region: location.region.unwrap_or_default(),
            retention_active_until: retention_stamp.active_until.map(clickhouse_timestamp),
            retention_days,
            retention_policy_version: retention_stamp.policy_version,
            retention_purge_after: retention_stamp.purge_after.map(clickhouse_timestamp),
            retention_tier: retention_stamp.tier.to_owned(),
            rule_id,
            rule_type,
            screen_height,
            screen_width,
            sdk_name: sdk.as_ref().map(|sdk| sdk.name.clone()).unwrap_or_default(),
            sdk_version: sdk.map(|sdk| sdk.version).unwrap_or_default(),
            site_id: normalize_site_id(&site_id)?,
            timezone: environment.timezone.unwrap_or_default(),
            tier_at_ingestion: retention_stamp.tier.to_owned(),
            url: page.url.unwrap_or_default(),
            url_hash: page.hash.unwrap_or_default(),
            url_host: page.host.unwrap_or_default(),
            url_path: page.path.unwrap_or_else(|| "/".to_owned()),
            url_search: page.search.unwrap_or_default(),
            utm_campaign: utm.campaign,
            utm_medium: utm.medium,
            utm_source: utm.source,
            visitor_id: anonymous_ids.visitor_id,
            viewport_height,
            viewport_width,
        })
    }
}

// An API integrator must explicitly send search/hash fields to retain them.
// A full URL alone must not accidentally persist login tokens or fragments.
fn normalize_page_urls(page: &mut SdkPage) {
    if let Some(raw) = page.url.as_deref() {
        if let Ok(mut url) = reqwest::Url::parse(raw) {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(
                page.search
                    .as_deref()
                    .map(|value| value.trim_start_matches('?')),
            );
            url.set_fragment(
                page.hash
                    .as_deref()
                    .map(|value| value.trim_start_matches('#')),
            );
            page.host
                .get_or_insert_with(|| url.host_str().unwrap_or_default().to_owned());
            page.path.get_or_insert_with(|| url.path().to_owned());
            page.url = Some(url.to_string());
        }
    }
    if let Some(raw) = page.referrer.as_deref() {
        if let Ok(mut url) = reqwest::Url::parse(raw) {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            if page.search.is_none() {
                url.set_query(None);
            }
            if page.hash.is_none() {
                url.set_fragment(None);
            }
            page.referrer = Some(url.to_string());
        }
    }
}

pub fn validate_event(event: &SdkEvent, now: DateTime<Utc>) -> Result<(), ApiError> {
    if let Some(sdk) = &event.sdk {
        validate_optional_text("SDK name", Some(&sdk.name), 64)?;
        validate_optional_text("SDK version", Some(&sdk.version), 32)?;
    }
    validate_bounded_text("event name", &event.name, 1, MAX_EVENT_NAME_BYTES)?;

    if !matches!(
        event.event_type.trim(),
        "pageview" | "page_session" | "external" | "rule" | "performance"
    ) {
        return Err(ApiError::BadRequest(
            "Unsupported analytics event type".to_owned(),
        ));
    }

    if let Some(timestamp) = event.timestamp.as_ref() {
        let oldest = now - Duration::days(MAX_EVENT_AGE_DAYS);
        let newest = now + Duration::minutes(MAX_EVENT_FUTURE_MINUTES);
        if timestamp < &oldest {
            return Err(ApiError::BadRequest(format!(
                "Event timestamp cannot be more than {MAX_EVENT_AGE_DAYS} days old"
            )));
        }
        if timestamp > &newest {
            return Err(ApiError::BadRequest(format!(
                "Event timestamp cannot be more than {MAX_EVENT_FUTURE_MINUTES} minutes in the future"
            )));
        }
    }

    validate_optional_text("page URL", event.page.url.as_deref(), MAX_URL_BYTES)?;
    validate_optional_text(
        "page referrer",
        event.page.referrer.as_deref(),
        MAX_URL_BYTES,
    )?;
    validate_optional_text("page path", event.page.path.as_deref(), 2 * 1024)?;
    validate_optional_text("page search", event.page.search.as_deref(), 2 * 1024)?;
    validate_optional_text("page hash", event.page.hash.as_deref(), 2 * 1024)?;
    validate_optional_text("page title", event.page.title.as_deref(), 512)?;
    validate_optional_text("page host", event.page.host.as_deref(), 255)?;
    validate_optional_text("referrer host", event.page.referrer_host.as_deref(), 255)?;
    validate_optional_text("browser name", event.environment.browser.as_deref(), 128)?;
    validate_optional_text(
        "browser version",
        event.environment.browser_version.as_deref(),
        128,
    )?;
    validate_optional_text("language", event.environment.language.as_deref(), 64)?;
    validate_optional_text("timezone", event.environment.timezone.as_deref(), 128)?;

    if let Some(color_scheme) = event.environment.color_scheme.as_deref() {
        if !matches!(color_scheme, "dark" | "light") {
            return Err(ApiError::BadRequest(
                "color_scheme must be 'dark' or 'light'".to_owned(),
            ));
        }
    }
    if let Some(ratio) = event.environment.device_pixel_ratio {
        if !ratio.is_finite() || !(0.1..=16.0).contains(&ratio) {
            return Err(ApiError::BadRequest(
                "device_pixel_ratio is outside the supported range".to_owned(),
            ));
        }
    }
    for size in [
        event.environment.screen.as_ref(),
        event.environment.viewport.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if size.width.is_some_and(|value| value > 100_000)
            || size.height.is_some_and(|value| value > 100_000)
        {
            return Err(ApiError::BadRequest(
                "screen or viewport dimensions are outside the supported range".to_owned(),
            ));
        }
    }

    if let Some(data) = &event.data {
        let data_size = serde_json::to_vec(data)?.len();
        if data_size > MAX_DATA_BYTES {
            return Err(ApiError::BadRequest(format!(
                "Event data cannot exceed {MAX_DATA_BYTES} bytes"
            )));
        }
        if data
            .get("duration_ms")
            .and_then(json_u64)
            .is_some_and(|duration| duration > 7 * 24 * 60 * 60 * 1_000)
        {
            return Err(ApiError::BadRequest(
                "Event duration cannot exceed 7 days".to_owned(),
            ));
        }
    }

    validate_event_data(event)
}

pub fn validate_event_data(event: &SdkEvent) -> Result<(), ApiError> {
    match event.event_type.trim() {
        "external" | "rule" => validate_external_records(event),
        "performance" => validate_performance_records(event),
        _ => Ok(()),
    }
}

pub fn normalize_site_id(site_id: &str) -> Result<String, ApiError> {
    let site_id = site_id.trim();
    if site_id.is_empty()
        || site_id.len() > 128
        || !site_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ApiError::BadRequest(
            "A valid site_id is required".to_owned(),
        ));
    }

    Ok(site_id.to_owned())
}

fn validate_bounded_text(
    label: &str,
    value: &str,
    min_bytes: usize,
    max_bytes: usize,
) -> Result<(), ApiError> {
    let value = value.trim();
    if value.len() < min_bytes || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(format!(
            "{label} must be between {min_bytes} and {max_bytes} bytes"
        )));
    }

    Ok(())
}

fn validate_optional_text(
    label: &str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), ApiError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.len() > max_bytes || value.chars().any(|character| character == '\0') {
        return Err(ApiError::BadRequest(format!(
            "{label} cannot exceed {max_bytes} bytes"
        )));
    }

    Ok(())
}

pub fn clickhouse_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn json_u64(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|value| *value >= 0.0)
            .map(|value| value as u64)
    })
}

#[derive(Default)]
struct UtmAttribution {
    campaign: String,
    medium: String,
    source: String,
}

fn utm_attribution(search: Option<&str>) -> UtmAttribution {
    let Some(search) = search else {
        return UtmAttribution::default();
    };
    let mut attribution = UtmAttribution::default();

    for pair in search.trim_start_matches('?').split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let Some(key) = decode_query_component(key) else {
            continue;
        };
        let target = if key.eq_ignore_ascii_case("utm_source") {
            &mut attribution.source
        } else if key.eq_ignore_ascii_case("utm_medium") {
            &mut attribution.medium
        } else if key.eq_ignore_ascii_case("utm_campaign") {
            &mut attribution.campaign
        } else {
            continue;
        };
        if target.is_empty() {
            *target = decode_query_component(value).unwrap_or_default();
        }
    }

    attribution
}

fn decode_query_component(value: &str) -> Option<String> {
    let plus_as_space = value.replace('+', " ");
    let decoded = urlencoding::decode(&plus_as_space).ok()?;
    let decoded = decoded.trim();
    if decoded.is_empty() || decoded.chars().any(char::is_control) {
        return None;
    }

    let end = decoded
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= MAX_UTM_VALUE_BYTES)
        .last()
        .unwrap_or(0);
    if decoded.len() <= MAX_UTM_VALUE_BYTES {
        Some(decoded.to_owned())
    } else {
        Some(decoded[..end].to_owned())
    }
}

fn validate_external_records(event: &SdkEvent) -> Result<(), ApiError> {
    let Some(data) = &event.data else {
        return Ok(());
    };
    let Some(records) = data.get("records") else {
        return Ok(());
    };
    validate_record_object("event records", records, 10)
}

fn validate_performance_records(event: &SdkEvent) -> Result<(), ApiError> {
    let Some(data) = &event.data else {
        return Ok(());
    };

    for key in ["start", "end"] {
        let Some(records) = data.get(key) else {
            continue;
        };

        validate_record_object(&format!("performance event {key} records"), records, 10)?;
    }

    Ok(())
}

fn validate_record_object(label: &str, value: &Value, limit: usize) -> Result<(), ApiError> {
    let Some(records) = value.as_object() else {
        return Err(ApiError::BadRequest(format!(
            "{label} must be a JSON object"
        )));
    };

    if records.len() > limit {
        return Err(ApiError::BadRequest(format!(
            "{label} accept at most {limit} fields"
        )));
    }

    for (key, value) in records {
        if !matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_)) {
            return Err(ApiError::BadRequest(format!(
                "{label} field '{key}' must be a string, number, or boolean"
            )));
        }
    }

    Ok(())
}

fn clickhouse_timestamp(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn custom_properties_accept_ten_and_reject_eleven_or_non_primitives() {
        let ten: serde_json::Map<String, Value> = (0..10)
            .map(|i| {
                (
                    format!("field{i}"),
                    match i % 3 {
                        0 => json!(true),
                        1 => json!("text"),
                        _ => json!(i),
                    },
                )
            })
            .collect();
        for event_type in ["external", "rule", "performance"] {
            let mut event = external_event(None);
            event.event_type = event_type.into();
            let keys: &[&str] = if event_type == "performance" {
                &["start", "end"]
            } else {
                &["records"]
            };
            for key in keys {
                event.data = Some(json!({*key:ten.clone()}));
                assert!(validate_event_data(&event).is_ok());
                let mut eleven = ten.clone();
                eleven.insert("extra".into(), json!(1));
                event.data = Some(json!({*key:eleven}));
                assert!(validate_event_data(&event).is_err());
                for invalid in [Value::Null, json!([]), json!({"nested":1})] {
                    event.data = Some(json!({*key:{"invalid":invalid}}));
                    assert!(validate_event_data(&event).is_err());
                }
            }
        }
    }
    #[test]
    fn page_urls_require_explicit_capture_fields_and_strip_credentials() {
        let mut page = SdkPage {
            url: Some("https://user:password@example.test/path?token=secret#private".to_owned()),
            referrer: Some("https://example.test/login?token=secret#private".to_owned()),
            ..SdkPage::default()
        };
        normalize_page_urls(&mut page);
        assert_eq!(page.url.as_deref(), Some("https://example.test/path"));
        assert_eq!(page.referrer.as_deref(), Some("https://example.test/login"));
        page.search = Some("?utm_source=newsletter".to_owned());
        page.hash = Some("#section".to_owned());
        normalize_page_urls(&mut page);
        assert_eq!(
            page.url.as_deref(),
            Some("https://example.test/path?utm_source=newsletter#section")
        );
    }

    fn external_event(data: Option<Value>) -> SdkEvent {
        SdkEvent {
            sdk: None,
            data,
            environment: SdkEnvironment::default(),
            event_type: "external".to_owned(),
            name: "signup_clicked".to_owned(),
            page: SdkPage::default(),
            timestamp: None,
        }
    }

    fn performance_event(data: Option<Value>) -> SdkEvent {
        SdkEvent {
            sdk: None,
            data,
            environment: SdkEnvironment::default(),
            event_type: "performance".to_owned(),
            name: "checkout_submit".to_owned(),
            page: SdkPage::default(),
            timestamp: None,
        }
    }

    #[test]
    fn accepts_external_records_with_primitive_values() {
        let event = external_event(Some(json!({
            "records": {
                "plan": "starter",
                "amount": 10,
                "annual": true
            }
        })));

        validate_event_data(&event).unwrap();
    }

    #[test]
    fn rejects_external_records_with_too_many_fields() {
        let event = external_event(Some(json!({
            "records": {
                "a": 1,
                "b": 2,
                "c": 3,
                "d": 4,
                "e": 5,
                "f": 6, "g": 7, "h": 8, "i": 9, "j": 10, "k": 11
            }
        })));

        assert!(validate_event_data(&event).is_err());
    }

    #[test]
    fn rejects_external_records_with_nested_values() {
        let event = external_event(Some(json!({
            "records": {
                "plan": {
                    "name": "starter"
                }
            }
        })));

        assert!(validate_event_data(&event).is_err());
    }

    #[test]
    fn accepts_performance_records_with_primitive_values() {
        let event = performance_event(Some(json!({
            "duration_ms": 24,
            "start": {
                "cart_items": 3,
                "source": "checkout"
            },
            "end": {
                "status": "ok"
            }
        })));

        validate_event_data(&event).unwrap();
    }

    #[test]
    fn rejects_performance_records_with_too_many_fields() {
        let event = performance_event(Some(json!({
            "duration_ms": 24,
            "start": {
                "a": 1,
                "b": 2,
                "c": 3,
                "d": 4, "e": 5, "f": 6, "g": 7, "h": 8, "i": 9, "j": 10, "k": 11
            }
        })));

        assert!(validate_event_data(&event).is_err());
    }

    #[test]
    fn rejects_performance_records_with_nested_values() {
        let event = performance_event(Some(json!({
            "duration_ms": 24,
            "end": {
                "status": {
                    "ok": true
                }
            }
        })));

        assert!(validate_event_data(&event).is_err());
    }

    #[test]
    fn expands_batch_ingest_payload() {
        let payload = IngestPayload::Batch(BatchIngestRequest {
            events: vec![external_event(None), performance_event(None)],
            site_id: "site_test".to_owned(),
        });
        let requests = payload.into_requests().unwrap();

        assert_eq!(requests.len(), 2);
        assert!(requests
            .iter()
            .all(|request| request.site_id == "site_test"));
    }

    #[test]
    fn boxed_single_ingest_payload_keeps_the_wire_shape() {
        let payload: IngestPayload = serde_json::from_value(json!({
            "site_id": "site_test",
            "event": {
                "name": "signup",
                "type": "external"
            }
        }))
        .unwrap();

        assert_eq!(payload.site_id(), "site_test");
        let requests = payload.into_requests().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].event.name, "signup");
        assert_eq!(requests[0].event.event_type, "external");
    }

    #[test]
    fn rejects_empty_ingest_batch() {
        let payload = IngestPayload::Batch(BatchIngestRequest {
            events: Vec::new(),
            site_id: "site_test".to_owned(),
        });

        assert!(payload.into_requests().is_err());
    }

    #[test]
    fn validates_supported_event_and_timestamp_bounds() {
        let now = Utc::now();
        let mut event = external_event(None);
        event.timestamp = Some(now + Duration::minutes(MAX_EVENT_FUTURE_MINUTES + 1));
        assert!(validate_event(&event, now).is_err());

        event.timestamp = Some(now - Duration::days(MAX_EVENT_AGE_DAYS + 1));
        assert!(validate_event(&event, now).is_err());

        event.timestamp = Some(now);
        assert!(validate_event(&event, now).is_ok());
    }

    #[test]
    fn rejects_unknown_event_types_and_oversized_data() {
        let now = Utc::now();
        let mut event = external_event(None);
        event.event_type = "arbitrary".to_owned();
        assert!(validate_event(&event, now).is_err());

        event.event_type = "external".to_owned();
        event.data = Some(json!({
            "records": {
                "value": "x".repeat(MAX_DATA_BYTES)
            }
        }));
        assert!(validate_event(&event, now).is_err());
    }

    #[test]
    fn site_identifiers_use_a_small_safe_character_set() {
        assert_eq!(
            normalize_site_id(" site_123.test ").unwrap(),
            "site_123.test"
        );
        assert!(normalize_site_id("site id").is_err());
        assert!(normalize_site_id("../site").is_err());
    }

    #[test]
    fn extracts_only_bounded_utm_attribution_from_opted_in_search() {
        let attribution = utm_attribution(Some(
            "?utm_source=founder%20newsletter&utm_medium=email&utm_campaign=Launch+Week&token=secret",
        ));

        assert_eq!(attribution.source, "founder newsletter");
        assert_eq!(attribution.medium, "email");
        assert_eq!(attribution.campaign, "Launch Week");
        assert!(!serde_json::to_string(&attribution.campaign)
            .unwrap()
            .contains("secret"));
    }

    #[test]
    fn event_rows_have_anonymous_ids_and_no_client_ip_field() {
        let request = IngestRequest {
            event: SdkEvent {
                sdk: None,
                data: None,
                environment: SdkEnvironment::default(),
                name: "page_viewed".to_owned(),
                page: SdkPage {
                    path: Some("/".to_owned()),
                    search: Some("?utm_campaign=privacy-launch".to_owned()),
                    ..SdkPage::default()
                },
                timestamp: None,
                event_type: "pageview".to_owned(),
            },
            site_id: "site_test".to_owned(),
        };
        let row = EventRow::from_request(
            request,
            EventRowContext {
                anonymous_ids: AnonymousIds {
                    session_id: "session-hash".to_owned(),
                    user_id: "daily-hash".to_owned(),
                    visitor_id: "visitor-hash".to_owned(),
                },
                location: GeoLocation::default(),
                occurred_at: Utc::now(),
                received_at: Utc::now(),
                retention_days: 90,
                retention_stamp: crate::retention::RetentionStamp {
                    active_until: Some(Utc::now() + chrono::Duration::days(90)),
                    policy_version: 1,
                    purge_after: Some(Utc::now() + chrono::Duration::days(90)),
                    tier: "free",
                },
                user_agent: ParsedUserAgent::default(),
            },
        )
        .unwrap();
        let serialized = serde_json::to_value(row).unwrap();
        let fields = serialized.as_object().unwrap();

        assert_eq!(fields["visitor_id"], "visitor-hash");
        assert_eq!(fields["utm_campaign"], "privacy-launch");
        assert_eq!(fields["retention_days"], 90);
        assert!(fields
            .keys()
            .all(|field| !field.contains("ip") && !field.contains("address")));
    }
}
