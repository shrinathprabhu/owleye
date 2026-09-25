use std::collections::HashSet;

use serde_json::Value;

use super::{FunnelDefinition, FunnelPropertyGroup, WidgetRequest};
use crate::ApiError;

const MAX_CONFIG_BYTES: usize = 16 * 1024;

pub(super) fn validate_widget(mut request: WidgetRequest) -> Result<WidgetRequest, ApiError> {
    normalize_name(&request.title, "Widget title")?;
    if !matches!(request.kind.as_str(), "chart" | "funnel") {
        return Err(ApiError::BadRequest(
            "Widget kind must be chart or funnel".to_owned(),
        ));
    }
    if !matches!(
        request.visualization.as_str(),
        "line" | "bar" | "donut" | "pie" | "scatter" | "map" | "funnel"
    ) {
        return Err(ApiError::BadRequest(
            "Unsupported widget visualization".to_owned(),
        ));
    }
    if request.kind == "funnel" && request.visualization != "funnel" {
        return Err(ApiError::BadRequest(
            "Funnel widgets must use the funnel visualization".to_owned(),
        ));
    }
    if !matches!(
        request.comparison.as_str(),
        "none" | "today_vs_yesterday" | "last_7_vs_previous_7" | "last_30_vs_previous_30"
    ) {
        return Err(ApiError::BadRequest(
            "Unsupported widget comparison".to_owned(),
        ));
    }
    if !request.filters.is_object() {
        return Err(ApiError::BadRequest(
            "Widget filters must be an object".to_owned(),
        ));
    }
    if request
        .date_range
        .as_ref()
        .is_some_and(|r| !(1..=365).contains(&r.days))
    {
        return Err(ApiError::BadRequest(
            "Date range must be between 1 and 365 days".into(),
        ));
    }
    if !matches!(
        request.breakdown.as_str(),
        "time"
            | "browser"
            | "country"
            | "os"
            | "city"
            | "device"
            | "campaign"
            | "referrer"
            | "source"
            | "medium"
            | "page"
    ) {
        return Err(ApiError::BadRequest("Unsupported chart breakdown".into()));
    }
    if request.kind == "funnel" && request.breakdown != "time" {
        return Err(ApiError::BadRequest(
            "Breakdowns are available for charts; funnel steps define the grouping".into(),
        ));
    }
    if request.kind == "chart" {
        validate_chart_filters(&request.filters)?;
        let source = request
            .source
            .as_ref()
            .ok_or_else(|| ApiError::BadRequest("Chart widgets require a source".to_owned()))?;
        if !matches!(source.kind.as_str(), "event" | "rule" | "future_event") {
            return Err(ApiError::BadRequest(
                "Widget source must be event, rule, or future_event".to_owned(),
            ));
        }
        normalize_name(&source.id, "Source id")?;
        normalize_name(&source.name, "Source name")?;
        if request.visualization == "map" {
            request.breakdown = "country".into();
        }
        if request.funnel.is_some() {
            return Err(ApiError::BadRequest(
                "Chart widgets cannot contain a funnel definition".to_owned(),
            ));
        }
    } else {
        validate_funnel(
            request
                .funnel
                .as_ref()
                .ok_or_else(|| ApiError::BadRequest("Funnel definition is required".to_owned()))?,
        )?;
    }
    if !matches!(request.display.size.as_str(), "compact" | "wide") {
        return Err(ApiError::BadRequest(
            "Widget size must be compact or wide".to_owned(),
        ));
    }
    if serde_json::to_vec(&request)?.len() > MAX_CONFIG_BYTES {
        return Err(ApiError::BadRequest(format!(
            "Widget configuration must be at most {MAX_CONFIG_BYTES} bytes"
        )));
    }
    Ok(request)
}

fn validate_chart_filters(filters: &Value) -> Result<(), ApiError> {
    let Some(filters) = filters.as_object() else {
        return Err(ApiError::BadRequest(
            "Widget filters must be an object".to_owned(),
        ));
    };
    for (key, value) in filters {
        if !matches!(key.as_str(), "country" | "page") {
            return Err(ApiError::BadRequest(format!(
                "Unsupported chart filter: {key}"
            )));
        }
        let value = value
            .as_str()
            .ok_or_else(|| ApiError::BadRequest(format!("Chart filter {key} must be a string")))?;
        if value.len() > 2_048 || value.chars().any(char::is_control) {
            return Err(ApiError::BadRequest(format!(
                "Chart filter {key} must be at most 2048 characters"
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_funnel(funnel: &FunnelDefinition) -> Result<(), ApiError> {
    if !matches!(
        funnel.conversion_window.as_str(),
        "same_session" | "1h" | "1d" | "7d" | "30d"
    ) {
        return Err(ApiError::BadRequest(
            "Funnel conversion_window must be same_session, 1h, 1d, 7d, or 30d".to_owned(),
        ));
    }
    if !matches!(funnel.entry_mode.as_str(), "closed" | "open") {
        return Err(ApiError::BadRequest(
            "Funnel entry_mode must be closed or open".to_owned(),
        ));
    }
    if !matches!(funnel.order_mode.as_str(), "ordered" | "exact") {
        return Err(ApiError::BadRequest(
            "Funnel order_mode must be ordered or exact".to_owned(),
        ));
    }
    if !(2..=10).contains(&funnel.steps.len()) {
        return Err(ApiError::BadRequest(
            "A funnel requires between 2 and 10 ordered steps".to_owned(),
        ));
    }
    let mut ids = HashSet::with_capacity(funnel.steps.len());
    for step in &funnel.steps {
        normalize_name(&step.id, "Funnel step id")?;
        normalize_name(&step.name, "Funnel step name")?;
        if !ids.insert(&step.id) {
            return Err(ApiError::BadRequest(
                "Funnel step ids must be unique".to_owned(),
            ));
        }
        if !matches!(step.condition.kind.as_str(), "page" | "event" | "rule") {
            return Err(ApiError::BadRequest(
                "Funnel condition kind must be page, event, or rule".to_owned(),
            ));
        }
        normalize_condition_id(&step.condition.id)?;
        normalize_name(&step.condition.name, "Funnel condition name")?;
        if !matches!(step.condition.operator.as_str(), "exact" | "starts_with") {
            return Err(ApiError::BadRequest(
                "Funnel condition operator must be exact or starts_with".to_owned(),
            ));
        }
        if let Some(properties) = step.condition.property_filters.as_ref() {
            validate_property_group(properties)?;
        }
    }
    Ok(())
}

pub(super) fn validate_previewable_funnel(funnel: &FunnelDefinition) -> Result<(), ApiError> {
    if funnel.entry_mode == "open" {
        return Err(ApiError::BadRequest(
            "Open-entry funnel preview is not available yet; use closed entry mode".to_owned(),
        ));
    }
    Ok(())
}

fn validate_property_group(group: &FunnelPropertyGroup) -> Result<(), ApiError> {
    if !matches!(group.logic.as_str(), "and" | "or") {
        return Err(ApiError::BadRequest(
            "Property filter operator must be and or or".to_owned(),
        ));
    }
    if group.filters.is_empty() || group.filters.len() > 10 {
        return Err(ApiError::BadRequest(
            "Property filters require between 1 and 10 items".to_owned(),
        ));
    }
    let mut ids = HashSet::with_capacity(group.filters.len());
    for filter in &group.filters {
        normalize_name(&filter.id, "Property filter id")?;
        if !ids.insert(&filter.id) {
            return Err(ApiError::BadRequest(
                "Property filter ids must be unique".to_owned(),
            ));
        }
        validate_property_key(&filter.key)?;
        if !matches!(
            filter.operator.as_str(),
            "equals" | "not_equals" | "contains" | "exists"
        ) {
            return Err(ApiError::BadRequest(
                "Property comparison must be equals, not_equals, contains, or exists".to_owned(),
            ));
        }
        if filter.operator != "exists" && filter.value.is_none() {
            return Err(ApiError::BadRequest(
                "Property comparison requires a value".to_owned(),
            ));
        }
        if let Some(value) = filter.value.as_deref() {
            if value.len() > 512 || value.chars().any(char::is_control) {
                return Err(ApiError::BadRequest(
                    "Property filter values must be at most 512 characters".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_property_key(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(ApiError::BadRequest(
            "Property keys use letters, numbers, dot, dash, or underscore".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_condition_id(value: &str) -> Result<(), ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 2_048 || value.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "Funnel condition id must be between 1 and 2048 characters".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn normalize_name(value: &str, label: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 || value.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(format!(
            "{label} must be between 1 and 120 characters"
        )));
    }
    Ok(value.to_owned())
}
