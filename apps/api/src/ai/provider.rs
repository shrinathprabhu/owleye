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
            "dataset":{"type":"string","enum":["events","performance","engagement","uptime"]},
            "measure":{"type":"string","enum":["count","mean","p50","p75","p95","total"]},
            "report":{"type":"string", "enum":["totals","daily","weekly","country","city","event","page","campaign","funnel","browser","device","os","clarification","unsupported"]},
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
            "metric":{"type":"string","enum":["events","pageviews","visitors","sessions","value"]},
            "output":{"type":"string","enum":["text","pdf"]}
        }, "required":["dataset","measure","report","days","offset_days","period","start_date","end_date","country","browser","device","os","cities","events","campaigns","properties","steps","clarification","chart","metric","output"]
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
) -> Result<(String, &'static str), ApiError> {
    let result = completion(config, json!([
        {"role":"system", "content":include_str!("skills/explain-v1.md")},
        {"role":"user", "content":serde_json::to_string(&json!({"question":prompt,"evidence":evidence,"display":evidence.answer_context()})).map_err(|_| unavailable())?}
    ]), None).await;
    Ok(checked_explanation(result, evidence))
}

#[cfg(test)]
fn checked_answer(result: Result<String, ApiError>, evidence: &super::report::Evidence) -> String {
    checked_explanation(result, evidence).0
}

fn checked_explanation(
    result: Result<String, ApiError>,
    evidence: &super::report::Evidence,
) -> (String, &'static str) {
    match result {
        Ok(text) if validate_explanation(&text, evidence).is_ok() => {
            tracing::info!(
                event = "ai_explanation",
                outcome = "accepted",
                "AI explanation evaluated"
            );
            (super::sanitize::redact(&text), "model")
        }
        result => {
            let reason = if result.is_err() {
                "provider_error"
            } else {
                "validation_failed"
            };
            tracing::warn!(
                event = "ai_explanation",
                outcome = "fallback",
                reason,
                "AI explanation evaluated"
            );
            (fallback_answer(evidence), "fallback")
        }
    }
}

// Only already-suppressed evidence is used; never query or infer hidden rows.
fn fallback_answer(evidence: &super::report::Evidence) -> String {
    use super::report::Metric;
    let display = evidence.answer_context();
    let mut scope = Vec::new();
    for (label, values) in [
        ("countries", &display.countries),
        ("browsers", &display.browsers),
        ("devices", &display.devices),
        ("operating systems", &display.operating_systems),
        ("cities", &evidence.details.cities),
        ("events", &evidence.details.events),
        ("campaigns", &evidence.details.campaigns),
        ("properties", &evidence.details.properties),
    ] {
        if !values.is_empty() {
            scope.push(format!("{label}: {}", values.join(", ")));
        }
    }
    let metric = match evidence.metric {
        Metric::Events => "events",
        Metric::Pageviews => "page views",
        Metric::Visitors => "estimated visitors",
        Metric::Sessions => "estimated sessions",
        Metric::Value => {
            if evidence
                .details
                .events
                .iter()
                .any(|event| event == "web_vital_cls")
            {
                "CLS"
            } else {
                "ms"
            }
        }
    };
    let mut answer = format!(
        "{} ({} to {}, UTC).",
        display.period, evidence.start_date, evidence.end_date
    );
    if !scope.is_empty() {
        answer.push_str(&format!(" Filters — {}.", scope.join("; ")));
    }
    if evidence.rows.is_empty() {
        answer.push_str(" No reportable rows are available. This can mean no matching activity or groups withheld for privacy.");
    } else {
        for row in evidence.rows.iter().take(5) {
            let value = match evidence.metric {
                Metric::Events => row.events.to_string(),
                Metric::Pageviews => row.pageviews.to_string(),
                Metric::Visitors => row.visitors.to_string(),
                Metric::Sessions => row.sessions.to_string(),
                Metric::Value => {
                    if evidence
                        .details
                        .events
                        .iter()
                        .any(|event| event == "web_vital_cls")
                    {
                        format!("{:.3}", row.value)
                    } else {
                        format!("{:.1}", row.value)
                    }
                }
            };
            answer.push_str(&format!("\n{}: {value} {metric}.", row.label));
        }
        if evidence.rows.len() > 5 {
            answer.push_str("\nSee the supporting data for the remaining groups.");
        }
    }
    answer.push_str(&format!(
        "\nGroups below {} visitors are withheld where privacy suppression applies.",
        evidence.groups_suppressed_below_visitors
    ));
    for rate in &evidence.derived {
        answer.push_str(&format!(
            "\n{}: {:.1}% ({}; {}).",
            rate.label,
            rate.percent,
            rate.kind.replace('_', " "),
            rate.denominator_scope
        ));
    }
    for note in &evidence.details.notes {
        answer.push_str(&format!("\n{note}"));
    }
    super::sanitize::redact(&answer)
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
        || !numbers_grounded(text, evidence)
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

// Check numeric claims against evidence, allowing common display rounding.
// This is a grounding check, not a proof that a number is attached to the right claim.
fn numbers_grounded(text: &str, evidence: &super::report::Evidence) -> bool {
    static NUMBERS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"[-+]?\b\d+(?:,\d{3})*(?:\.\d+)?\b").expect("number regex")
    });
    fn collect_numbers(value: &Value, output: &mut Vec<f64>) {
        match value {
            Value::Number(number) => {
                if let Some(n) = number.as_f64() {
                    output.push(n);
                }
            }
            Value::Array(values) => values.iter().for_each(|v| collect_numbers(v, output)),
            Value::Object(values) => values.values().for_each(|v| collect_numbers(v, output)),
            _ => {}
        }
    }
    let mut values = Vec::new();
    collect_numbers(
        &serde_json::to_value(evidence).unwrap_or_default(),
        &mut values,
    );
    // Dates, period names and group labels are context, not metric values.
    // Their digits must not justify an invented count (e.g. 2026 visitors).
    let dates = regex::Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").expect("date regex");
    let mut narrative = dates.replace_all(text, "").into_owned();
    for label in std::iter::once(evidence.period_label.as_str())
        .chain(evidence.rows.iter().map(|r| r.label.as_str()))
    {
        if !label.is_empty() {
            let pattern = regex::Regex::new(&format!(r"(?i)\b{}\b", regex::escape(label)))
                .expect("escaped label");
            narrative = pattern.replace_all(&narrative, "").into_owned();
        }
    }
    let percentages =
        regex::Regex::new(r"([-+]?\d+(?:\.\d+)?)\s*(?:%|percent\b)").expect("percent regex");
    if percentages.captures_iter(&narrative).any(|m| {
        let value: f64 = m[1].parse().unwrap_or(f64::NAN);
        !evidence.derived.iter().any(|r| {
            value == r.percent
                || value == r.percent.round()
                || value == (r.percent * 10.0).round() / 10.0
        })
    }) {
        return false;
    }
    // "Top 3 pages" describes the displayed row count, not a traffic metric.
    let ranks = regex::Regex::new(r"(?i)\b(?:top|first)\s+(\d+)\s+(?:pages?|rows?|groups?|results?|events?|countries|browsers?|devices?|campaigns?)\b").expect("rank regex");
    let mut invalid_rank = false;
    narrative = ranks
        .replace_all(&narrative, |m: &regex::Captures<'_>| {
            if !m[1]
                .parse::<usize>()
                .is_ok_and(|n| n > 0 && n <= evidence.rows.len())
            {
                invalid_rank = true;
            }
            "".to_string()
        })
        .into_owned();
    if invalid_rank {
        return false;
    }
    NUMBERS.find_iter(&narrative).all(|m| {
        let Ok(value) = m.as_str().replace(',', "").parse::<f64>() else {
            return false;
        };
        values
            .iter()
            .any(|n| value == *n || value == n.round() || value == (n * 10.0).round() / 10.0)
    })
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
            rows: vec![super::super::report::ReportRow {
                label: "Total".into(),
                events: 8,
                pageviews: 8,
                visitors: 8,
                sessions: 8,
                value: 0.0,
            }],
            details: Default::default(),
            derived: vec![],
        };
        let display = evidence.answer_context();
        assert_eq!(display.countries, ["India"]);
        assert_eq!(display.operating_systems, ["Mac"]);
        assert!(validate_explanation(
            "In the last 14 days, 8 visitors used your app from India on Mac.",
            &evidence
        )
        .is_ok());
        for result in [
            Err(unavailable()),
            Ok("Invalid narrative: 999 visitors".into()),
        ] {
            let answer = checked_answer(result, &evidence);
            assert!(answer.contains("Total: 8 estimated visitors"));
            assert!(answer.contains("India"));
            assert!(answer.contains("Mac"));
            assert!(!answer.contains("999"));
        }
        let mut withheld = Evidence {
            rows: vec![],
            ..evidence
        };
        assert!(fallback_answer(&withheld).contains("No reportable rows"));
        assert!(!fallback_answer(&withheld).contains("0 estimated visitors"));
        withheld.rows.push(super::super::report::ReportRow {
            label: "Total".into(),
            events: 8,
            pageviews: 8,
            visitors: 8,
            sessions: 8,
            value: 0.0,
        });
        let evidence = withheld;
        for answer in [
            "In the last 14 days, 999 visitors used your app from India on Mac.",
            "In the last 14 days, -8 visitors used your app from India on Mac.",
            "In the last 14 days, 2026 visitors used your app from India on Mac.",
            "In the last 7 days, 8 visitors used your app from India on Mac.",
            "In the last 14 days, 8 visitors used your app on Mac.",
            "In the last 14 days, 8 visitors used your app from India.",
            "In the last 14 days, 8 visitors from India used Mac on 2000-01-01.",
        ] {
            assert!(validate_explanation(answer, &evidence).is_err(), "{answer}");
        }
    }

    #[test]
    fn legitimate_rates_ranks_and_measurement_units_survive_grounding() {
        let row = |label: &str, visitors| super::super::report::ReportRow {
            label: label.into(),
            events: 0,
            pageviews: 0,
            visitors,
            sessions: 0,
            value: 0.0,
        };
        let mut evidence = Evidence {
            report: "funnel".into(),
            start_date: "2026-09-01".into(),
            end_date: "2026-09-07".into(),
            timezone: "UTC",
            period_label: "last 7 days".into(),
            groups_suppressed_below_visitors: 5,
            country: vec![],
            browser: vec![],
            device: vec![],
            os: vec![],
            chart: Chart::None,
            metric: Metric::Visitors,
            output: Output::Text,
            rows: vec![row("Viewed", 80), row("Converted", 30)],
            details: Default::default(),
            derived: vec![],
        };
        evidence.derived =
            super::super::report::derived_rates("funnel", Metric::Visitors, &evidence.rows);
        assert!(validate_explanation(
            "In the last 7 days, a 38% conversion was observed.",
            &evidence
        )
        .is_ok());
        assert!(validate_explanation(
            "In the last 7 days, a 39% conversion was observed.",
            &evidence
        )
        .is_err());
        evidence.report = "page".into();
        evidence.rows = vec![row("/a", 40), row("/b", 20), row("/c", 10)];
        evidence.derived.clear();
        assert!(validate_explanation(
            "In the last 7 days, your top 3 pages were /a, /b and /c.",
            &evidence
        )
        .is_ok());
        assert!(validate_explanation(
            "In the last 7 days, your top 4 pages were /a, /b and /c.",
            &evidence
        )
        .is_err());
        evidence.metric = Metric::Value;
        evidence.rows[0].value = 0.125;
        evidence.rows.truncate(1);
        evidence.details.events = vec!["web_vital_cls".into()];
        assert!(fallback_answer(&evidence).contains("0.125 CLS"));
        evidence.rows[0].value = 2480.0;
        evidence.details.events = vec!["web_vital_lcp".into()];
        assert!(fallback_answer(&evidence).contains("2480.0 ms"));
        assert!(validate_explanation("In the last 7 days, LCP was 2480 ms.", &evidence).is_ok());
        assert!(
            validate_explanation("In the last 7 days, LCP was 2.5 seconds.", &evidence).is_err()
        );
        assert_eq!(
            checked_explanation(Err(unavailable()), &evidence).1,
            "fallback"
        );
        assert_eq!(
            checked_explanation(Ok("In the last 7 days, LCP was 2480 ms.".into()), &evidence).1,
            "model"
        );
    }
}
