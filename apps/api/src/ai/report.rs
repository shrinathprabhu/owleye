use crate::{retention::ACTIVE_ROW_PREDICATE, ApiError, AppState};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Report {
    #[default]
    Totals,
    Daily,
    Weekly,
    Country,
    Browser,
    Device,
    Os,
    City,
    Event,
    Campaign,
    Funnel,
    Clarification,
    Unsupported,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Period {
    #[default]
    Rolling,
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    Custom,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Chart {
    Auto,
    #[default]
    None,
    Line,
    Bar,
    Pie,
    Donut,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Metric {
    Events,
    Pageviews,
    #[default]
    Visitors,
    Sessions,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Output {
    #[default]
    Text,
    Pdf,
}
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReportPlan {
    pub report: Report,
    pub days: u32,
    pub offset_days: u32,
    #[serde(default)]
    pub period: Period,
    #[serde(default, deserialize_with = "deserialize_filter")]
    pub country: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_filter")]
    pub browser: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_filter")]
    pub device: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_filter")]
    pub os: Vec<String>,
    #[serde(default)]
    pub chart: Chart,
    #[serde(default)]
    pub metric: Metric,
    #[serde(default)]
    pub output: Output,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub cities: Vec<super::insights::CityFilter>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub campaigns: Vec<String>,
    #[serde(default)]
    pub properties: Vec<super::insights::PropertyFilter>,
    #[serde(default)]
    pub steps: Vec<super::insights::FunnelStep>,
    #[serde(default)]
    pub clarification: Option<String>,
}
impl ReportPlan {
    pub fn normalize(&mut self) -> Result<(), ApiError> {
        normalize_values(&mut self.country, normalize_country)?;
        normalize_values(&mut self.browser, |v| {
            canonical(
                v,
                &[
                    "Chrome",
                    "Firefox",
                    "Safari",
                    "Edge",
                    "Opera",
                    "Samsung Internet",
                ],
            )
        })?;
        normalize_values(&mut self.device, |v| {
            canonical(v, &["desktop", "mobile", "tablet"])
        })?;
        normalize_values(&mut self.os, normalize_os)?;
        super::insights::normalize(self)?;
        self.validate()
    }
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.days == 0 || self.days > 366 || self.offset_days > 36500 {
            return Err(ApiError::BadRequest("Choose a date range of 1–366 days. Historical dates are supported when retained data is available. No prompt allowance was consumed.".into()));
        }
        if self.period == Period::Custom {
            let parse = |v: &Option<String>| {
                v.as_deref()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            };
            let (Some(start), Some(end)) = (parse(&self.start_date), parse(&self.end_date)) else {
                return Err(ApiError::BadRequest(
                    "Specify both start and end dates as YYYY-MM-DD.".into(),
                ));
            };
            if start > end
                || (end - start).num_days() >= 366
                || start.year() < 2000
                || end > Utc::now().date_naive()
            {
                return Err(ApiError::BadRequest("Choose ordered historical dates spanning at most 366 days and ending no later than today.".into()));
            }
        } else if self.start_date.is_some() || self.end_date.is_some() {
            return Err(ApiError::BadRequest(
                "Explicit dates require a custom date range.".into(),
            ));
        }
        if self.report == Report::Unsupported {
            return Err(ApiError::BadRequest("Ask about traffic totals, daily or weekly trends, countries, browsers, operating systems, or devices for this app, or clarify the location, event, property, and dates you want. No prompt allowance was consumed.".into()));
        }
        Ok(())
    }
    fn range(&self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let monday = today - Duration::days(today.weekday().num_days_from_monday().into());
        let month = today.with_day(1).expect("first day");
        match self.period {
            Period::Custom => (
                NaiveDate::parse_from_str(
                    self.start_date.as_deref().expect("validated date"),
                    "%Y-%m-%d",
                )
                .expect("validated date"),
                NaiveDate::parse_from_str(
                    self.end_date.as_deref().expect("validated date"),
                    "%Y-%m-%d",
                )
                .expect("validated date"),
            ),
            Period::ThisWeek => (monday, today),
            Period::LastWeek => (monday - Duration::days(7), monday - Duration::days(1)),
            Period::ThisMonth => (month, today),
            Period::LastMonth => {
                let end = month - Duration::days(1);
                (end.with_day(1).expect("first day"), end)
            }
            Period::Rolling => {
                let end = today - Duration::days(self.offset_days.into());
                (end - Duration::days(i64::from(self.days) - 1), end)
            }
        }
    }
    fn period_label(&self, start: &str, end: &str) -> String {
        match self.period {
            Period::Custom => format!("{start} to {end}"),
            Period::ThisWeek => "this week so far".into(),
            Period::LastWeek => "last week".into(),
            Period::ThisMonth => "this month so far".into(),
            Period::LastMonth => "last month".into(),
            Period::Rolling if self.offset_days == 0 && self.days == 1 => "today".into(),
            Period::Rolling if self.offset_days == 1 && self.days == 1 => "yesterday".into(),
            Period::Rolling if self.offset_days == 0 => format!("last {} days", self.days),
            Period::Rolling if self.offset_days == 1 => {
                format!("{} days ending yesterday", self.days)
            }
            Period::Rolling => format!("{start} to {end}"),
        }
    }
    fn filtered(&self) -> bool {
        !self.country.is_empty()
            || !self.browser.is_empty()
            || !self.device.is_empty()
            || !self.os.is_empty()
            || !self.cities.is_empty()
            || !self.events.is_empty()
            || !self.campaigns.is_empty()
            || !self.properties.is_empty()
    }
}
// Accept older single-value plans internally; the provider schema always uses arrays.
fn deserialize_filter<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Values {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Option::<Values>::deserialize(deserializer)? {
        None => vec![],
        Some(Values::One(v)) => vec![v],
        Some(Values::Many(v)) => v,
    })
}
fn canonical(value: &str, allowed: &[&str]) -> Option<String> {
    allowed
        .iter()
        .find(|name| name.eq_ignore_ascii_case(value.trim()))
        .map(|s| (*s).into())
}
fn normalize_values(
    values: &mut Vec<String>,
    normalize: impl Fn(&str) -> Option<String>,
) -> Result<(), ApiError> {
    if values.len() > 10 {
        return Err(invalid_filter());
    }
    *values = values
        .iter()
        .map(|v| normalize(v).ok_or_else(invalid_filter))
        .collect::<Result<_, _>>()?;
    values.sort();
    values.dedup();
    Ok(())
}
fn normalize_os(value: &str) -> Option<String> {
    match value.trim().to_lowercase().as_str() {
        "mac" | "macos" | "mac os" | "mac os x" | "mac osx" | "os x" | "osx" | "macintosh"
        | "macbook" | "imac" => Some("macOS".into()),
        "iphone" | "ipad" | "ipados" | "ios" => Some("iOS".into()),
        "chrome os" | "chromeos" | "chromebook" => Some("Chrome OS".into()),
        _ => canonical(value, &["Windows", "Linux", "Android"]),
    }
}

fn invalid_filter() -> ApiError {
    ApiError::BadRequest(
        "Unsupported country, browser, operating system, or device filter; no prompt allowance was consumed".into(),
    )
}
pub(super) fn normalize_country(value: &str) -> Option<String> {
    let value = value.trim().to_lowercase();
    let alias = match value.as_str() {
        "uk" | "u.k." | "britain" | "great britain" => "GB",
        "usa" | "u.s." | "u.s.a." | "united states of america" => "US",
        "bharat" => "IN",
        "uae" | "u.a.e." | "emirates" => "AE",
        "south korea" => "KR",
        "russia" => "RU",
        _ => "",
    };
    if !alias.is_empty() {
        return Some(alias.into());
    }
    let countries: std::collections::BTreeMap<String, String> =
        serde_json::from_str(include_str!("countries.json")).expect("bundled country names");
    countries
        .into_iter()
        .find(|(code, name)| code.eq_ignore_ascii_case(&value) || name.to_lowercase() == value)
        .map(|(code, _)| code)
}
// Explicit numeric rolling ranges are user constraints, not model interpretations.
// In particular, "last 30 days" must never become the previous calendar month.
pub(super) fn apply_explicit_day_range(
    question: &str,
    plan: &mut ReportPlan,
) -> Result<(), ApiError> {
    static DAYS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)\b(?:last|past)\s+([0-9]+)\s+days?\b").expect("day range regex")
    });
    let mut requested = None;
    for captures in DAYS.captures_iter(question) {
        let days: u32 = captures[1]
            .parse()
            .map_err(|_| ApiError::BadRequest("AI reports must cover 1–366 days".into()))?;
        if requested.is_some_and(|previous| previous != days) {
            return Err(ApiError::BadRequest(
                "Ask about one date range at a time; no prompt allowance was consumed".into(),
            ));
        }
        requested = Some(days);
    }
    if let Some(days) = requested {
        plan.period = Period::Rolling;
        plan.start_date = None;
        plan.end_date = None;
        plan.days = days;
        // Preserve an explicitly planned end offset (for example, ending yesterday).
        plan.validate()?;
    }
    Ok(())
}

// Check explicit constraints independently of model instructions. A missing recognized
// filter must fail before ClickHouse, rather than return a broader and misleading total.
pub(super) fn apply_question_constraints(
    question: &str,
    plan: &mut ReportPlan,
) -> Result<(), ApiError> {
    let question = normalize_question(question);
    let contains = |pattern: &str| {
        regex::Regex::new(pattern)
            .expect("static intent regex")
            .is_match(&question)
    };
    let chart_requested = contains(r"(?i)\b(chart|graph|plot|pie|donut|visuali[sz](?:e|ation))\b")
        && !contains(r"(?i)\b(?:no|without)\s+(?:a\s+)?(?:chart|graph|plot)\b|\btext[ -]only\b");
    if !chart_requested || plan.report == Report::Totals {
        plan.chart = Chart::None;
    }
    if !contains(r"(?i)\bpdf\b") {
        plan.output = Output::Text;
    }
    let filters = recognized_filters(&question);
    for (dimension, requested, applied) in [
        ("country", &filters.country, &plan.country),
        ("browser", &filters.browser, &plan.browser),
        ("device", &filters.device, &plan.device),
        ("operating-system", &filters.os, &plan.os),
    ] {
        if let Some(value) = requested.iter().find(|v| !applied.contains(v)) {
            return Err(missing_filter(dimension, value));
        }
    }
    Ok(())
}
#[derive(Default, Serialize)]
pub(super) struct FilterHints {
    country: Vec<String>,
    browser: Vec<String>,
    device: Vec<String>,
    os: Vec<String>,
}
// Hints identify public categories only; the model must still interpret their
// Boolean meaning, and unsupported/ambiguous questions must not become SQL.
pub(super) fn recognized_filters(question: &str) -> FilterHints {
    let question = normalize_question(question);
    let contains = |pattern: &str| {
        regex::Regex::new(pattern)
            .expect("category regex")
            .is_match(&question)
    };
    let mut filters = FilterHints::default();
    for (pattern, value) in [
        (
            r"(?i)\b(?:mac|macos|mac os(?: x|x)?|os x|osx|macintosh|macbook|imac)\b",
            "macOS",
        ),
        (r"(?i)\bwindows\b", "Windows"),
        (r"(?i)\blinux\b", "Linux"),
        (r"(?i)\b(?:ios|iphone|ipad|ipados)\b", "iOS"),
        (r"(?i)\bandroid\b", "Android"),
        (r"(?i)\b(?:chrome os|chromeos|chromebook)\b", "Chrome OS"),
    ] {
        if contains(pattern) {
            filters.os.push(value.into());
        }
    }
    let browser_question = regex::Regex::new(r"(?i)\bchrome os\b")
        .expect("OS name")
        .replace_all(&question, "ChromeOS");
    for name in [
        "Chrome",
        "Firefox",
        "Safari",
        "Edge",
        "Opera",
        "Samsung Internet",
    ] {
        if regex::Regex::new(&format!(r"(?i)\b{}\b", regex::escape(name)))
            .expect("browser name")
            .is_match(&browser_question)
        {
            filters.browser.push(name.into());
        }
    }
    for name in ["desktop", "mobile", "tablet"] {
        if contains(&format!(r"(?i)\b{name}\b")) {
            filters.device.push(name.into());
        }
    }
    for word in question.split(|c: char| !c.is_ascii_alphabetic()) {
        if word.len() == 2
            && word.chars().all(|c| c.is_ascii_uppercase())
            && normalize_country(word).is_some()
            && !filters.country.iter().any(|v| v == word)
        {
            filters.country.push(word.into());
        }
    }
    filters
}
fn missing_filter(dimension: &str, value: &str) -> ApiError {
    ApiError::BadRequest(format!("The {dimension} filter for {value} could not be applied faithfully. Please rephrase with the filters you want included. No prompt allowance was consumed."))
}

// Normalize full country names before planning as well as validating returned filters.
// Do not rewrite ordinary words such as "in" or "us" as country codes.
pub(super) fn normalize_question(question: &str) -> String {
    static NAMES: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        let countries: std::collections::BTreeMap<String, String> =
            serde_json::from_str(include_str!("countries.json")).expect("bundled countries");
        let mut names: Vec<String> = countries.into_values().collect();
        names.extend(
            [
                "Bharat",
                "UAE",
                "U.A.E.",
                "UK",
                "U.K.",
                "Britain",
                "Great Britain",
                "USA",
                "United States of America",
                "South Korea",
                "Russia",
            ]
            .map(String::from),
        );
        names.sort_by_key(|name| std::cmp::Reverse(name.len()));
        regex::Regex::new(&format!(
            r"(?i)\b(?:{})\b",
            names
                .iter()
                .map(|name| regex::escape(name))
                .collect::<Vec<_>>()
                .join("|")
        ))
        .expect("country regex")
    });
    NAMES
        .replace_all(question, |captures: &regex::Captures<'_>| {
            normalize_country(&captures[0]).unwrap_or_else(|| captures[0].to_owned())
        })
        .into_owned()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportRow {
    pub label: String,
    pub events: u64,
    pub pageviews: u64,
    pub visitors: u64,
    pub sessions: u64,
}
#[derive(Debug, Serialize)]
pub(crate) struct Evidence {
    pub report: String,
    pub start_date: String,
    pub end_date: String,
    pub timezone: &'static str,
    pub period_label: String,
    pub groups_suppressed_below_visitors: u32,
    pub country: Vec<String>,
    pub browser: Vec<String>,
    pub device: Vec<String>,
    pub os: Vec<String>,
    pub chart: Chart,
    pub metric: Metric,
    pub output: Output,
    pub rows: Vec<ReportRow>,
    pub details: super::insights::InsightDetails,
}

#[derive(Serialize)]
pub(super) struct AnswerContext {
    pub period: String,
    pub countries: Vec<String>,
    pub browsers: Vec<String>,
    pub devices: Vec<String>,
    pub operating_systems: Vec<String>,
}
impl Evidence {
    pub(super) fn answer_context(&self) -> AnswerContext {
        let countries: std::collections::BTreeMap<String, String> =
            serde_json::from_str(include_str!("countries.json")).expect("bundled countries");
        AnswerContext {
            period: self.period_label.clone(),
            countries: self
                .country
                .iter()
                .map(|code| countries.get(code).unwrap_or(code).clone())
                .collect(),
            browsers: self.browser.clone(),
            devices: self
                .device
                .iter()
                .map(|v| match v.as_str() {
                    "desktop" => "desktop computers".into(),
                    "mobile" => "phones".into(),
                    "tablet" => "tablets".into(),
                    _ => v.clone(),
                })
                .collect(),
            operating_systems: self
                .os
                .iter()
                .map(|v| match v.as_str() {
                    "macOS" => "Mac".into(),
                    "iOS" => "iOS".into(),
                    _ => v.clone(),
                })
                .collect(),
        }
    }
}

const OS_EXPRESSION: &str = "multiIf(lowerUTF8(os_name) IN ('mac','macos','mac os','mac os x','mac osx','os x','osx','macintosh'), 'macOS', startsWith(lowerUTF8(os_name), 'windows'), 'Windows', lowerUTF8(os_name) IN ('ios','iphone','ipad','ipados','ipod'), 'iOS', lowerUTF8(os_name) = 'android', 'Android', lowerUTF8(os_name) IN ('chrome os','chromeos'), 'Chrome OS', lowerUTF8(os_name) = 'linux', 'Linux', 'Other')";
// Values have already passed canonical allowlists; serialization is a bound parameter,
// never interpolated into the SQL text.
pub(super) fn bind_filter(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|v| format!("'{}'", v.replace('\\', "\\\\").replace('\'', "\\'")))
            .collect::<Vec<_>>()
            .join(",")
    )
}

// Only server-authored expressions enter SQL. Filters and tenant scope are bound values.
#[cfg(test)]
fn query(plan: &ReportPlan) -> String {
    compiled_query(plan, &mut Vec::new())
}
fn compiled_query(plan: &ReportPlan, parameters: &mut Vec<(String, String)>) -> String {
    let dimension = match plan.report {
        Report::Daily => "toString(toDate(occurred_at, 'UTC'))",
        Report::Weekly => "toString(toMonday(toDate(occurred_at, 'UTC')))",
        Report::Country => "if(match(country, '^[A-Z]{2}$'), country, 'Unknown')",
        Report::City => "concat(city, ', ', country)",
        Report::Event => "event_name",
        Report::Campaign => "if(utm_campaign = '', 'Unattributed', utm_campaign)",
        Report::Browser => "multiIf(browser_name IN ('Chrome','Firefox','Safari','Edge','Opera','Samsung Internet'), browser_name, 'Other')",
        Report::Os => OS_EXPRESSION,
        Report::Device => "multiIf(lowerUTF8(device_type) = 'desktop', 'Desktop', lowerUTF8(device_type) = 'mobile', 'Mobile', lowerUTF8(device_type) = 'tablet', 'Tablet', 'Other')",
        _ => "'Total'",
    };
    let extra = super::insights::predicate(plan, parameters, plan.report != Report::Funnel);
    let scope = format!("site_id = {{site:String}} AND occurred_at >= toDateTime({{start:Date}}, 'UTC') AND occurred_at < toDateTime({{end:Date}}, 'UTC') + INTERVAL 1 DAY AND occurred_at <= now64(3) AND event_type IN ('pageview','external','rule') AND (empty({{country:Array(String)}}) OR country IN {{country:Array(String)}}) AND (empty({{browser:Array(String)}}) OR browser_name IN {{browser:Array(String)}}) AND (empty({{device:Array(String)}}) OR lowerUTF8(device_type) IN {{device:Array(String)}}) AND (empty({{os:Array(String)}}) OR {OS_EXPRESSION} IN {{os:Array(String)}}) AND {ACTIVE_ROW_PREDICATE} AND ({extra})");
    if plan.report == Report::Funnel {
        return super::insights::funnel_query(plan, &scope, parameters);
    }
    let grouped = plan.report != Report::Totals;
    let suppression =
        plan.filtered() || !matches!(plan.report, Report::Totals | Report::Daily | Report::Weekly);
    let group = if grouped { "GROUP BY label" } else { "" };
    let having = if suppression {
        "HAVING visitors >= 5"
    } else {
        ""
    };
    let order = if matches!(plan.report, Report::Daily | Report::Weekly) {
        "ORDER BY label LIMIT 366"
    } else {
        "ORDER BY events DESC, label LIMIT 20"
    };
    format!("SELECT {dimension} AS label, toUInt64(count()) AS events, toUInt64(countIf(event_type = 'pageview')) AS pageviews, toUInt64(uniqCombined64If(visitor_id, visitor_id != '')) AS visitors, toUInt64(uniqCombined64If(anon_session_id, anon_session_id != '')) AS sessions FROM owleye_events WHERE {scope} {group} {having} {order}")
}

pub(super) async fn execute(
    state: &AppState,
    tracking_id: &str,
    mut plan: ReportPlan,
) -> Result<Evidence, ApiError> {
    plan.normalize()?;
    let (start, end) = plan.range(Utc::now().date_naive());
    let (start, end) = (start.to_string(), end.to_string());
    let mut extra_parameters = Vec::new();
    let sql = compiled_query(&plan, &mut extra_parameters);
    let country = bind_filter(&plan.country);
    let browser = bind_filter(&plan.browser);
    let device = bind_filter(&plan.device);
    let os = bind_filter(&plan.os);
    let mut parameters = vec![
        ("param_site", tracking_id),
        ("param_start", &start),
        ("param_end", &end),
        ("param_country", &country),
        ("param_browser", &browser),
        ("param_device", &device),
        ("param_os", &os),
    ];
    parameters.extend(
        extra_parameters
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str())),
    );
    let rows = state
        .clickhouse
        .query_ai_report(
            &sql,
            &parameters,
            state.settings.ai.clickhouse_url.as_deref(),
        )
        .await
        .map_err(|error| {
            #[cfg(test)]
            eprintln!("AI report test failure: {error:#}");
            #[cfg(not(test))]
            let _ = error;
            ApiError::ServiceUnavailable(
                "Analytics could not be queried; no prompt allowance was consumed".into(),
            )
        })?;
    let threshold = if plan.filtered()
        || !matches!(plan.report, Report::Totals | Report::Daily | Report::Weekly)
    {
        5
    } else {
        0
    };
    let chart = if plan.report == Report::Totals || rows.len() < 2 {
        Chart::None
    } else if plan.chart == Chart::Auto {
        match plan.report {
            Report::Daily | Report::Weekly => Chart::Line,
            Report::Totals => Chart::None,
            _ => Chart::Bar,
        }
    } else {
        plan.chart
    };
    let mut details = super::insights::details(&plan);
    details.notes.push("Only retained, active analytics is included. Missing or expired history cannot be reconstructed; these results do not prove complete coverage of the requested dates.".into());
    Ok(Evidence {
        report: serde_json::to_value(plan.report)
            .expect("enum")
            .as_str()
            .expect("string enum")
            .into(),
        period_label: plan.period_label(&start, &end),
        start_date: start,
        end_date: end,
        timezone: "UTC",
        groups_suppressed_below_visitors: threshold,
        country: plan.country,
        browser: plan.browser,
        device: plan.device,
        os: plan.os,
        chart,
        metric: plan.metric,
        output: plan.output,
        rows,
        details,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_sql_scope_overrides_and_invalid_ranges() {
        for value in [
            r#"{"report":"totals","days":7,"offset_days":0,"site_id":"other"}"#,
            r#"{"report":"totals","days":7,"offset_days":0,"sql":"SELECT * FROM users"}"#,
            r#"{"report":"url_path","days":7,"offset_days":0}"#,
        ] {
            assert!(serde_json::from_str::<ReportPlan>(value).is_err());
        }
        for (days, offset_days) in [(0, 0), (367, 0), (1, 36501), (1, u32::MAX)] {
            assert!(ReportPlan {
                days,
                offset_days,
                ..Default::default()
            }
            .validate()
            .is_err());
        }
        for report in [
            Report::Totals,
            Report::Daily,
            Report::Weekly,
            Report::Country,
            Report::Browser,
            Report::Device,
            Report::Os,
        ] {
            let sql = query(&ReportPlan {
                report,
                ..Default::default()
            });
            assert!(sql.contains("site_id = {site:String}"));
            assert!(sql.contains(ACTIVE_ROW_PREDICATE));
            assert!(!sql.contains("SELECT *"));
        }
    }
    #[test]
    fn normalizes_country_names_codes_and_aliases_and_rejects_injection() {
        for name in ["India", "india", "IN", "in", " Bharat "] {
            assert_eq!(normalize_country(name).as_deref(), Some("IN"));
        }
        assert_eq!(
            normalize_question("How many users in India visited this week?"),
            "How many users in IN visited this week?"
        );
        assert_eq!(
            normalize_question("Show us visits from United States of America and Bharat"),
            "Show us visits from US and IN"
        );
        assert_eq!(
            normalize_question("linear trends in browsers"),
            "linear trends in browsers"
        );
        assert_eq!(normalize_country("UK").as_deref(), Some("GB"));
        assert_eq!(normalize_country("United States").as_deref(), Some("US"));
        assert!(normalize_country("IN' OR 1=1").is_none());
        let mut plan = ReportPlan {
            days: 7,
            country: vec!["India".into()],
            browser: vec!["chrome".into()],
            device: vec!["Mobile".into()],
            ..Default::default()
        };
        plan.normalize().unwrap();
        assert_eq!(plan.country, vec!["IN"]);
        assert_eq!(plan.browser, vec!["Chrome"]);
        assert_eq!(plan.device, vec!["mobile"]);
        assert!(query(&plan).contains("HAVING visitors >= 5"));
        plan.device = vec!["anything' OR true".into()];
        assert!(plan.normalize().is_err());
    }
    #[test]
    fn explicit_day_ranges_override_calendar_hallucinations_and_reject_out_of_bounds() {
        let mut plan = ReportPlan {
            days: 30,
            period: Period::LastMonth,
            ..Default::default()
        };
        apply_explicit_day_range("Weekly visits from India over the last 30 days", &mut plan)
            .unwrap();
        assert_eq!(plan.period, Period::Rolling);
        assert_eq!(plan.days, 30);
        assert!(apply_explicit_day_range("past 365 days", &mut plan).is_ok());
        assert!(apply_explicit_day_range("past 367 days", &mut plan).is_err());
        assert!(apply_explicit_day_range("past 0 days", &mut plan).is_err());
        assert!(apply_explicit_day_range("last 7 days versus last 30 days", &mut plan).is_err());
        let mut calendar = ReportPlan {
            days: 30,
            period: Period::LastMonth,
            ..Default::default()
        };
        apply_explicit_day_range("traffic last month", &mut calendar).unwrap();
        assert_eq!(calendar.period, Period::LastMonth);
    }
    #[test]
    fn combined_filters_are_canonical_and_missing_filters_fail_closed() {
        let mut plan = ReportPlan {
            days: 3,
            country: vec!["India".into()],
            browser: vec!["chrome".into(), "Safari".into()],
            os: vec!["Mac OSX".into()],
            chart: Chart::Pie,
            output: Output::Pdf,
            ..Default::default()
        };
        plan.normalize().unwrap();
        let question = "Total users from India in last 3 days using Mac and Chrome or Safari";
        apply_question_constraints(question, &mut plan).unwrap();
        assert_eq!(plan.os, ["macOS"]);
        assert_eq!(plan.chart, Chart::None);
        assert_eq!(plan.output, Output::Text);
        assert!(query(&plan).contains("browser_name IN {browser:Array(String)}"));
        assert!(query(&plan).contains("os_name"));
        assert_eq!(bind_filter(&plan.browser), "['Chrome','Safari']");
        plan.os.clear();
        assert!(apply_question_constraints(question, &mut plan).is_err());
        plan.os.push("macOS".into());
        plan.browser.pop();
        assert!(apply_question_constraints(question, &mut plan).is_err());
        plan.os = vec!["macOS' OR 1=1".into()];
        assert!(plan.normalize().is_err());
        plan.os = vec!["macOS".into(); 11];
        assert!(plan.normalize().is_err());
        let mut chrome_os = ReportPlan {
            days: 3,
            os: vec!["Chrome OS".into()],
            ..Default::default()
        };
        apply_question_constraints("Users using Chrome OS", &mut chrome_os).unwrap();
    }
    #[test]
    fn presentations_are_opt_in_and_totals_never_become_charts() {
        for question in [
            "Daily visits",
            "Users by browser",
            "Daily visits without a chart",
            "Text-only daily visits",
        ] {
            let mut plan = ReportPlan {
                days: 7,
                report: Report::Daily,
                chart: Chart::Line,
                output: Output::Pdf,
                ..Default::default()
            };
            apply_question_constraints(question, &mut plan).unwrap();
            assert_eq!(plan.chart, Chart::None);
            assert_eq!(plan.output, Output::Text);
        }
        let mut plan = ReportPlan {
            days: 7,
            report: Report::Daily,
            chart: Chart::Line,
            output: Output::Pdf,
            ..Default::default()
        };
        apply_question_constraints("PDF with a chart of daily visits", &mut plan).unwrap();
        assert_eq!(plan.chart, Chart::Line);
        assert_eq!(plan.output, Output::Pdf);
        plan.report = Report::Totals;
        apply_question_constraints("Pie chart of total users", &mut plan).unwrap();
        assert_eq!(plan.chart, Chart::None);
    }
    #[test]
    fn friendly_period_labels_do_not_broaden_shifted_or_partial_ranges() {
        let mut plan = ReportPlan {
            days: 14,
            ..Default::default()
        };
        assert_eq!(
            plan.period_label("2026-09-07", "2026-09-20"),
            "last 14 days"
        );
        plan.offset_days = 1;
        assert_eq!(
            plan.period_label("2026-09-06", "2026-09-19"),
            "14 days ending yesterday"
        );
        plan.offset_days = 2;
        assert_eq!(
            plan.period_label("2026-09-05", "2026-09-18"),
            "2026-09-05 to 2026-09-18"
        );
        plan.period = Period::ThisWeek;
        assert_eq!(
            plan.period_label("2026-09-14", "2026-09-20"),
            "this week so far"
        );
    }
    #[test]
    fn calendar_periods_and_weekly_buckets_are_explicit() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 19).unwrap();
        for (period, start, end) in [
            (Period::ThisWeek, "2026-09-14", "2026-09-19"),
            (Period::LastWeek, "2026-09-07", "2026-09-13"),
            (Period::ThisMonth, "2026-09-01", "2026-09-19"),
            (Period::LastMonth, "2026-08-01", "2026-08-31"),
        ] {
            let plan = ReportPlan {
                period,
                ..Default::default()
            };
            let (a, b) = plan.range(today);
            assert_eq!((a.to_string(), b.to_string()), (start.into(), end.into()));
        }
        assert!(query(&ReportPlan {
            report: Report::Weekly,
            ..Default::default()
        })
        .contains("toMonday"));
    }
}
