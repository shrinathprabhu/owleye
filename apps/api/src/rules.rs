use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap},
    Json,
};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    auth,
    errors::ApiError,
    models::{clickhouse_string, normalize_site_id},
    retention::ACTIVE_ROW_PREDICATE,
    sites,
    sites::ActiveSite,
    AppState,
};

const MAX_RULE_NAME_BYTES: usize = 120;
const MAX_RULE_DESCRIPTION_BYTES: usize = 1_000;
const MAX_RULE_SELECTOR_BYTES: usize = 2_048;
const MAX_RULE_CONFIG_BYTES: usize = 8 * 1_024;

#[derive(Debug, Deserialize)]
pub struct RulesQuery {
    site_id: String,
}

#[derive(Debug, Serialize)]
pub struct TrackingRule {
    capture_text: bool,
    id: String,
    metadata: Value,
    name: String,
    sample_rate: f64,
    selector: String,
    trigger_config: Value,
    #[serde(rename = "type")]
    rule_type: String,
}

#[derive(Debug, sqlx::FromRow)]
struct TrackingRuleRow {
    capture_text: bool,
    id: String,
    metadata_json: String,
    name: String,
    sample_rate: f64,
    selector: String,
    trigger_config_json: String,
    rule_type: String,
}

impl TrackingRuleRow {
    fn into_rule(self) -> Result<TrackingRule, ApiError> {
        Ok(TrackingRule {
            capture_text: self.capture_text,
            id: self.id,
            metadata: decode_stored_object(&self.metadata_json, "metadata_json")?,
            name: self.name,
            sample_rate: self.sample_rate,
            selector: self.selector,
            trigger_config: decode_stored_object(&self.trigger_config_json, "trigger_config_json")?,
            rule_type: self.rule_type,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct RulesResponse {
    rules: Vec<TrackingRule>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ManagedRule {
    id: String,
    site_id: String,
    name: String,
    description: Option<String>,
    selector: String,
    #[serde(rename = "type")]
    rule_type: String,
    capture_text: bool,
    enabled: bool,
    lifetime_event_count: u64,
    sample_rate: f64,
    trigger_config: Value,
    metadata: Value,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ManagedRuleRow {
    id: String,
    site_id: String,
    name: String,
    description: Option<String>,
    selector: String,
    rule_type: String,
    capture_text: bool,
    enabled: bool,
    sample_rate: f64,
    trigger_config_json: String,
    metadata_json: String,
    created_at: String,
    updated_at: String,
}

impl ManagedRuleRow {
    fn into_rule(self) -> Result<ManagedRule, ApiError> {
        Ok(ManagedRule {
            id: self.id,
            site_id: self.site_id,
            name: self.name,
            description: self.description,
            selector: self.selector,
            rule_type: self.rule_type,
            capture_text: self.capture_text,
            enabled: self.enabled,
            lifetime_event_count: 0,
            sample_rate: self.sample_rate,
            trigger_config: decode_stored_object(&self.trigger_config_json, "trigger_config_json")?,
            metadata: decode_stored_object(&self.metadata_json, "metadata_json")?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateRuleRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    selector: String,
    #[serde(rename = "type")]
    rule_type: String,
    #[serde(default)]
    capture_text: bool,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_sample_rate")]
    sample_rate: f64,
    #[serde(default = "empty_object")]
    trigger_config: Value,
    #[serde(default = "empty_object")]
    metadata: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateRuleRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    selector: Option<String>,
    #[serde(default, rename = "type")]
    rule_type: Option<String>,
    #[serde(default)]
    capture_text: Option<bool>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    sample_rate: Option<f64>,
    #[serde(default)]
    trigger_config: Option<Value>,
    #[serde(default)]
    metadata: Option<Value>,
}

pub async fn list_rules(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RulesQuery>,
) -> Result<Json<RulesResponse>, ApiError> {
    let identifier = normalize_site_id(&query.site_id)?;
    let site = sites::resolve_active_site(&state.sqlite, &identifier).await?;
    let source = rule_request_source(&headers)?;
    if !sites::origin_is_allowed(
        &state.sqlite,
        &site.id,
        source,
        state.settings.allow_loopback_origins,
    )
    .await?
    {
        return Err(ApiError::Forbidden(
            "Origin is not allowed for this site".to_owned(),
        ));
    }

    Ok(Json(RulesResponse {
        rules: load_public_rules(&state.sqlite, &site).await?,
    }))
}

pub(crate) async fn list_site_rules(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Json<Vec<ManagedRule>>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        let site = crate::demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Ok(Json(demo_rules(site)));
    }
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    let mut rules = load_managed_rules(&state.sqlite, &site).await?;
    attach_lifetime_counts(&state, &site, &mut rules).await?;
    Ok(Json(rules))
}

pub(crate) async fn get_site_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, rule_id)): Path<(String, String)>,
) -> Result<Json<ManagedRule>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        let site = crate::demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return demo_rules(site)
            .into_iter()
            .find(|rule| rule.id == rule_id)
            .map(Json)
            .ok_or_else(|| ApiError::BadRequest("Unknown tracking rule".to_owned()));
    }
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    let mut rule = load_managed_rule(&state.sqlite, &site, &rule_id).await?;
    attach_lifetime_counts(&state, &site, std::slice::from_mut(&mut rule)).await?;
    Ok(Json(rule))
}

pub(crate) async fn create_site_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<CreateRuleRequest>,
) -> Result<Json<ManagedRule>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    let rule = validated_create_rule(request)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    sqlx::query(
        r#"
        INSERT INTO tracking_rules (
            id,
            site_id,
            name,
            description,
            selector,
            type,
            capture_text,
            enabled,
            sample_rate,
            trigger_config_json,
            metadata_json,
            created_by_user_id,
            created_at,
            updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&site.id)
    .bind(rule.name)
    .bind(rule.description)
    .bind(rule.selector)
    .bind(rule.rule_type)
    .bind(rule.capture_text)
    .bind(rule.enabled)
    .bind(rule.sample_rate)
    .bind(serde_json::to_string(&rule.trigger_config)?)
    .bind(serde_json::to_string(&rule.metadata)?)
    .bind(session.id)
    .bind(&now)
    .bind(&now)
    .execute(&state.sqlite)
    .await?;

    let mut created = load_managed_rule(&state.sqlite, &site, &id).await?;
    attach_lifetime_counts(&state, &site, std::slice::from_mut(&mut created)).await?;
    Ok(Json(created))
}

pub(crate) async fn update_site_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, rule_id)): Path<(String, String)>,
    Json(request): Json<UpdateRuleRequest>,
) -> Result<Json<ManagedRule>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    if request.is_empty() {
        return Err(ApiError::BadRequest(
            "At least one rule field must be provided".to_owned(),
        ));
    }
    let existing = load_managed_rule(&state.sqlite, &site, &rule_id).await?;
    let updated = merge_rule(existing, request)?;
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    let result = sqlx::query(
        r#"
        UPDATE tracking_rules
        SET
            name = ?,
            description = ?,
            selector = ?,
            type = ?,
            capture_text = ?,
            enabled = ?,
            sample_rate = ?,
            trigger_config_json = ?,
            metadata_json = ?,
            updated_at = ?
        WHERE id = ? AND site_id IN (?, ?)
        "#,
    )
    .bind(updated.name)
    .bind(updated.description)
    .bind(updated.selector)
    .bind(updated.rule_type)
    .bind(updated.capture_text)
    .bind(updated.enabled)
    .bind(updated.sample_rate)
    .bind(serde_json::to_string(&updated.trigger_config)?)
    .bind(serde_json::to_string(&updated.metadata)?)
    .bind(now)
    .bind(&rule_id)
    .bind(&site.id)
    .bind(&site.tracking_id)
    .execute(&state.sqlite)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest("Unknown tracking rule".to_owned()));
    }

    let mut changed = load_managed_rule(&state.sqlite, &site, &rule_id).await?;
    attach_lifetime_counts(&state, &site, std::slice::from_mut(&mut changed)).await?;
    Ok(Json(changed))
}

#[derive(Debug, Deserialize)]
struct RuleCountRow {
    count: u64,
    rule_id: String,
}

async fn attach_lifetime_counts(
    state: &AppState,
    site: &ActiveSite,
    rules: &mut [ManagedRule],
) -> Result<(), ApiError> {
    if rules.is_empty() {
        return Ok(());
    }
    let site_id = clickhouse_string(&site.tracking_id);
    let sql = format!(
        r#"
        SELECT rule_id, toUInt64(count()) AS count
        FROM owleye_events
        WHERE site_id = {site_id} AND event_type = 'rule' AND rule_id != ''
          AND {ACTIVE_ROW_PREDICATE}
        GROUP BY rule_id
        "#
    );
    let counts = state
        .clickhouse
        .query_json_each_row::<RuleCountRow>(&sql)
        .await?;
    for rule in rules {
        rule.lifetime_event_count = counts
            .iter()
            .find(|row| row.rule_id == rule.id)
            .map(|row| row.count)
            .unwrap_or_default();
    }
    Ok(())
}

pub(crate) async fn delete_site_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, rule_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    sqlx::query("DELETE FROM tracking_rules WHERE id = ? AND site_id IN (?, ?)")
        .bind(rule_id)
        .bind(site.id)
        .bind(site.tracking_id)
        .execute(&state.sqlite)
        .await?;
    Ok(Json(json!({ "status": "deleted" })))
}

fn demo_rules(site: &crate::demo::DemoSiteDefinition) -> Vec<ManagedRule> {
    use crate::demo::DemoTrafficProfile;

    let scale = match site.profile {
        DemoTrafficProfile::Massive => 284_310,
        DemoTrafficProfile::Campaign => 91_720,
        DemoTrafficProfile::Mixed => 44_105,
        DemoTrafficProfile::LowVolume => 1_382,
        DemoTrafficProfile::New => 137,
    };
    let created_at = (Utc::now() - chrono::Duration::days(site.created_days_ago.min(120)))
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    vec![
        ManagedRule {
            id: format!("rule_signup_{}", site.id),
            site_id: site.id.to_owned(),
            name: "signup_clicked".to_owned(),
            description: Some("Track the main signup call to action.".to_owned()),
            selector: "signup".to_owned(),
            rule_type: "click".to_owned(),
            capture_text: false,
            enabled: true,
            lifetime_event_count: scale,
            sample_rate: 1.0,
            trigger_config: json!({
                "selector_type": "id",
                "dom_event": "click",
                "page_path": "/pricing"
            }),
            metadata: json!({ "event_name": "signup_clicked" }),
            created_at: created_at.clone(),
            updated_at: updated_at.clone(),
        },
        ManagedRule {
            id: format!("rule_plan_{}", site.id),
            site_id: site.id.to_owned(),
            name: "plan_compared".to_owned(),
            description: Some("Track plan-card interactions and the selected plan.".to_owned()),
            selector: ".plan-card".to_owned(),
            rule_type: "click".to_owned(),
            capture_text: false,
            enabled: true,
            lifetime_event_count: scale / 3,
            sample_rate: 1.0,
            trigger_config: json!({
                "selector_type": "css",
                "dom_event": "click"
            }),
            metadata: json!({
                "event_name": "plan_compared",
                "custom_props": [{
                    "key": "plan",
                    "selector": "[data-plan]",
                    "selector_type": "css"
                }]
            }),
            created_at: created_at.clone(),
            updated_at: updated_at.clone(),
        },
        ManagedRule {
            id: format!("rule_search_{}", site.id),
            site_id: site.id.to_owned(),
            name: "search_settled".to_owned(),
            description: Some("Track a search after typing settles for 800ms.".to_owned()),
            selector: "[data-search]".to_owned(),
            rule_type: "click".to_owned(),
            capture_text: false,
            enabled: true,
            lifetime_event_count: scale / 7,
            sample_rate: 1.0,
            trigger_config: json!({
                "selector_type": "css",
                "dom_event": "keyup",
                "key_condition": "debounce",
                "condition_value": 0.8
            }),
            metadata: json!({
                "event_name": "search_settled",
                "custom_props": [{
                    "key": "query",
                    "selector": "[data-search]",
                    "selector_type": "css"
                }]
            }),
            created_at,
            updated_at,
        },
    ]
}

#[derive(Debug)]
struct ValidatedRule {
    name: String,
    description: Option<String>,
    selector: String,
    rule_type: String,
    capture_text: bool,
    enabled: bool,
    sample_rate: f64,
    trigger_config: Value,
    metadata: Value,
}

fn validated_create_rule(request: CreateRuleRequest) -> Result<ValidatedRule, ApiError> {
    let name = normalize_rule_name(&request.name)?;
    let (trigger_config, metadata) =
        normalize_rule_payload(request.trigger_config, request.metadata, &name)?;
    Ok(ValidatedRule {
        name,
        description: normalize_description(request.description.as_deref())?,
        selector: normalize_selector(&request.selector)?,
        rule_type: normalize_rule_type(&request.rule_type)?,
        capture_text: request.capture_text,
        enabled: request.enabled,
        sample_rate: normalize_sample_rate(request.sample_rate)?,
        trigger_config,
        metadata,
    })
}

fn merge_rule(
    existing: ManagedRule,
    request: UpdateRuleRequest,
) -> Result<ValidatedRule, ApiError> {
    let name = request
        .name
        .as_deref()
        .map(normalize_rule_name)
        .transpose()?
        .unwrap_or(existing.name);
    let (trigger_config, metadata) = normalize_rule_payload(
        request.trigger_config.unwrap_or(existing.trigger_config),
        request.metadata.unwrap_or(existing.metadata),
        &name,
    )?;
    Ok(ValidatedRule {
        name,
        description: request
            .description
            .as_deref()
            .map(|description| normalize_description(Some(description)))
            .transpose()?
            .unwrap_or(existing.description),
        selector: request
            .selector
            .as_deref()
            .map(normalize_selector)
            .transpose()?
            .unwrap_or(existing.selector),
        rule_type: request
            .rule_type
            .as_deref()
            .map(normalize_rule_type)
            .transpose()?
            .unwrap_or(existing.rule_type),
        capture_text: request.capture_text.unwrap_or(existing.capture_text),
        enabled: request.enabled.unwrap_or(existing.enabled),
        sample_rate: request
            .sample_rate
            .map(normalize_sample_rate)
            .transpose()?
            .unwrap_or(existing.sample_rate),
        trigger_config,
        metadata,
    })
}

impl UpdateRuleRequest {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.selector.is_none()
            && self.rule_type.is_none()
            && self.capture_text.is_none()
            && self.enabled.is_none()
            && self.sample_rate.is_none()
            && self.trigger_config.is_none()
            && self.metadata.is_none()
    }
}

async fn load_public_rules(
    pool: &SqlitePool,
    site: &ActiveSite,
) -> Result<Vec<TrackingRule>, ApiError> {
    let rows = sqlx::query_as::<_, TrackingRuleRow>(
        r#"
        SELECT
            id,
            name,
            selector,
            type AS rule_type,
            capture_text,
            COALESCE(sample_rate, 1.0) AS sample_rate,
            trigger_config_json,
            metadata_json
        FROM tracking_rules
        WHERE site_id IN (?, ?) AND enabled = 1
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(&site.id)
    .bind(&site.tracking_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(TrackingRuleRow::into_rule)
        .collect::<Result<Vec<_>, _>>()
}

async fn load_managed_rules(
    pool: &SqlitePool,
    site: &ActiveSite,
) -> Result<Vec<ManagedRule>, ApiError> {
    let rows = sqlx::query_as::<_, ManagedRuleRow>(
        r#"
        SELECT
            id,
            ? AS site_id,
            name,
            description,
            selector,
            type AS rule_type,
            capture_text,
            enabled,
            COALESCE(sample_rate, 1.0) AS sample_rate,
            trigger_config_json,
            metadata_json,
            created_at,
            updated_at
        FROM tracking_rules
        WHERE site_id IN (?, ?)
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(&site.id)
    .bind(&site.id)
    .bind(&site.tracking_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(ManagedRuleRow::into_rule)
        .collect::<Result<Vec<_>, _>>()
}

async fn load_managed_rule(
    pool: &SqlitePool,
    site: &ActiveSite,
    rule_id: &str,
) -> Result<ManagedRule, ApiError> {
    sqlx::query_as::<_, ManagedRuleRow>(
        r#"
        SELECT
            id,
            ? AS site_id,
            name,
            description,
            selector,
            type AS rule_type,
            capture_text,
            enabled,
            COALESCE(sample_rate, 1.0) AS sample_rate,
            trigger_config_json,
            metadata_json,
            created_at,
            updated_at
        FROM tracking_rules
        WHERE id = ? AND site_id IN (?, ?)
        LIMIT 1
        "#,
    )
    .bind(&site.id)
    .bind(rule_id)
    .bind(&site.id)
    .bind(&site.tracking_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::BadRequest("Unknown tracking rule".to_owned()))?
    .into_rule()
}

fn rule_request_source(headers: &HeaderMap) -> Result<&str, ApiError> {
    if let Some(origin) = headers.get(header::ORIGIN) {
        return origin.to_str().map_err(|_| disallowed_rule_source());
    }
    if let Some(referer) = headers.get(header::REFERER) {
        return referer.to_str().map_err(|_| disallowed_rule_source());
    }
    Err(disallowed_rule_source())
}

fn disallowed_rule_source() -> ApiError {
    ApiError::Forbidden("Browser rule loading requires an allowed Origin or Referer".to_owned())
}

fn normalize_rule_name(value: &str) -> Result<String, ApiError> {
    normalize_bounded_text(value, "Rule name", MAX_RULE_NAME_BYTES, false)
}

fn normalize_description(value: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > MAX_RULE_DESCRIPTION_BYTES
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return Err(ApiError::BadRequest(
            "Rule description must be at most 1000 characters".to_owned(),
        ));
    }
    Ok(Some(value.to_owned()))
}

fn normalize_selector(value: &str) -> Result<String, ApiError> {
    normalize_bounded_text(value, "Rule selector", MAX_RULE_SELECTOR_BYTES, false)
}

fn normalize_bounded_text(
    value: &str,
    label: &str,
    max_bytes: usize,
    allow_newlines: bool,
) -> Result<String, ApiError> {
    let value = value.trim();
    let has_invalid_control = value.chars().any(|character| {
        character.is_control() && !(allow_newlines && matches!(character, '\n' | '\t'))
    });
    if value.is_empty() || value.len() > max_bytes || has_invalid_control {
        return Err(ApiError::BadRequest(format!(
            "{label} must be between 1 and {max_bytes} characters"
        )));
    }
    Ok(value.to_owned())
}

fn normalize_rule_type(value: &str) -> Result<String, ApiError> {
    match value.trim() {
        "click" | "submit" | "view" => Ok(value.trim().to_owned()),
        _ => Err(ApiError::BadRequest(
            "Rule type must be click, submit, or view".to_owned(),
        )),
    }
}

fn normalize_sample_rate(value: f64) -> Result<f64, ApiError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(ApiError::BadRequest(
            "sample_rate must be a finite number between 0 and 1".to_owned(),
        ));
    }
    Ok(value)
}

fn normalize_config(value: Value, field: &str) -> Result<Value, ApiError> {
    if !value.is_object() {
        return Err(ApiError::BadRequest(format!(
            "{field} must be a JSON object"
        )));
    }
    if serde_json::to_vec(&value)?.len() > MAX_RULE_CONFIG_BYTES {
        return Err(ApiError::BadRequest(format!(
            "{field} must be at most {MAX_RULE_CONFIG_BYTES} bytes"
        )));
    }
    Ok(value)
}

fn normalize_rule_payload(
    trigger_config: Value,
    metadata: Value,
    event_name: &str,
) -> Result<(Value, Value), ApiError> {
    let mut trigger_config = normalize_config(trigger_config, "trigger_config")?;
    let mut metadata = normalize_config(metadata, "metadata")?;
    let Some(trigger) = trigger_config.as_object_mut() else {
        return Err(ApiError::BadRequest(
            "trigger_config must be a JSON object".to_owned(),
        ));
    };
    let Some(metadata) = metadata.as_object_mut() else {
        return Err(ApiError::BadRequest(
            "metadata must be a JSON object".to_owned(),
        ));
    };

    let legacy_custom_props = trigger.remove("custom_props");
    if legacy_custom_props.is_some() && metadata.contains_key("custom_props") {
        return Err(ApiError::BadRequest(
            "custom_props must be supplied in metadata or trigger_config, not both".to_owned(),
        ));
    }
    if let Some(custom_props) = legacy_custom_props {
        metadata.insert("custom_props".to_owned(), custom_props);
    }
    metadata
        .entry("event_name".to_owned())
        .or_insert_with(|| Value::String(event_name.to_owned()));

    canonicalize_trigger_config(trigger)?;
    normalize_rule_metadata(metadata)?;
    Ok((trigger_config, Value::Object(metadata.clone())))
}

#[cfg(test)]
fn normalize_trigger_config(value: Value) -> Result<Value, ApiError> {
    normalize_rule_payload(value, json!({}), "event").map(|(trigger_config, _)| trigger_config)
}

fn canonicalize_trigger_config(
    trigger: &mut serde_json::Map<String, Value>,
) -> Result<(), ApiError> {
    if let Some(selector_type) = trigger.get("selector_type") {
        validate_selector_type(selector_type, "trigger_config.selector_type")?;
    }

    let legacy_event = trigger.remove("event");
    if !trigger.contains_key("dom_event") {
        if let Some(event) = legacy_event {
            let event = event.as_str().ok_or_else(|| {
                ApiError::BadRequest("trigger_config.event must be a string".to_owned())
            })?;
            let canonical = match event {
                "change" | "focus" | "blur" | "keydown" | "play" | "ended" => Some(event),
                "click" => Some("click"),
                "double_click" | "dblclick" => Some("dblclick"),
                "mouse_down" | "mousedown" => Some("mousedown"),
                "mouse_up" | "mouseup" => Some("mouseup"),
                "key_up" | "keyup" => Some("keyup"),
                "key_press" | "keypress" => Some("keypress"),
                "submit" => Some("submit"),
                "view" => None,
                _ => {
                    return Err(ApiError::BadRequest(
                        "Unsupported rule event trigger".to_owned(),
                    ))
                }
            };
            if let Some(canonical) = canonical {
                trigger.insert("dom_event".to_owned(), Value::String(canonical.to_owned()));
            }
        }
    }
    for field in ["dom_event", "on"] {
        if let Some(event) = trigger.get(field) {
            let event = event.as_str().ok_or_else(|| {
                ApiError::BadRequest(format!("trigger_config.{field} must be a string"))
            })?;
            if !matches!(
                event,
                "click"
                    | "dblclick"
                    | "keypress"
                    | "keyup"
                    | "mousedown"
                    | "mouseup"
                    | "submit"
                    | "change"
                    | "focus"
                    | "blur"
                    | "keydown"
                    | "play"
                    | "ended"
            ) {
                return Err(ApiError::BadRequest(
                    "Unsupported rule DOM event".to_owned(),
                ));
            }
        }
    }

    if !trigger.contains_key("page_path") {
        if let Some(page) = trigger.remove("page") {
            trigger.insert("page_path".to_owned(), page);
        }
    }
    if let Some(page) = trigger.get("page_path") {
        let page = page.as_str().ok_or_else(|| {
            ApiError::BadRequest("trigger_config.page_path must be a string".to_owned())
        })?;
        if !page.starts_with('/')
            || page.len() > MAX_RULE_SELECTOR_BYTES
            || page.chars().any(char::is_control)
        {
            return Err(ApiError::BadRequest(
                "trigger_config.page_path must be a relative path starting with /".to_owned(),
            ));
        }
    }

    if let Some(condition) = trigger.remove("condition") {
        let (key_condition, condition_value) = canonical_rule_condition(&condition)?;
        trigger
            .entry("key_condition".to_owned())
            .or_insert(Value::String(key_condition));
        if let Some(value) = condition_value {
            trigger.entry("condition_value".to_owned()).or_insert(value);
        }
    }
    if let Some(condition) = trigger.get("key_condition") {
        let condition = condition.as_str().ok_or_else(|| {
            ApiError::BadRequest("trigger_config.key_condition must be a string".to_owned())
        })?;
        if !matches!(
            condition,
            "characters" | "debounce" | "enter" | "escape" | "period" | "space"
        ) {
            return Err(ApiError::BadRequest("Unsupported key condition".to_owned()));
        }
    }
    if let Some(value) = trigger
        .get("condition_value")
        .or_else(|| trigger.get("key_value"))
    {
        let value = value
            .as_f64()
            .ok_or_else(|| ApiError::BadRequest("condition_value must be a number".to_owned()))?;
        if !value.is_finite() || !(0.0..=10_000.0).contains(&value) {
            return Err(ApiError::BadRequest(
                "condition_value must be between 0 and 10000".to_owned(),
            ));
        }
    }
    Ok(())
}

fn canonical_rule_condition(value: &Value) -> Result<(String, Option<Value>), ApiError> {
    let condition = value.as_object().ok_or_else(|| {
        ApiError::BadRequest("trigger_config.condition must be an object".to_owned())
    })?;
    let kind = condition
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::BadRequest("condition.kind is required".to_owned()))?;
    match kind {
        "debounce" => {
            let milliseconds = condition
                .get("milliseconds")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    ApiError::BadRequest("debounce condition requires milliseconds".to_owned())
                })?;
            if !(100..=60_000).contains(&milliseconds) {
                return Err(ApiError::BadRequest(
                    "condition.milliseconds must be between 100 and 60000".to_owned(),
                ));
            }
            let seconds = serde_json::Number::from_f64(milliseconds as f64 / 1_000.0)
                .ok_or_else(|| ApiError::BadRequest("Invalid debounce duration".to_owned()))?;
            Ok(("debounce".to_owned(), Some(Value::Number(seconds))))
        }
        "key" => {
            let key = condition
                .get("key")
                .and_then(Value::as_str)
                .ok_or_else(|| ApiError::BadRequest("key condition requires key".to_owned()))?;
            let key = match key {
                "Enter" => "enter",
                "Escape" => "escape",
                "." => "period",
                " " => "space",
                _ => {
                    return Err(ApiError::BadRequest(
                        "condition.key must be Enter, Escape, period, or space".to_owned(),
                    ))
                }
            };
            Ok((key.to_owned(), None))
        }
        "characters" => {
            let count = condition
                .get("count")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    ApiError::BadRequest("characters condition requires count".to_owned())
                })?;
            if !(1..=10_000).contains(&count) {
                return Err(ApiError::BadRequest(
                    "condition.count must be between 1 and 10000".to_owned(),
                ));
            }
            Ok(("characters".to_owned(), Some(Value::from(count))))
        }
        _ => Err(ApiError::BadRequest(
            "condition.kind must be debounce, key, or characters".to_owned(),
        )),
    }
}

fn normalize_rule_metadata(metadata: &mut serde_json::Map<String, Value>) -> Result<(), ApiError> {
    let event_name = metadata
        .get("event_name")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::BadRequest("metadata.event_name must be a string".to_owned()))?;
    normalize_rule_name(event_name)?;
    if let Some(custom_props) = metadata.get_mut("custom_props") {
        normalize_custom_props(custom_props)?;
    }
    Ok(())
}

fn normalize_custom_props(value: &mut Value) -> Result<(), ApiError> {
    let props = value
        .as_array()
        .ok_or_else(|| ApiError::BadRequest("metadata.custom_props must be an array".to_owned()))?;
    if props.len() > 10 {
        return Err(ApiError::BadRequest(
            "A rule can capture at most 10 custom properties".to_owned(),
        ));
    }
    let mut keys = std::collections::HashSet::with_capacity(props.len());
    let mut canonical = Vec::with_capacity(props.len());
    for prop in props {
        let prop = prop.as_object().ok_or_else(|| {
            ApiError::BadRequest("Each custom property must be an object".to_owned())
        })?;
        let key = prop.get("key").and_then(Value::as_str).ok_or_else(|| {
            ApiError::BadRequest("Each custom property requires a key".to_owned())
        })?;
        let key = normalize_bounded_text(key, "Custom property key", 64, false)?;
        if !keys.insert(key.clone()) {
            return Err(ApiError::BadRequest(
                "Custom property keys must be unique".to_owned(),
            ));
        }
        let selector = prop
            .get("selector")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ApiError::BadRequest("Each custom property requires a selector".to_owned())
            })?;
        normalize_selector(selector)?;
        if let Some(source) = prop.get("source") {
            if !matches!(source.as_str(), Some("value" | "text_content")) {
                return Err(ApiError::BadRequest(
                    "Custom property source must be value or text_content".to_owned(),
                ));
            }
        }
        let selector_type = prop
            .get("selector_type")
            .cloned()
            .unwrap_or_else(|| Value::String("css".to_owned()));
        validate_selector_type(&selector_type, "metadata.custom_props[].selector_type")?;
        canonical.push(json!({
            "key": key,
            "selector": selector,
            "selector_type": selector_type
        }));
    }
    *value = Value::Array(canonical);
    Ok(())
}

fn validate_selector_type(value: &Value, field: &str) -> Result<(), ApiError> {
    let selector_type = value
        .as_str()
        .ok_or_else(|| ApiError::BadRequest(format!("{field} must be a string")))?;
    if !matches!(selector_type, "css" | "id" | "class" | "text" | "xpath") {
        return Err(ApiError::BadRequest(
            "selector_type must be css, id, class, text, or xpath".to_owned(),
        ));
    }
    Ok(())
}

fn decode_stored_object(value: &str, field: &str) -> Result<Value, ApiError> {
    let decoded = serde_json::from_str::<Value>(value)?;
    if !decoded.is_object() {
        return Err(ApiError::Internal(anyhow::anyhow!(
            "stored {field} is not a JSON object"
        )));
    }
    Ok(decoded)
}

fn default_true() -> bool {
    true
}

fn default_sample_rate() -> f64 {
    1.0
}

fn empty_object() -> Value {
    json!({})
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::storage::sqlite;

    use super::*;

    #[test]
    fn rule_configuration_accepts_ten_properties_but_not_eleven() {
        let mut fields = json!((0..10)
            .map(|i| json!({"key":format!("field{i}"),"selector":format!("#field{i}")}))
            .collect::<Vec<_>>());
        assert!(normalize_custom_props(&mut fields).is_ok());
        fields
            .as_array_mut()
            .unwrap()
            .push(json!({"key":"extra","selector":"#extra"}));
        assert!(normalize_custom_props(&mut fields).is_err());
    }
    #[test]
    fn accepts_common_native_interactions_in_canonical_and_legacy_config() {
        for event in ["change", "focus", "blur", "keydown", "play", "ended"] {
            for field in ["dom_event", "on", "event"] {
                let result =
                    normalize_trigger_config(json!({field: event, "selector_type": "css"}))
                        .unwrap();
                let canonical_field = if field == "event" { "dom_event" } else { field };
                assert_eq!(result[canonical_field], event);
            }
        }
        assert!(normalize_trigger_config(json!({"dom_event": "arbitrary"})).is_err());
    }

    #[test]
    fn validates_rule_fields_configs_and_sample_rate() {
        let request: CreateRuleRequest = serde_json::from_value(json!({
            "name": "Signup button",
            "selector": "#signup",
            "type": "click"
        }))
        .unwrap();
        let rule = validated_create_rule(request).unwrap();
        assert_eq!(rule.sample_rate, 1.0);
        assert!(rule.enabled);
        assert_eq!(rule.trigger_config, json!({}));
        assert_eq!(rule.metadata, json!({ "event_name": "Signup button" }));

        assert!(normalize_rule_type("hover").is_err());
        assert!(normalize_sample_rate(f64::NAN).is_err());
        assert!(normalize_sample_rate(-0.01).is_err());
        assert!(normalize_sample_rate(1.01).is_err());
        assert!(normalize_config(json!([]), "trigger_config").is_err());
        assert!(normalize_config(
            json!({ "large": "x".repeat(MAX_RULE_CONFIG_BYTES) }),
            "metadata"
        )
        .is_err());
        assert!(normalize_trigger_config(json!({
            "selector_type": "id",
            "event": "key_up",
            "condition": { "kind": "debounce", "milliseconds": 800 },
            "page": "/pricing",
            "custom_props": [
                { "key": "query", "selector": "#search", "source": "value" }
            ]
        }))
        .is_ok());
        assert!(normalize_trigger_config(json!({
            "custom_props": [
                { "key": "duplicate", "selector": "#one" },
                { "key": "duplicate", "selector": "#two" }
            ]
        }))
        .is_err());
        assert!(normalize_trigger_config(json!({ "selector_type": "magic" })).is_err());
        assert!(normalize_rule_name(&"x".repeat(MAX_RULE_NAME_BYTES + 1)).is_err());
        assert!(normalize_selector(&"x".repeat(MAX_RULE_SELECTOR_BYTES + 1)).is_err());
        assert!(normalize_description(Some(&"x".repeat(MAX_RULE_DESCRIPTION_BYTES + 1))).is_err());
    }

    #[test]
    fn public_rule_source_requires_origin_or_referer() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, "https://example.com".parse().unwrap());
        headers.insert(
            header::REFERER,
            "https://ignored.example/page".parse().unwrap(),
        );
        assert_eq!(
            rule_request_source(&headers).unwrap(),
            "https://example.com"
        );

        headers.remove(header::ORIGIN);
        assert_eq!(
            rule_request_source(&headers).unwrap(),
            "https://ignored.example/page"
        );
        headers.remove(header::REFERER);
        assert!(matches!(
            rule_request_source(&headers),
            Err(ApiError::Forbidden(_))
        ));
    }

    #[tokio::test]
    async fn public_delivery_supports_internal_and_legacy_site_ids_without_cross_site_leaks() {
        let pool = test_pool().await;
        let (site_a, site_b) = seed_sites(&pool).await;
        for (id, site_id, enabled, sample_rate) in [
            ("modern", "site-a", true, 0.25),
            ("legacy", "public-a", true, 0.75),
            ("disabled", "site-a", false, 1.0),
            ("other", "site-b", true, 1.0),
        ] {
            sqlx::query(
                r#"
                INSERT INTO tracking_rules (
                    id,
                    site_id,
                    name,
                    selector,
                    type,
                    enabled,
                    sample_rate,
                    trigger_config_json,
                    metadata_json,
                    created_by_user_id
                )
                VALUES (?, ?, ?, '#target', 'click', ?, ?, '{}', '{}', 'owner')
                "#,
            )
            .bind(id)
            .bind(site_id)
            .bind(id)
            .bind(enabled)
            .bind(sample_rate)
            .execute(&pool)
            .await
            .unwrap();
        }

        let public = load_public_rules(&pool, &site_a).await.unwrap();
        assert_eq!(public.len(), 2);
        assert_eq!(
            public
                .iter()
                .map(|rule| (rule.id.as_str(), rule.sample_rate))
                .collect::<Vec<_>>(),
            vec![("legacy", 0.75), ("modern", 0.25)]
        );
        let public_json = serde_json::to_value(&public[0]).unwrap();
        assert!(public_json.get("sample_rate").is_some());
        assert!(public_json.get("trigger_config").is_some());
        assert!(public_json.get("metadata").is_some());

        let managed = load_managed_rules(&pool, &site_a).await.unwrap();
        assert_eq!(managed.len(), 3);
        assert!(managed.iter().all(|rule| rule.site_id == "site-a"));
        let managed_json = serde_json::to_value(&managed[0]).unwrap();
        for field in [
            "id",
            "site_id",
            "name",
            "description",
            "selector",
            "type",
            "capture_text",
            "enabled",
            "lifetime_event_count",
            "sample_rate",
            "trigger_config",
            "metadata",
            "created_at",
            "updated_at",
        ] {
            assert!(managed_json.get(field).is_some(), "missing {field}");
        }

        assert_eq!(load_public_rules(&pool, &site_b).await.unwrap().len(), 1);
        assert!(
            sites::origin_is_allowed(&pool, &site_a.id, "https://example.com/a/page", false)
                .await
                .unwrap()
        );
        assert!(!sites::origin_is_allowed(
            &pool,
            &site_a.id,
            "https://attacker.example/page",
            false,
        )
        .await
        .unwrap());
    }

    #[tokio::test]
    async fn managed_rule_reads_allow_members_but_writes_require_admin() {
        let pool = test_pool().await;
        let (site, _) = seed_sites(&pool).await;

        assert!(sites::resolve_site_for_user(&pool, "member", &site.id)
            .await
            .is_ok());
        assert!(matches!(
            sites::resolve_site_for_admin(&pool, "member", &site.id).await,
            Err(ApiError::Forbidden(_))
        ));
        assert!(matches!(
            sites::resolve_site_for_user(&pool, "outsider", &site.id).await,
            Err(ApiError::Forbidden(_))
        ));
    }

    async fn seed_sites(pool: &SqlitePool) -> (ActiveSite, ActiveSite) {
        for (id, email) in [
            ("owner", "owner@example.com"),
            ("member", "member@example.com"),
            ("outsider", "outsider@example.com"),
        ] {
            sqlx::query("INSERT INTO users (id, email) VALUES (?, ?)")
                .bind(id)
                .bind(email)
                .execute(pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ('org', 'Org', 'org')")
            .execute(pool)
            .await
            .unwrap();
        for (user_id, role) in [("owner", "owner"), ("member", "member")] {
            sqlx::query(
                "INSERT INTO organization_members (organization_id, user_id, role) VALUES ('org', ?, ?)",
            )
            .bind(user_id)
            .bind(role)
            .execute(pool)
            .await
            .unwrap();
        }
        for (id, tracking_id, domain) in [
            ("site-a", "public-a", "example.com"),
            ("site-b", "public-b", "other.example.com"),
        ] {
            sqlx::query(
                r#"
                INSERT INTO sites (
                    id, organization_id, tracking_id, name, domain, created_by_user_id
                )
                VALUES (?, 'org', ?, ?, ?, 'owner')
                "#,
            )
            .bind(id)
            .bind(tracking_id)
            .bind(id)
            .bind(domain)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO site_memberships (site_id, user_id, role) VALUES ('site-a', 'member', 'read_only')",
        )
        .execute(pool)
        .await
        .unwrap();

        (
            ActiveSite {
                created_by_user_id: Some("owner".to_owned()),
                id: "site-a".to_owned(),
                organization_id: Some("org".to_owned()),
                team_id: None,
                tracking_id: "public-a".to_owned(),
            },
            ActiveSite {
                created_by_user_id: Some("owner".to_owned()),
                id: "site-b".to_owned(),
                organization_id: Some("org".to_owned()),
                team_id: None,
                tracking_id: "public-b".to_owned(),
            },
        )
    }

    async fn test_pool() -> SqlitePool {
        let db_path = std::env::temp_dir().join(format!(
            "owleye-rules-{}.sqlite",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        sqlite::connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap()
    }
}
