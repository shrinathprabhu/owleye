use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    auth, demo,
    privacy::audit::{self, AuditEvent},
    sites::{self, ActiveSite},
    ApiError, AppState,
};

pub(crate) const DEFAULT_RETENTION_DAYS: u16 = 0; // No expiry in the self-hosted edition.

#[derive(Debug, Serialize)]
pub(crate) struct SiteConfigurationResponse {
    capabilities: SiteConfigurationCapabilities,
    site: SiteConfigurationValues,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct SiteConfigurationCapabilities {
    ai_toggle: bool,
    contact_support: bool,
    delete_site: bool,
    pause_tracking: bool,
    privacy_controls: bool,
    rename_site: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct SiteConfigurationValues {
    ai_enabled: bool,
    data_retention_days: Option<i64>,
    domain: String,
    honor_privacy_signals: bool,
    id: String,
    name: String,
    privacy_contact_email: Option<String>,
    timezone: String,
    tracking_id: String,
    tracking_paused: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateSiteConfigurationRequest {
    #[serde(default)]
    ai_enabled: Option<bool>,
    #[serde(default)]
    data_retention_days: Option<u16>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    privacy_contact_email: Option<String>,
    #[serde(default)]
    tracking_paused: Option<bool>,
}

pub(crate) async fn get_site_configuration(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Json<SiteConfigurationResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        let site = demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        if !sites::demo_site_role(site).is_owner() {
            return Err(ApiError::Forbidden(
                "Site owner access is required".to_owned(),
            ));
        }
        return Ok(Json(SiteConfigurationResponse {
            capabilities: capabilities(),
            site: SiteConfigurationValues {
                ai_enabled: false,
                data_retention_days: None,
                domain: site.domain.to_owned(),
                honor_privacy_signals: false,
                id: site.id.to_owned(),
                name: site.name.to_owned(),
                privacy_contact_email: None,
                timezone: site.timezone.to_owned(),
                tracking_id: site.tracking_id.to_owned(),
                tracking_paused: false,
            },
        }));
    }
    let site = sites::resolve_site_for_owner(&state.sqlite, &session.id, &site_identifier).await?;
    response_for_site(&state, &site).await.map(Json)
}

pub(crate) async fn update_site_configuration(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<UpdateSiteConfigurationRequest>,
) -> Result<Json<SiteConfigurationResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    if request.ai_enabled.is_none()
        && request.data_retention_days.is_none()
        && request.name.is_none()
        && request.privacy_contact_email.is_none()
        && request.tracking_paused.is_none()
    {
        return Err(ApiError::BadRequest(
            "At least one setting must be provided".to_owned(),
        ));
    }
    let site = sites::resolve_site_for_owner(&state.sqlite, &session.id, &site_identifier).await?;
    if request.data_retention_days.is_some() {
        return Err(ApiError::BadRequest(
            "Analytics retention is unlimited on this server".to_owned(),
        ));
    }
    let privacy_contact_was_provided = request.privacy_contact_email.is_some();
    let privacy_contact_email = request
        .privacy_contact_email
        .as_deref()
        .map(normalize_optional_email)
        .transpose()?
        .flatten();

    if let Some(name) = request.name.as_deref() {
        let name = normalize_name(name)?;
        sqlx::query("UPDATE sites SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(Utc::now().to_rfc3339())
            .bind(&site.id)
            .execute(&state.sqlite)
            .await?;
    }
    if request.ai_enabled.is_some()
        || privacy_contact_was_provided
        || request.tracking_paused.is_some()
    {
        sqlx::query(
            r#"
            INSERT INTO site_settings (
                site_id,
                ai_enabled,
                tracking_paused,
                data_retention_days,
                honor_dnt,
                privacy_contact_email
            )
            VALUES (?, COALESCE(?, 0), COALESCE(?, 0), 90, 1, ?)
            ON CONFLICT(site_id) DO UPDATE SET
                ai_enabled = COALESCE(?, site_settings.ai_enabled),
                tracking_paused = COALESCE(?, site_settings.tracking_paused),
                honor_dnt = 1,
                privacy_contact_email = CASE
                    WHEN ? THEN ?
                    ELSE site_settings.privacy_contact_email
                END
            "#,
        )
        .bind(&site.id)
        .bind(request.ai_enabled)
        .bind(request.tracking_paused)
        .bind(&privacy_contact_email)
        .bind(request.ai_enabled)
        .bind(request.tracking_paused)
        .bind(privacy_contact_was_provided)
        .bind(&privacy_contact_email)
        .execute(&state.sqlite)
        .await?;
    }
    audit::record(
        &state.sqlite,
        AuditEvent {
            action: "privacy.site_settings_updated",
            metadata: json!({
                "ai_setting_changed": request.ai_enabled.is_some(),
                "name_changed": request.name.is_some(),
                "privacy_contact_changed": privacy_contact_was_provided,
                "tracking_status_changed": request.tracking_paused.is_some()
            }),
            organization_id: site.organization_id.as_deref(),
            target_id: Some(&site.id),
            target_type: Some("site"),
            user_id: Some(&session.id),
        },
    )
    .await?;
    response_for_site(&state, &site).await.map(Json)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct IngestionPolicy {
    pub(crate) retention_days: u16,
    pub(crate) tracking_paused: bool,
}

#[derive(Clone, Copy, Debug, sqlx::FromRow)]
struct IngestionPolicyRow {
    tracking_paused: bool,
}

pub(crate) async fn ingestion_policy(
    state: &AppState,
    site: &ActiveSite,
) -> Result<IngestionPolicy, ApiError> {
    let row = sqlx::query_as::<_, IngestionPolicyRow>(
        r#"
        SELECT
            COALESCE(tracking_paused, 0) AS tracking_paused
        FROM sites
        LEFT JOIN site_settings ON site_settings.site_id = sites.id
        WHERE sites.id = ?
        LIMIT 1
        "#,
    )
    .bind(&site.id)
    .fetch_optional(&state.sqlite)
    .await?;
    Ok(row.map_or(
        IngestionPolicy {
            retention_days: DEFAULT_RETENTION_DAYS,
            tracking_paused: false,
        },
        |row| IngestionPolicy {
            retention_days: DEFAULT_RETENTION_DAYS,
            tracking_paused: row.tracking_paused,
        },
    ))
}

async fn response_for_site(
    state: &AppState,
    site: &ActiveSite,
) -> Result<SiteConfigurationResponse, ApiError> {
    let values = sqlx::query_as::<_, SiteConfigurationValues>(
        r#"
        SELECT
            sites.id,
            sites.tracking_id,
            sites.name,
            sites.domain,
            COALESCE(sites.default_timezone, sites.timezone, 'UTC') AS timezone,
            COALESCE(site_settings.tracking_paused, 0) AS tracking_paused,
            COALESCE(site_settings.ai_enabled, 0) AS ai_enabled,
            NULL AS data_retention_days,
            0 AS honor_privacy_signals,
            NULLIF(site_settings.privacy_contact_email, '') AS privacy_contact_email
        FROM sites
        LEFT JOIN site_settings ON site_settings.site_id = sites.id
        WHERE sites.id = ?
        LIMIT 1
        "#,
    )
    .bind(&site.id)
    .fetch_optional(&state.sqlite)
    .await?
    .ok_or_else(|| ApiError::BadRequest("Unknown site".to_owned()))?;
    Ok(SiteConfigurationResponse {
        capabilities: capabilities(),
        site: values,
    })
}

fn capabilities() -> SiteConfigurationCapabilities {
    SiteConfigurationCapabilities {
        ai_toggle: true,
        contact_support: false,
        delete_site: true,
        pause_tracking: true,
        privacy_controls: true,
        rename_site: true,
    }
}

fn normalize_optional_email(value: &str) -> Result<Option<String>, ApiError> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(None);
    }
    let valid = value.len() <= 254
        && !value.chars().any(char::is_whitespace)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && local.len() <= 64
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });
    if !valid {
        return Err(ApiError::BadRequest(
            "Privacy contact must be a valid email address".to_owned(),
        ));
    }
    Ok(Some(value))
}

fn normalize_name(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 || value.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "Site name must be between 1 and 120 characters".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_contact_is_normalized_and_bounded() {
        assert_eq!(
            normalize_optional_email(" Privacy@Example.com ").unwrap(),
            Some("privacy@example.com".to_owned())
        );
        assert_eq!(normalize_optional_email(" ").unwrap(), None);
        assert!(normalize_optional_email("not-an-email").is_err());
    }
}
