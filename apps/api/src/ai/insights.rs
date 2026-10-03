//! Bounded semantic filters and aggregate-only discovery. No model-authored SQL.
use super::report::{bind_filter, Report, ReportPlan};
use crate::{retention::ACTIVE_ROW_PREDICATE, ApiError, AppState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CityFilter {
    pub city: String,
    pub country: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PropertyFilter {
    pub key: String,
    pub value: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunnelStep {
    pub label: String,
    pub events: Vec<String>,
    pub properties: Vec<PropertyFilter>,
}
#[derive(Debug, Default, Serialize)]
pub(crate) struct InsightDetails {
    pub cities: Vec<String>,
    pub events: Vec<String>,
    pub campaigns: Vec<String>,
    pub properties: Vec<String>,
    pub steps: Vec<String>,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct CatalogEntry {
    pub kind: String,
    pub name: String,
    pub qualifier: String,
}
#[derive(Debug, Default, Serialize)]
pub(super) struct Catalog {
    pub entries: Vec<CatalogEntry>,
    pub truncated: bool,
}

pub(super) fn safe_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 120
        && !value.chars().any(char::is_control)
        && super::sanitize::redact(value) == value
}
fn safe_property(key: &str) -> bool {
    let compact: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    safe_name(key)
        && ![
            "email",
            "phone",
            "password",
            "secret",
            "token",
            "address",
            "fullname",
            "firstname",
            "lastname",
            "userid",
            "customerid",
            "sessionid",
            "ipaddress",
        ]
        .iter()
        .any(|s| compact.contains(s))
}
pub(super) fn normalize(plan: &mut ReportPlan) -> Result<(), ApiError> {
    let invalid = || {
        ApiError::BadRequest("Use up to 10 locations, events or campaigns and up to 3 non-personal property filters. No prompt allowance was consumed.".into())
    };
    if plan.cities.len() > 10
        || plan.events.len() > 10
        || plan.campaigns.len() > 10
        || plan.properties.len() > 3
        || plan.steps.len() > 3
    {
        return Err(invalid());
    }
    for location in &mut plan.cities {
        if !safe_name(&location.city) {
            return Err(invalid());
        }
        location.city = location.city.trim().to_owned();
        location.country =
            super::report::normalize_country(&location.country).ok_or_else(invalid)?;
    }
    for name in plan.events.iter().chain(&plan.campaigns) {
        if !safe_name(name) {
            return Err(invalid());
        }
    }
    for property in plan
        .properties
        .iter()
        .chain(plan.steps.iter().flat_map(|s| &s.properties))
    {
        if !safe_property(&property.key) || !safe_name(&property.value) {
            return Err(invalid());
        }
    }
    for step in &plan.steps {
        if !safe_name(&step.label)
            || step.events.is_empty()
            || step.events.len() > 10
            || step.properties.len() > 3
            || step.events.iter().any(|n| !safe_name(n))
        {
            return Err(invalid());
        }
    }
    if plan.report == Report::Funnel
        && (plan.steps.len() < 2 || !plan.events.is_empty() || !plan.properties.is_empty())
    {
        return Err(ApiError::BadRequest("A conversion funnel needs two or three ordered event steps, with property filters attached to each step.".into()));
    }
    if plan.report == Report::Funnel {
        plan.metric = super::report::Metric::Visitors;
        plan.chart = super::report::Chart::None;
    }
    if plan.report != Report::Funnel && !plan.steps.is_empty() {
        return Err(invalid());
    }
    Ok(())
}

// Expand category names after a single tenant/retention-filtered scan. Separate
// UNION branches reread visitor IDs and history for every category and can exhaust
// the shared read budget even when the resulting catalog is tiny.
fn catalog_query() -> String {
    format!(
        r#"
        SELECT kind,
            if(kind = 'coverage', toString(toDate(first_seen, 'UTC')), name) AS name,
            if(kind = 'coverage', toString(toDate(last_seen, 'UTC')), qualifier) AS qualifier
        FROM (
            SELECT category.1 AS kind, category.2 AS name, category.3 AS qualifier,
                min(occurred_at) AS first_seen, max(occurred_at) AS last_seen,
                row_number() OVER (
                    PARTITION BY kind ORDER BY count() DESC, name, qualifier
                ) AS position
            FROM (
                SELECT occurred_at, visitor_id,
                    arrayJoin(arrayConcat(
                        [('coverage', '', '')],
                        if(event_type IN ('external', 'rule', 'performance', 'page_session'), [('event', event_name, '')], []),
                        if(city != '' AND match(country, '^[A-Z]{{2}}$'), [('city', city, country)], []),
                        if(utm_campaign != '', [('campaign', utm_campaign, '')], []),
                        if(event_type IN ('external', 'rule', 'performance', 'page_session'),
                            arrayMap(key -> ('property', key, event_name), JSONExtractKeys(payload_json, 'records')), [])
                    )) AS category
                FROM owleye_events
                WHERE site_id = {{site:String}} AND occurred_at <= now64(3)
                    AND event_type IN ('pageview', 'external', 'rule', 'performance', 'page_session')
                    AND {ACTIVE_ROW_PREDICATE}
            )
            GROUP BY kind, name, qualifier
            HAVING uniqCombined64If(visitor_id, visitor_id != '') >= 5
        )
        WHERE position <= multiIf(kind IN ('event', 'city'), 30, kind IN ('campaign', 'property'), 20, 1)
        ORDER BY kind, position
    "#
    )
}

pub(super) async fn discover(state: &AppState, site: &str) -> Result<Catalog, ApiError> {
    // Only names and property KEYS leave ClickHouse, never property values or identifiers.
    // Low-volume categories are withheld; a missing entry is not proof of absence.
    let rows: Vec<CatalogEntry> = state.clickhouse.query_scoped_ai_report(&catalog_query(), &[("param_site", site)], state.settings.ai.clickhouse_url.as_deref()).await.map_err(|_| ApiError::ServiceUnavailable("The app's analytics catalog could not be loaded; no prompt allowance was consumed.".into()))?;
    let truncated = rows.iter().filter(|r| r.kind != "coverage").count() >= 100;
    Ok(Catalog {
        entries: rows
            .into_iter()
            .filter(|r| {
                safe_name(&r.name)
                    && (r.qualifier.is_empty() || safe_name(&r.qualifier))
                    && (r.kind != "property" || safe_property(&r.name))
            })
            .collect(),
        truncated,
    })
}

pub(super) fn validate_catalog(plan: &ReportPlan, catalog: &Catalog) -> Option<String> {
    for name in plan
        .events
        .iter()
        .chain(plan.steps.iter().flat_map(|s| &s.events))
    {
        if !catalog
            .entries
            .iter()
            .any(|e| e.kind == "event" && e.name == *name)
        {
            return Some(format!("Which recorded event represents {name}? I couldn't verify that event in this app's available catalog. Rare events may be hidden by the privacy threshold."));
        }
    }
    for (properties, events) in std::iter::once((&plan.properties, &plan.events))
        .chain(plan.steps.iter().map(|s| (&s.properties, &s.events)))
    {
        for property in properties {
            if !catalog.entries.iter().any(|e| {
                e.kind == "property"
                    && e.name == property.key
                    && (events.is_empty() || events.contains(&e.qualifier))
            }) {
                return Some(format!("Which property should identify this condition? I couldn't verify the property {} on the selected events.", property.key));
            }
        }
    }
    None
}

fn property_predicates(
    properties: &[PropertyFilter],
    prefix: &str,
    params: &mut Vec<(String, String)>,
) -> Vec<String> {
    properties.iter().enumerate().map(|(i,p)| {
        let key = format!("{prefix}_key_{i}"); let value = format!("{prefix}_value_{i}");
        params.push((format!("param_{key}"),p.key.clone())); params.push((format!("param_{value}"),p.value.clone()));
        format!("JSONHas(payload_json, 'records', {{{key}:String}}) AND if(JSONType(payload_json, 'records', {{{key}:String}}) = 'String', JSONExtractString(payload_json, 'records', {{{key}:String}}), JSONExtractRaw(payload_json, 'records', {{{key}:String}})) = {{{value}:String}}")
    }).collect()
}
pub(super) fn predicate(
    plan: &ReportPlan,
    params: &mut Vec<(String, String)>,
    event_filters: bool,
) -> String {
    let mut terms = Vec::new();
    if !plan.cities.is_empty() {
        let mut cities = Vec::new();
        for (i, location) in plan.cities.iter().enumerate() {
            params.push((format!("param_city_{i}"), location.city.clone()));
            params.push((format!("param_city_country_{i}"), location.country.clone()));
            cities.push(format!("(lowerUTF8(city) = lowerUTF8({{city_{i}:String}}) AND country = {{city_country_{i}:String}})"));
        }
        terms.push(format!("({})", cities.join(" OR ")));
    }
    if event_filters {
        if !plan.events.is_empty() {
            params.push(("param_events".into(), bind_filter(&plan.events)));
            terms.push("event_name IN {events:Array(String)}".into());
        }
        if !plan.campaigns.is_empty() {
            params.push(("param_campaigns".into(), bind_filter(&plan.campaigns)));
            terms.push("utm_campaign IN {campaigns:Array(String)}".into());
        }
        terms.extend(property_predicates(&plan.properties, "prop", params));
    }
    if terms.is_empty() {
        "1".into()
    } else {
        terms.join(" AND ")
    }
}

pub(super) fn funnel_query(
    plan: &ReportPlan,
    base_scope: &str,
    params: &mut Vec<(String, String)>,
) -> String {
    let source = if plan.campaigns.is_empty() {
        "SELECT * FROM scoped".to_owned()
    } else {
        params.push(("param_campaigns".into(), bind_filter(&plan.campaigns)));
        "SELECT scoped.* FROM scoped INNER JOIN (SELECT visitor_id, min(occurred_at) AS first_campaign FROM scoped WHERE utm_campaign IN {campaigns:Array(String)} GROUP BY visitor_id) AS cohort USING (visitor_id) WHERE occurred_at >= first_campaign".into()
    };
    let conditions: Vec<String> = plan
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let key = format!("step_{i}");
            params.push((format!("param_{key}"), bind_filter(&step.events)));
            let mut terms = vec![format!("event_name IN {{{key}:Array(String)}}")];
            terms.extend(property_predicates(&step.properties, &key, params));
            terms.join(" AND ")
        })
        .collect();
    let mut labels: Vec<String> = plan.steps.iter().map(|s| s.label.clone()).collect();
    if !plan.campaigns.is_empty() {
        labels.insert(0, "Campaign audience".into());
    }
    params.push(("param_labels".into(), bind_filter(&labels)));
    let first = if plan.campaigns.is_empty() { 1 } else { 0 };
    let n = plan.steps.len() + 1;
    let label_offset = if first == 0 { 1 } else { 0 };
    // Millisecond timestamps, strict ordering, within the requested period. Repeat events
    // do not become extra customers. Visitor identity never leaves this query.
    format!("WITH scoped AS (SELECT visitor_id, occurred_at, event_name, payload_json, utm_campaign FROM owleye_events WHERE {base_scope} AND visitor_id != ''), levels AS (SELECT visitor_id, windowFunnel(31622400000, 'strict_increase')(toUInt64(toUnixTimestamp64Milli(occurred_at)), {}) AS level FROM ({source}) GROUP BY visitor_id) SELECT {{labels:Array(String)}}[stage + {label_offset}] AS label, toUInt64(0) AS events, toUInt64(0) AS pageviews, toUInt64(countIf(level >= stage)) AS visitors, toUInt64(0) AS sessions FROM levels CROSS JOIN (SELECT arrayJoin(range({first}, {n})) AS stage) AS stages GROUP BY stage HAVING visitors >= 5 ORDER BY stage", conditions.join(", "))
}

pub(super) fn details(plan: &ReportPlan) -> InsightDetails {
    InsightDetails {
        cities: plan
            .cities
            .iter()
            .map(|c| format!("{}, {}", c.city, c.country))
            .collect(),
        events: plan.events.clone(),
        campaigns: plan.campaigns.clone(),
        properties: plan
            .properties
            .iter()
            .map(|p| format!("{} = {}", p.key, p.value))
            .collect(),
        steps: plan
            .steps
            .iter()
            .map(|s| {
                format!(
                    "{}: {}{}",
                    s.label,
                    s.events.join(" or "),
                    s.properties
                        .iter()
                        .map(|p| format!("; {} = {}", p.key, p.value))
                        .collect::<String>()
                )
            })
            .collect(),
        notes: if plan.report == Report::Funnel {
            vec!["Conversions count anonymous visitors completing the selected events in timestamp order within the requested dates. This is observed attribution, not proof the campaign caused a purchase or was profitable.".into(), "Campaign attribution starts at the first matching campaign event in the range. Later steps do not need to repeat UTM tags. Anonymous visitor matching can merge or split people and cannot reliably link devices.".into(), "Stages below five visitors are suppressed, not zero. Retention gaps can undercount conversion paths.".into()]
        } else {
            vec![]
        },
    }
}
