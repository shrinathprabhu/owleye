use super::report::ReportPlan;
use crate::ApiError;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone)]
pub struct AiSettings {
    pub api_key: Option<String>,
    pub model: String,
    pub clickhouse_url: Option<String>,
    #[cfg(test)]
    pub endpoint: Option<String>,
}
impl Default for AiSettings {
    fn default() -> Self {
        Self {
            api_key: None,
            clickhouse_url: None,
            model: "z-ai/glm-5.2".into(),
            #[cfg(test)]
            endpoint: None,
        }
    }
}
impl AiSettings {
    pub fn from_env() -> anyhow::Result<Self> {
        let mut settings = Self {
            api_key: std::env::var("OPENROUTER_API_KEY")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            clickhouse_url: std::env::var("OWLEYE_AI_CLICKHOUSE_URL")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            ..Self::default()
        };
        if let Some(url) = &settings.clickhouse_url {
            let url = reqwest::Url::parse(url)?;
            anyhow::ensure!(
                matches!(url.scheme(), "http" | "https"),
                "OWLEYE_AI_CLICKHOUSE_URL must use HTTP or HTTPS"
            );
        }
        if let Ok(model) = std::env::var("OWLEYE_AI_MODEL") {
            anyhow::ensure!(
                !model.is_empty()
                    && model.len() <= 150
                    && model.contains('/')
                    && model
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_/.:".contains(c)),
                "OWLEYE_AI_MODEL must be an OpenRouter model identifier"
            );
            settings.model = model;
        }
        Ok(settings)
    }
    pub fn available(&self) -> bool {
        self.api_key
            .as_ref()
            .is_some_and(|key| !key.trim().is_empty())
    }
}

fn unavailable() -> ApiError {
    ApiError::ServiceUnavailable("AI provider is unavailable or returned an invalid response; no prompt allowance was consumed".into())
}

async fn completion(
    config: &AiSettings,
    messages: Value,
    schema: Option<Value>,
) -> Result<String, ApiError> {
    let key = config
        .api_key
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(unavailable)?;
    let endpoint = "https://openrouter.ai/api/v1/chat/completions";
    #[cfg(test)]
    let endpoint = config.endpoint.as_deref().unwrap_or(endpoint);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(35))
        .build()
        .map_err(|_| unavailable())?;
    let mut body = json!({
        "model": config.model, "messages": messages,
        "stream": false, "max_tokens": 2400, "temperature": 0,
        "reasoning": {"enabled": false},
        "provider": {"require_parameters": true, "data_collection": "deny", "zdr": true, "ignore": ["google-vertex", "google-ai-studio"]}
    });
    if let Some(schema) = schema {
        body["response_format"] = json!({"type":"json_schema", "json_schema": {"name":"analytics_report", "strict":true, "schema": schema}});
    }
    let mut response = client
        .post(endpoint)
        .bearer_auth(key)
        .header("X-OpenRouter-Title", "OwlEye")
        .json(&body)
        .send()
        .await
        .map_err(|_| unavailable())?;
    if !response.status().is_success() {
        return Err(unavailable());
    }
    // Do not log or forward provider error bodies, prompts, or keys.
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > 64 * 1024 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    let choices = value["choices"]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or_else(unavailable)?;
    let choice = &choices[0];
    if choice["finish_reason"] != "stop"
        || choice["message"]["tool_calls"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    {
        return Err(unavailable());
    }
    let text = choice["message"]["content"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 12_000)
        .ok_or_else(unavailable)?;
    Ok(text.to_owned())
}

pub(super) async fn plan_with_catalog(
    config: &AiSettings,
    prompt: &str,
    catalog: &super::insights::Catalog,
    context: &[super::ConversationTurn],
) -> Result<ReportPlan, ApiError> {
    let prompt = super::report::normalize_question(prompt);
    let strings = json!({"type":"array","items":{"type":"string"},"maxItems":10});
    let properties = json!({"type":"array","maxItems":3,"items":{"type":"object","additionalProperties":false,"properties":{"key":{"type":"string"},"value":{"type":"string"}},"required":["key","value"]}});
    let result = completion(config, json!([
        {"role":"system", "content":include_str!("skills/analytics-v1.md")},
        {"role":"user", "content":serde_json::to_string(&json!({"question":prompt,"clarification_context":context,"catalog":catalog,"recognized_categories":super::report::recognized_filters(&prompt),"today_utc":chrono::Utc::now().date_naive().to_string()})).map_err(|_| unavailable())?}
    ]), Some(json!({
        "type":"object", "additionalProperties":false,
        "properties": {
            "report":{"type":"string", "enum":["totals","daily","weekly","country","city","event","campaign","funnel","browser","device","os","clarification","unsupported"]},
            "days":{"type":"integer", "minimum":1,"maximum":366},
            "offset_days":{"type":"integer","minimum":0,"maximum":36500},
            "period":{"type":"string","enum":["rolling","this_week","last_week","this_month","last_month","custom"]},
            "start_date":{"type":["string","null"]}, "end_date":{"type":["string","null"]},
            "country":strings,
            "browser":{"type":"array","items":{"type":"string","enum":["Chrome","Firefox","Safari","Edge","Opera","Samsung Internet"]},"maxItems":10},
            "device":{"type":"array","items":{"type":"string","enum":["desktop","mobile","tablet"]},"maxItems":10},
            "os":{"type":"array","items":{"type":"string","enum":["macOS","Windows","Linux","Android","iOS","Chrome OS"]},"maxItems":10},
            "cities":{"type":"array","maxItems":10,"items":{"type":"object","additionalProperties":false,"properties":{"city":{"type":"string"},"country":{"type":"string"}},"required":["city","country"]}},
            "events":strings,"campaigns":strings,"properties":properties,
            "steps":{"type":"array","maxItems":3,"items":{"type":"object","additionalProperties":false,"properties":{"label":{"type":"string"},"events":strings,"properties":properties},"required":["label","events","properties"]}},
            "clarification":{"type":["string","null"]},
            "chart":{"type":"string","enum":["auto","none","line","bar","pie","donut"]},
            "metric":{"type":"string","enum":["events","pageviews","visitors","sessions"]},
            "output":{"type":"string","enum":["text","pdf"]}
        }, "required":["report","days","offset_days","period","start_date","end_date","country","browser","device","os","cities","events","campaigns","properties","steps","clarification","chart","metric","output"]
    }))).await?;
    let mut plan: ReportPlan = serde_json::from_str(&result).map_err(|_| unavailable())?;
    // A planner may attach a valid clarification while retaining its tentative report.
    // Asking the question always wins over executing an incomplete or guessed plan.
    if plan
        .clarification
        .as_ref()
        .is_some_and(|q| !q.trim().is_empty())
    {
        plan.report = super::report::Report::Clarification;
    }
    if plan.report == super::report::Report::Clarification {
        let question = plan
            .clarification
            .as_deref()
            .filter(|q| !q.trim().is_empty() && q.len() <= 1000)
            .ok_or_else(unavailable)?;
        plan.clarification = Some(super::sanitize::redact(question));
        return Ok(plan);
    }
    if plan.report == super::report::Report::Unsupported {
        return Err(ApiError::BadRequest("I can analyze this app's retained traffic, locations, events, properties and ordered conversions. Please ask an aggregate analytics question or specify the metric you want. No prompt allowance was consumed.".into()));
    }
    super::report::apply_explicit_day_range(&prompt, &mut plan)?;
    plan.normalize()?;
    super::report::apply_question_constraints(&prompt, &mut plan)?;
    if let Some(question) = super::insights::validate_catalog(&plan, catalog) {
        plan.report = super::report::Report::Clarification;
        plan.clarification = Some(question);
    }
    Ok(plan)
}

pub(super) async fn explain(
    config: &AiSettings,
    prompt: &str,
    evidence: &super::report::Evidence,
) -> Result<String, ApiError> {
    let text = completion(config, json!([
        {"role":"system", "content":include_str!("skills/explain-v1.md")},
        {"role":"user", "content":serde_json::to_string(&json!({"question":prompt,"evidence":evidence,"display":evidence.answer_context()})).map_err(|_| unavailable())?}
    ]), None).await?;
    validate_explanation(&text, evidence)?;
    Ok(super::sanitize::redact(&text))
}

fn validate_explanation(text: &str, evidence: &super::report::Evidence) -> Result<(), ApiError> {
    static DATES: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").expect("date regex")
    });
    let display = evidence.answer_context();
    let lower = text.to_lowercase();
    let has_period = lower.contains(&display.period.to_lowercase())
        || (text.contains(&evidence.start_date) && text.contains(&evidence.end_date));
    let has_label = |label: &str| {
        regex::Regex::new(&format!(r"(?i)\b{}\b", regex::escape(label)))
            .expect("escaped label")
            .is_match(text)
    };
    let filters_missing = [
        (&evidence.country, &display.countries),
        (&evidence.browser, &display.browsers),
        (&evidence.device, &display.devices),
        (&evidence.os, &display.operating_systems),
    ]
    .into_iter()
    .any(|(codes, names)| {
        codes.iter().zip(names).any(|(code, name)| {
            // Country codes stay case-sensitive so the word "in" cannot satisfy IN.
            let canonical_present = regex::Regex::new(&format!(r"\b{}\b", regex::escape(code)))
                .expect("escaped code")
                .is_match(text);
            !has_label(name) && !canonical_present
        })
    });
    if !has_period
        || filters_missing
        || DATES.find_iter(text).any(|date| {
            date.as_str() != evidence.start_date
                && date.as_str() != evidence.end_date
                && !evidence.rows.iter().any(|row| row.label == date.as_str())
                && !evidence
                    .details
                    .notes
                    .iter()
                    .any(|note| note.contains(date.as_str()))
        })
    {
        return Err(unavailable());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::report::{Chart, Evidence, Metric, Output};
    #[test]
    fn natural_answers_keep_verified_filters_and_period() {
        let evidence = Evidence {
            report: "totals".into(),
            start_date: "2026-09-07".into(),
            end_date: "2026-09-20".into(),
            timezone: "UTC",
            period_label: "last 14 days".into(),
            groups_suppressed_below_visitors: 5,
            country: vec!["IN".into()],
            browser: vec![],
            device: vec![],
            os: vec!["macOS".into()],
            chart: Chart::None,
            metric: Metric::Visitors,
            output: Output::Text,
            rows: vec![],
            details: Default::default(),
        };
        let display = evidence.answer_context();
        assert_eq!(display.countries, ["India"]);
        assert_eq!(display.operating_systems, ["Mac"]);
        assert!(validate_explanation(
            "In the last 14 days, 8 visitors used your app from India on Mac.",
            &evidence
        )
        .is_ok());
        for answer in [
            "In the last 7 days, 8 visitors used your app from India on Mac.",
            "In the last 14 days, 8 visitors used your app on Mac.",
            "In the last 14 days, 8 visitors used your app from India.",
            "In the last 14 days, 8 visitors from India used Mac on 2000-01-01.",
        ] {
            assert!(validate_explanation(answer, &evidence).is_err(), "{answer}");
        }
    }
}
