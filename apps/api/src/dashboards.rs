use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    auth, dashboard_sharing, demo,
    sites::{self, ActiveSite},
    ApiError, AppState,
};

mod query;
mod validation;

use query::clickhouse_datetime;
use query::{chart_conditions, funnel_query, preview_bucket_count, preview_bucket_label};
#[cfg(test)]
use validation::validate_funnel;
use validation::{normalize_name, validate_previewable_funnel, validate_widget};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DashboardSummary {
    capabilities: DashboardCapabilities,
    created_at: String,
    created_by: DashboardUser,
    id: String,
    is_default: bool,
    name: String,
    share_status: Option<dashboard_sharing::DashboardShareStatus>,
    site_id: String,
    updated_at: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct DashboardCapabilities {
    can_delete: bool,
    can_delete_all: bool,
    can_edit: bool,
    can_remove_for_me: bool,
    can_revoke_for_others: bool,
    can_share: bool,
}

#[derive(Clone, Debug, Serialize)]
struct DashboardUser {
    email: String,
    id: String,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DashboardDetail {
    #[serde(flatten)]
    summary: DashboardSummary,
    shares: Vec<dashboard_sharing::DashboardShare>,
    widgets: Vec<DashboardWidget>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DashboardWidget {
    #[serde(default)]
    date_range: Option<WidgetDateRange>,
    #[serde(default = "default_breakdown")]
    breakdown: String,
    comparison: String,
    created_at: String,
    created_by: DashboardUser,
    display: WidgetDisplay,
    filters: Value,
    funnel: Option<FunnelDefinition>,
    id: String,
    kind: String,
    source: Option<WidgetSource>,
    title: String,
    updated_at: String,
    visualization: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WidgetSource {
    #[serde(default)]
    has_location: bool,
    id: String,
    kind: String,
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelDefinition {
    conversion_window: String,
    #[serde(default = "default_entry_mode")]
    entry_mode: String,
    #[serde(default = "default_order_mode")]
    order_mode: String,
    steps: Vec<FunnelStep>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelStep {
    condition: FunnelCondition,
    id: String,
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelCondition {
    id: String,
    kind: String,
    name: String,
    #[serde(default = "default_operator")]
    operator: String,
    #[serde(default)]
    property_filters: Option<FunnelPropertyGroup>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelPropertyGroup {
    filters: Vec<FunnelPropertyFilter>,
    logic: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelPropertyFilter {
    id: String,
    key: String,
    operator: String,
    #[serde(default)]
    value: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WidgetPreviewResponse {
    current: PreviewPeriod,
    granularity: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    map: Option<Vec<MapPreviewPoint>>,
    previous: Option<PreviewPeriod>,
}

#[derive(Debug, Serialize)]
struct PreviewPeriod {
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    points: Option<Vec<ChartPreviewPoint>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps: Option<Vec<FunnelPreviewStep>>,
}

#[derive(Debug, Serialize)]
struct ChartPreviewPoint {
    period: String,
    value: u64,
}

#[derive(Debug, Serialize)]
struct MapPreviewPoint {
    country_code: String,
    name: String,
    value: u64,
}

#[derive(Debug, Serialize)]
struct FunnelPreviewStep {
    avg_time_to_next_ms: Option<u64>,
    count: u64,
    dropoff_count: u64,
    dropoff_rate: f64,
    id: String,
    name: String,
    overall_conversion: f64,
    step_conversion: f64,
    median_time_to_next_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WidgetDisplay {
    order: u16,
    size: String,
}

impl Default for WidgetDisplay {
    fn default() -> Self {
        Self {
            order: 0,
            size: "compact".to_owned(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateDashboardRequest {
    #[serde(default)]
    is_default: bool,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateDashboardRequest {
    #[serde(default)]
    is_default: Option<bool>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WidgetDateRange {
    days: u16,
    #[serde(default)]
    compare_previous: bool,
}

fn default_breakdown() -> String {
    "time".into()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WidgetRequest {
    #[serde(default)]
    date_range: Option<WidgetDateRange>,
    #[serde(default = "default_breakdown")]
    breakdown: String,
    #[serde(default = "default_comparison")]
    comparison: String,
    #[serde(default)]
    display: WidgetDisplay,
    #[serde(default = "empty_object")]
    filters: Value,
    #[serde(default)]
    funnel: Option<FunnelDefinition>,
    kind: String,
    #[serde(default)]
    source: Option<WidgetSource>,
    title: String,
    visualization: String,
}

#[derive(Debug, FromRow)]
struct DashboardRow {
    created_at: i64,
    creator_email: String,
    creator_id: String,
    creator_name: Option<String>,
    id: String,
    is_default: bool,
    name: String,
    share_status: Option<String>,
    site_id: String,
    updated_at: i64,
}

impl DashboardRow {
    fn into_summary(self, current_user_id: &str) -> Result<DashboardSummary, ApiError> {
        let is_creator = self.creator_id == current_user_id;
        let share_status = self
            .share_status
            .as_deref()
            .map(dashboard_sharing::DashboardShareStatus::from_database)
            .transpose()?;
        Ok(DashboardSummary {
            capabilities: DashboardCapabilities {
                can_delete: is_creator,
                can_delete_all: is_creator,
                can_edit: is_creator,
                can_remove_for_me: !is_creator && share_status.is_some(),
                can_revoke_for_others: is_creator,
                can_share: is_creator,
            },
            created_at: timestamp(self.created_at),
            created_by: DashboardUser {
                email: self.creator_email,
                id: self.creator_id,
                name: self.creator_name,
            },
            id: self.id,
            is_default: self.is_default,
            name: self.name,
            share_status,
            site_id: self.site_id,
            updated_at: timestamp(self.updated_at),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DashboardAccess {
    OwnedOrAccepted,
    PendingInvitation,
}

#[derive(Debug, FromRow)]
struct WidgetRow {
    chart_type: String,
    created_at: i64,
    display_config: Option<String>,
    id: String,
    query_config: String,
    title: String,
    updated_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct StoredWidgetConfig {
    #[serde(default)]
    date_range: Option<WidgetDateRange>,
    #[serde(default = "default_breakdown")]
    breakdown: String,
    comparison: String,
    filters: Value,
    funnel: Option<FunnelDefinition>,
    kind: String,
    source: Option<WidgetSource>,
}

pub(crate) async fn list_dashboards(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Json<Vec<DashboardSummary>>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Ok(Json(Vec::new()));
    }
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    let rows = dashboard_rows(&state.sqlite, &site.id, &session.id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| row.into_summary(&session.id))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

pub(crate) async fn create_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<CreateDashboardRequest>,
) -> Result<Json<DashboardDetail>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    let name = normalize_name(&request.name, "Dashboard name")?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();
    let mut transaction = state.sqlite.begin().await?;
    if request.is_default {
        sqlx::query("UPDATE dashboards SET is_default = 0 WHERE site_id = ? AND created_by = ?")
            .bind(&site.id)
            .bind(&session.id)
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query(
        r#"
        INSERT INTO dashboards (id, site_id, name, layout, is_default, created_by, created_at, updated_at)
        VALUES (?, ?, ?, '[]', ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&site.id)
    .bind(name)
    .bind(request.is_default)
    .bind(&session.id)
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    load_dashboard_detail(
        &state.sqlite,
        &site.id,
        &id,
        &session.id,
        DashboardAccess::OwnedOrAccepted,
    )
    .await
    .map(Json)
}

pub(crate) async fn get_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
) -> Result<Json<DashboardDetail>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Err(ApiError::BadRequest("Unknown dashboard".to_owned()));
    }
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    load_dashboard_detail(
        &state.sqlite,
        &site.id,
        &dashboard_id,
        &session.id,
        DashboardAccess::OwnedOrAccepted,
    )
    .await
    .map(Json)
}

pub(crate) async fn update_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
    Json(request): Json<UpdateDashboardRequest>,
) -> Result<Json<DashboardDetail>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    require_dashboard_owner(&state.sqlite, &site.id, &dashboard_id, &session.id).await?;
    if request.name.is_none() && request.is_default.is_none() {
        return Err(ApiError::BadRequest(
            "At least one dashboard field must be provided".to_owned(),
        ));
    }
    let name = request
        .name
        .as_deref()
        .map(|name| normalize_name(name, "Dashboard name"))
        .transpose()?;
    let mut transaction = state.sqlite.begin().await?;
    if request.is_default == Some(true) {
        sqlx::query("UPDATE dashboards SET is_default = 0 WHERE site_id = ? AND created_by = ?")
            .bind(&site.id)
            .bind(&session.id)
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query(
        r#"
        UPDATE dashboards
        SET name = COALESCE(?, name), is_default = COALESCE(?, is_default), updated_at = ?
        WHERE id = ? AND site_id = ? AND created_by = ?
        "#,
    )
    .bind(name)
    .bind(request.is_default)
    .bind(Utc::now().timestamp())
    .bind(&dashboard_id)
    .bind(&site.id)
    .bind(&session.id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    load_dashboard_detail(
        &state.sqlite,
        &site.id,
        &dashboard_id,
        &session.id,
        DashboardAccess::OwnedOrAccepted,
    )
    .await
    .map(Json)
}

pub(crate) async fn delete_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    require_dashboard_owner(&state.sqlite, &site.id, &dashboard_id, &session.id).await?;
    sqlx::query("DELETE FROM dashboards WHERE id = ? AND site_id = ? AND created_by = ?")
        .bind(&dashboard_id)
        .bind(&site.id)
        .bind(&session.id)
        .execute(&state.sqlite)
        .await?;
    Ok(Json(json!({
        "dashboard_id": dashboard_id,
        "deleted": true,
        "scope": "all"
    })))
}

pub(crate) async fn create_widget(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
    Json(request): Json<WidgetRequest>,
) -> Result<Json<DashboardWidget>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    let creator =
        require_dashboard_owner(&state.sqlite, &site.id, &dashboard_id, &session.id).await?;
    let validated = validate_widget(request)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();
    let config = stored_widget_config(&validated);
    let display = serde_json::to_string(&validated.display)?;
    let width = if validated.display.size == "wide" {
        2
    } else {
        1
    };
    sqlx::query(
        r#"
        INSERT INTO dashboard_charts (
            id, dashboard_id, chart_type, title, query_config, display_config,
            position_x, position_y, width, height, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, 1, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&dashboard_id)
    .bind(&validated.visualization)
    .bind(&validated.title)
    .bind(serde_json::to_string(&config)?)
    .bind(display)
    .bind(validated.display.order)
    .bind(width)
    .bind(now)
    .bind(now)
    .execute(&state.sqlite)
    .await?;
    load_widget(&state.sqlite, &dashboard_id, &id, creator)
        .await
        .map(Json)
}

pub(crate) async fn update_widget(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id, widget_id)): Path<(String, String, String)>,
    Json(request): Json<WidgetRequest>,
) -> Result<Json<DashboardWidget>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    let creator =
        require_dashboard_owner(&state.sqlite, &site.id, &dashboard_id, &session.id).await?;
    let validated = validate_widget(request)?;
    let width = if validated.display.size == "wide" {
        2
    } else {
        1
    };
    let result = sqlx::query(
        r#"
        UPDATE dashboard_charts
        SET chart_type = ?, title = ?, query_config = ?, display_config = ?,
            position_y = ?, width = ?, updated_at = ?
        WHERE id = ? AND dashboard_id = ?
        "#,
    )
    .bind(&validated.visualization)
    .bind(&validated.title)
    .bind(serde_json::to_string(&stored_widget_config(&validated))?)
    .bind(serde_json::to_string(&validated.display)?)
    .bind(validated.display.order)
    .bind(width)
    .bind(Utc::now().timestamp())
    .bind(&widget_id)
    .bind(&dashboard_id)
    .execute(&state.sqlite)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest("Unknown dashboard widget".to_owned()));
    }
    load_widget(&state.sqlite, &dashboard_id, &widget_id, creator)
        .await
        .map(Json)
}

pub(crate) async fn delete_widget(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id, widget_id)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    require_dashboard_owner(&state.sqlite, &site.id, &dashboard_id, &session.id).await?;
    let result = sqlx::query("DELETE FROM dashboard_charts WHERE id = ? AND dashboard_id = ?")
        .bind(widget_id)
        .bind(dashboard_id)
        .execute(&state.sqlite)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest("Unknown dashboard widget".to_owned()));
    }
    Ok(Json(json!({ "status": "deleted" })))
}

pub(crate) async fn preview_widget(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
    Json(request): Json<WidgetRequest>,
) -> Result<Json<WidgetPreviewResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    let request = validate_widget(request)?;
    if let Some(funnel) = request.funnel.as_ref() {
        validate_previewable_funnel(funnel)?;
    }
    let preview = if auth::is_demo_session(&session) {
        let site = demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        demo_widget_preview(site, &request)
    } else {
        let site =
            sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
        require_dashboard_owner(&state.sqlite, &site.id, &dashboard_id, &session.id).await?;
        live_widget_preview(&state, &site, &request).await?
    };
    Ok(Json(preview))
}

/// Preview a draft without persisting it. Hosted tenants require an admin role
/// because a preview can execute an expensive multi-step ClickHouse query;
/// the bounded public demo continues to use static fixtures.
pub(crate) async fn preview_draft_widget(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<WidgetRequest>,
) -> Result<Json<WidgetPreviewResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    let request = validate_widget(request)?;
    if let Some(funnel) = request.funnel.as_ref() {
        validate_previewable_funnel(funnel)?;
    }

    let preview = if auth::is_demo_session(&session) {
        let site = demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        demo_widget_preview(site, &request)
    } else {
        let site =
            sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
        live_widget_preview(&state, &site, &request).await?
    };
    Ok(Json(preview))
}

async fn dashboard_rows(
    pool: &SqlitePool,
    site_id: &str,
    user_id: &str,
) -> Result<Vec<DashboardRow>, ApiError> {
    Ok(sqlx::query_as::<_, DashboardRow>(
        r#"
        SELECT
            dashboards.id,
            dashboards.site_id,
            dashboards.name,
            dashboards.is_default,
            dashboards.created_at,
            dashboards.updated_at,
            users.id AS creator_id,
            users.email AS creator_email,
            users.name AS creator_name,
            CASE
                WHEN dashboards.created_by = ? THEN NULL
                ELSE dashboard_shares.status
            END AS share_status
        FROM dashboards
        JOIN users ON users.id = dashboards.created_by
        LEFT JOIN dashboard_shares
            ON dashboard_shares.dashboard_id = dashboards.id
            AND dashboard_shares.site_id = dashboards.site_id
            AND dashboard_shares.user_id = ?
        WHERE dashboards.site_id = ?
            AND (
                dashboards.created_by = ?
                OR dashboard_shares.status = 'accepted'
            )
        ORDER BY dashboards.is_default DESC, dashboards.updated_at DESC, dashboards.id ASC
        "#,
    )
    .bind(user_id)
    .bind(user_id)
    .bind(site_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

pub(crate) async fn pending_invitation_summaries(
    pool: &SqlitePool,
    site_id: &str,
    user_id: &str,
) -> Result<Vec<DashboardSummary>, ApiError> {
    let rows = sqlx::query_as::<_, DashboardRow>(
        r#"
        SELECT
            dashboards.id,
            dashboards.site_id,
            dashboards.name,
            dashboards.is_default,
            dashboards.created_at,
            dashboards.updated_at,
            users.id AS creator_id,
            users.email AS creator_email,
            users.name AS creator_name,
            dashboard_shares.status AS share_status
        FROM dashboard_shares
        JOIN dashboards ON dashboards.id = dashboard_shares.dashboard_id
            AND dashboards.site_id = dashboard_shares.site_id
        JOIN users ON users.id = dashboards.created_by
        JOIN site_memberships ON site_memberships.site_id = dashboard_shares.site_id
            AND site_memberships.user_id = dashboard_shares.user_id
        WHERE dashboard_shares.site_id = ?
            AND dashboard_shares.user_id = ?
            AND dashboard_shares.status = 'pending'
        ORDER BY dashboard_shares.updated_at DESC, dashboards.id ASC
        "#,
    )
    .bind(site_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| row.into_summary(user_id))
        .collect()
}

pub(crate) async fn load_dashboard_detail(
    pool: &SqlitePool,
    site_id: &str,
    dashboard_id: &str,
    current_user_id: &str,
    access: DashboardAccess,
) -> Result<DashboardDetail, ApiError> {
    let row = match access {
        DashboardAccess::OwnedOrAccepted => {
            sqlx::query_as::<_, DashboardRow>(
                r#"
            SELECT
                dashboards.id,
                dashboards.site_id,
                dashboards.name,
                dashboards.is_default,
                dashboards.created_at,
                dashboards.updated_at,
                users.id AS creator_id,
                users.email AS creator_email,
                users.name AS creator_name,
                CASE
                    WHEN dashboards.created_by = ? THEN NULL
                    ELSE dashboard_shares.status
                END AS share_status
            FROM dashboards
            JOIN users ON users.id = dashboards.created_by
            LEFT JOIN dashboard_shares
                ON dashboard_shares.dashboard_id = dashboards.id
                AND dashboard_shares.site_id = dashboards.site_id
                AND dashboard_shares.user_id = ?
            WHERE dashboards.id = ?
                AND dashboards.site_id = ?
                AND (
                    dashboards.created_by = ?
                    OR dashboard_shares.status = 'accepted'
                )
            LIMIT 1
            "#,
            )
            .bind(current_user_id)
            .bind(current_user_id)
            .bind(dashboard_id)
            .bind(site_id)
            .bind(current_user_id)
            .fetch_optional(pool)
            .await?
        }
        DashboardAccess::PendingInvitation => {
            sqlx::query_as::<_, DashboardRow>(
                r#"
            SELECT
                dashboards.id,
                dashboards.site_id,
                dashboards.name,
                dashboards.is_default,
                dashboards.created_at,
                dashboards.updated_at,
                users.id AS creator_id,
                users.email AS creator_email,
                users.name AS creator_name,
                dashboard_shares.status AS share_status
            FROM dashboard_shares
            JOIN dashboards ON dashboards.id = dashboard_shares.dashboard_id
                AND dashboards.site_id = dashboard_shares.site_id
            JOIN users ON users.id = dashboards.created_by
            JOIN site_memberships ON site_memberships.site_id = dashboard_shares.site_id
                AND site_memberships.user_id = dashboard_shares.user_id
            WHERE dashboard_shares.dashboard_id = ?
                AND dashboard_shares.site_id = ?
                AND dashboard_shares.user_id = ?
                AND dashboard_shares.status = 'pending'
            LIMIT 1
            "#,
            )
            .bind(dashboard_id)
            .bind(site_id)
            .bind(current_user_id)
            .fetch_optional(pool)
            .await?
        }
    }
    .ok_or_else(|| ApiError::BadRequest("Unknown dashboard".to_owned()))?;
    let is_creator = row.creator_id == current_user_id;
    let summary = row.into_summary(current_user_id)?;
    let creator = summary.created_by.clone();
    let rows = sqlx::query_as::<_, WidgetRow>(
        r#"
        SELECT id, chart_type, title, query_config, display_config, created_at, updated_at
        FROM dashboard_charts
        WHERE dashboard_id = ?
        ORDER BY position_y ASC, created_at ASC, id ASC
        "#,
    )
    .bind(dashboard_id)
    .fetch_all(pool)
    .await?;
    let widgets = rows
        .into_iter()
        .map(|row| decode_widget(row, creator.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let shares = if is_creator {
        dashboard_sharing::load_dashboard_shares(pool, site_id, dashboard_id).await?
    } else {
        Vec::new()
    };
    Ok(DashboardDetail {
        summary,
        shares,
        widgets,
    })
}

async fn require_dashboard_owner(
    pool: &SqlitePool,
    site_id: &str,
    dashboard_id: &str,
    user_id: &str,
) -> Result<DashboardUser, ApiError> {
    sqlx::query_as::<_, DashboardRow>(
        r#"
        SELECT dashboards.id, dashboards.site_id, dashboards.name, dashboards.is_default,
            dashboards.created_at, dashboards.updated_at,
            users.id AS creator_id, users.email AS creator_email, users.name AS creator_name,
            NULL AS share_status
        FROM dashboards JOIN users ON users.id = dashboards.created_by
        WHERE dashboards.id = ? AND dashboards.site_id = ? AND dashboards.created_by = ?
        LIMIT 1
        "#,
    )
    .bind(dashboard_id)
    .bind(site_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .map(|row| DashboardUser {
        email: row.creator_email,
        id: row.creator_id,
        name: row.creator_name,
    })
    .ok_or_else(|| ApiError::Forbidden("Only the dashboard creator can edit it".to_owned()))
}

pub(crate) async fn ensure_dashboard_owner(
    pool: &SqlitePool,
    site_id: &str,
    dashboard_id: &str,
    user_id: &str,
) -> Result<(), ApiError> {
    require_dashboard_owner(pool, site_id, dashboard_id, user_id)
        .await
        .map(|_| ())
}

async fn load_widget(
    pool: &SqlitePool,
    dashboard_id: &str,
    widget_id: &str,
    creator: DashboardUser,
) -> Result<DashboardWidget, ApiError> {
    let row = sqlx::query_as::<_, WidgetRow>(
        r#"
        SELECT id, chart_type, title, query_config, display_config, created_at, updated_at
        FROM dashboard_charts WHERE id = ? AND dashboard_id = ? LIMIT 1
        "#,
    )
    .bind(widget_id)
    .bind(dashboard_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::BadRequest("Unknown dashboard widget".to_owned()))?;
    decode_widget(row, creator)
}

fn stored_widget_config(request: &WidgetRequest) -> StoredWidgetConfig {
    StoredWidgetConfig {
        date_range: request.date_range.clone(),
        breakdown: request.breakdown.clone(),
        comparison: request.comparison.clone(),
        filters: request.filters.clone(),
        funnel: request.funnel.clone(),
        kind: request.kind.clone(),
        source: request.source.clone(),
    }
}

fn decode_widget(row: WidgetRow, created_by: DashboardUser) -> Result<DashboardWidget, ApiError> {
    let config = serde_json::from_str::<StoredWidgetConfig>(&row.query_config)?;
    let display = row
        .display_config
        .as_deref()
        .map(serde_json::from_str::<WidgetDisplay>)
        .transpose()?
        .unwrap_or_default();
    Ok(DashboardWidget {
        date_range: config.date_range,
        breakdown: config.breakdown,
        comparison: config.comparison,
        created_at: timestamp(row.created_at),
        created_by,
        display,
        filters: config.filters,
        funnel: config.funnel,
        id: row.id,
        kind: config.kind,
        source: config.source,
        title: row.title,
        updated_at: timestamp(row.updated_at),
        visualization: row.chart_type,
    })
}

#[derive(Clone)]
struct FunnelPeriodRange {
    end: DateTime<Utc>,
    label: String,
    start: DateTime<Utc>,
}

struct FunnelCountMetrics {
    average_time_to_next_ms: Vec<Option<u64>>,
    counts: Vec<u64>,
    median_time_to_next_ms: Vec<Option<u64>>,
}

#[derive(Debug, Deserialize)]
struct ChartBucketRow {
    bucket: i64,
    value: u64,
}

#[derive(Debug, Deserialize)]
struct MapBucketRow {
    country: String,
    value: u64,
}

async fn live_widget_preview(
    state: &AppState,
    site: &ActiveSite,
    request: &WidgetRequest,
) -> Result<WidgetPreviewResponse, ApiError> {
    if let Some(funnel) = request.funnel.as_ref() {
        live_funnel_preview(state, site, funnel, request).await
    } else {
        live_chart_preview(state, site, request).await
    }
}

async fn live_funnel_preview(
    state: &AppState,
    site: &ActiveSite,
    funnel: &FunnelDefinition,
    request: &WidgetRequest,
) -> Result<WidgetPreviewResponse, ApiError> {
    let (current_range, previous_range, granularity) = widget_ranges(request, Utc::now())?;
    let (current_counts, previous) = if let Some(range) = previous_range {
        let (current_counts, previous_counts) = tokio::try_join!(
            query_funnel_counts(state, site, funnel, &current_range),
            query_funnel_counts(state, site, funnel, &range),
        )?;
        (
            current_counts,
            Some(funnel_period(funnel, range.label, previous_counts)),
        )
    } else {
        (
            query_funnel_counts(state, site, funnel, &current_range).await?,
            None,
        )
    };
    let current = funnel_period(funnel, current_range.label, current_counts);
    Ok(WidgetPreviewResponse {
        current,
        granularity,
        map: None,
        previous,
    })
}

async fn live_chart_preview(
    state: &AppState,
    site: &ActiveSite,
    request: &WidgetRequest,
) -> Result<WidgetPreviewResponse, ApiError> {
    let Some(source) = request.source.as_ref() else {
        return Err(ApiError::BadRequest(
            "Chart widgets require a source".to_owned(),
        ));
    };
    let (current_range, previous_range, granularity) = widget_ranges(request, Utc::now())?;

    if request.visualization == "map" {
        let rows = query_map_points(state, site, source, &request.filters, &current_range).await?;
        let previous = if let Some(range) = previous_range {
            let rows = query_map_points(state, site, source, &request.filters, &range).await?;
            Some(PreviewPeriod {
                label: range.label,
                steps: None,
                points: Some(
                    rows.into_iter()
                        .map(|row| ChartPreviewPoint {
                            period: row.country,
                            value: row.value,
                        })
                        .collect(),
                ),
            })
        } else {
            None
        };
        return Ok(WidgetPreviewResponse {
            current: PreviewPeriod {
                label: current_range.label,
                points: None,
                steps: None,
            },
            granularity,
            map: Some(
                rows.into_iter()
                    .map(|row| MapPreviewPoint {
                        country_code: row.country.clone(),
                        name: row.country,
                        value: row.value,
                    })
                    .collect(),
            ),
            previous,
        });
    }

    if request.breakdown != "time" {
        #[derive(Deserialize)]
        struct CategoryRow {
            group_label: String,
            current_value: u64,
            previous_value: u64,
        }
        let sql = query::category_query(
            &site.tracking_id,
            source,
            &request.filters,
            &request.breakdown,
            &current_range,
            previous_range.as_ref(),
        );
        let rows = state
            .clickhouse
            .query_json_each_row::<CategoryRow>(&sql)
            .await?;
        let points = |prior: bool| {
            rows.iter()
                .map(|row| ChartPreviewPoint {
                    period: row.group_label.clone(),
                    value: if prior {
                        row.previous_value
                    } else {
                        row.current_value
                    },
                })
                .collect()
        };
        return Ok(WidgetPreviewResponse {
            current: PreviewPeriod {
                label: current_range.label,
                points: Some(points(false)),
                steps: None,
            },
            previous: previous_range.map(|range| PreviewPeriod {
                label: range.label,
                points: Some(points(true)),
                steps: None,
            }),
            map: None,
            granularity,
        });
    }

    let aligned_labels = previous_range.is_some();
    let (current, previous) = if let Some(range) = previous_range {
        let (current, previous) = tokio::try_join!(
            query_chart_period(
                state,
                site,
                source,
                &request.filters,
                current_range,
                granularity,
                aligned_labels,
            ),
            query_chart_period(
                state,
                site,
                source,
                &request.filters,
                range,
                granularity,
                aligned_labels,
            ),
        )?;
        (current, Some(previous))
    } else {
        (
            query_chart_period(
                state,
                site,
                source,
                &request.filters,
                current_range,
                granularity,
                aligned_labels,
            )
            .await?,
            None,
        )
    };
    Ok(WidgetPreviewResponse {
        current,
        granularity,
        map: None,
        previous,
    })
}

async fn query_chart_period(
    state: &AppState,
    site: &ActiveSite,
    source: &WidgetSource,
    filters: &Value,
    range: FunnelPeriodRange,
    granularity: &str,
    aligned_labels: bool,
) -> Result<PreviewPeriod, ApiError> {
    let bucket_seconds = if granularity == "hour" { 3600 } else { 86400 };
    let conditions = chart_conditions(&site.tracking_id, source, filters, &range);
    let start = clickhouse_datetime(range.start);
    let sql = format!(
        r#"
        SELECT
            toInt64(intDiv(dateDiff('second', {start}, occurred_at), {bucket_seconds})) AS bucket,
            toUInt64(count()) AS value
        FROM owleye_events
        WHERE {conditions}
        GROUP BY bucket
        ORDER BY bucket ASC
        SETTINGS output_format_json_quote_64bit_integers = 0
        "#
    );
    let rows = state
        .clickhouse
        .query_json_each_row::<ChartBucketRow>(&sql)
        .await?;
    let values = rows
        .into_iter()
        .filter_map(|row| {
            usize::try_from(row.bucket)
                .ok()
                .map(|bucket| (bucket, row.value))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let buckets = preview_bucket_count(&range, granularity);
    let points = (0..buckets)
        .map(|bucket| ChartPreviewPoint {
            period: preview_bucket_label(&range, granularity, bucket, aligned_labels),
            value: values.get(&bucket).copied().unwrap_or_default(),
        })
        .collect();
    Ok(PreviewPeriod {
        label: range.label,
        points: Some(points),
        steps: None,
    })
}

async fn query_map_points(
    state: &AppState,
    site: &ActiveSite,
    source: &WidgetSource,
    filters: &Value,
    range: &FunnelPeriodRange,
) -> Result<Vec<MapBucketRow>, ApiError> {
    let conditions = chart_conditions(&site.tracking_id, source, filters, range);
    let sql = format!(
        r#"
        SELECT country, toUInt64(count()) AS value
        FROM owleye_events
        WHERE {conditions} AND country != ''
        GROUP BY country
        ORDER BY value DESC, country ASC
        LIMIT 250
        SETTINGS output_format_json_quote_64bit_integers = 0
        "#
    );
    Ok(state.clickhouse.query_json_each_row(&sql).await?)
}

async fn query_funnel_counts(
    state: &AppState,
    site: &ActiveSite,
    funnel: &FunnelDefinition,
    period: &FunnelPeriodRange,
) -> Result<FunnelCountMetrics, ApiError> {
    let sql = funnel_query(&site.tracking_id, funnel, period, Utc::now());
    let rows = state.clickhouse.query_json_each_row::<Value>(&sql).await?;
    decode_funnel_counts(rows.first(), funnel.steps.len())
}

fn decode_funnel_counts(row: Option<&Value>, steps: usize) -> Result<FunnelCountMetrics, ApiError> {
    let read = |key: String| {
        row.and_then(|row| row.get(key))
            .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse::<u64>().ok()))
            .ok_or_else(|| ApiError::ServiceUnavailable("The funnel query returned unreadable results. Retry the preview; these are not zero conversion counts.".into()))
    };
    let counts = (1..=steps)
        .map(|index| read(format!("step_{index}_count")))
        .collect::<Result<Vec<_>, _>>()?;
    let average_time_to_next_ms = (1..steps)
        .map(|index| {
            read(format!("step_{index}_avg_ms")).map(|value| (counts[index] > 0).then_some(value))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let median_time_to_next_ms = (1..steps)
        .map(|index| {
            read(format!("step_{index}_median_ms"))
                .map(|value| (counts[index] > 0).then_some(value))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FunnelCountMetrics {
        average_time_to_next_ms,
        counts,
        median_time_to_next_ms,
    })
}

fn widget_ranges(
    request: &WidgetRequest,
    now: DateTime<Utc>,
) -> Result<(FunnelPeriodRange, Option<FunnelPeriodRange>, &'static str), ApiError> {
    let Some(range) = &request.date_range else {
        return preview_ranges(&request.comparison, now);
    };
    if !(1..=365).contains(&range.days) {
        return Err(ApiError::BadRequest(
            "Date range must be between 1 and 365 days".into(),
        ));
    }
    let span = Duration::days(i64::from(range.days));
    let label = if range.days == 1 {
        "Last 24 hours".into()
    } else {
        format!("Last {} days", range.days)
    };
    Ok((
        FunnelPeriodRange {
            start: now - span,
            end: now,
            label,
        },
        range.compare_previous.then(|| FunnelPeriodRange {
            start: now - span - span,
            end: now - span,
            label: if range.days == 1 {
                "Previous 24 hours".into()
            } else {
                format!("Previous {} days", range.days)
            },
        }),
        if range.days == 1 { "hour" } else { "day" },
    ))
}

fn preview_ranges(
    comparison: &str,
    now: DateTime<Utc>,
) -> Result<(FunnelPeriodRange, Option<FunnelPeriodRange>, &'static str), ApiError> {
    match comparison {
        "none" => Ok((
            FunnelPeriodRange {
                end: now,
                label: "Last 30 days".to_owned(),
                start: now - Duration::days(30),
            },
            None,
            "day",
        )),
        "today_vs_yesterday" => {
            let today = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
            Ok((
                FunnelPeriodRange {
                    end: now,
                    label: "Today".to_owned(),
                    start: today,
                },
                Some(FunnelPeriodRange {
                    end: today,
                    label: "Yesterday".to_owned(),
                    start: today - Duration::days(1),
                }),
                "hour",
            ))
        }
        "last_7_vs_previous_7" => Ok(rolling_preview_ranges(
            now,
            7,
            "Last 7 days",
            "Previous 7 days",
        )),
        "last_30_vs_previous_30" => Ok(rolling_preview_ranges(
            now,
            30,
            "Last 30 days",
            "Previous 30 days",
        )),
        _ => Err(ApiError::BadRequest(
            "Unsupported widget comparison".to_owned(),
        )),
    }
}

fn rolling_preview_ranges(
    now: DateTime<Utc>,
    days: i64,
    current_label: &str,
    previous_label: &str,
) -> (FunnelPeriodRange, Option<FunnelPeriodRange>, &'static str) {
    let boundary = now - Duration::days(days);
    (
        FunnelPeriodRange {
            end: now,
            label: current_label.to_owned(),
            start: boundary,
        },
        Some(FunnelPeriodRange {
            end: boundary,
            label: previous_label.to_owned(),
            start: boundary - Duration::days(days),
        }),
        "day",
    )
}

fn funnel_period(
    funnel: &FunnelDefinition,
    label: String,
    metrics: FunnelCountMetrics,
) -> PreviewPeriod {
    let counts = metrics.counts;
    let first = counts.first().copied().unwrap_or_default();
    let steps = funnel
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let count = counts.get(index).copied().unwrap_or_default();
            let previous = index
                .checked_sub(1)
                .and_then(|index| counts.get(index).copied())
                .unwrap_or(count);
            let dropoff_count = previous.saturating_sub(count);
            FunnelPreviewStep {
                avg_time_to_next_ms: metrics
                    .average_time_to_next_ms
                    .get(index)
                    .copied()
                    .flatten(),
                count,
                dropoff_count,
                dropoff_rate: percentage(dropoff_count, previous),
                id: step.id.clone(),
                name: step.name.clone(),
                overall_conversion: percentage(count, first),
                step_conversion: if index == 0 {
                    if count == 0 {
                        0.0
                    } else {
                        100.0
                    }
                } else {
                    percentage(count, previous)
                },
                median_time_to_next_ms: metrics
                    .median_time_to_next_ms
                    .get(index)
                    .copied()
                    .flatten(),
            }
        })
        .collect();
    PreviewPeriod {
        label,
        points: None,
        steps: Some(steps),
    }
}

fn percentage(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    ((numerator as f64 / denominator as f64) * 10_000.0).round() / 100.0
}

fn demo_widget_preview(
    site: &demo::DemoSiteDefinition,
    request: &WidgetRequest,
) -> WidgetPreviewResponse {
    if let Some(funnel) = request.funnel.as_ref() {
        demo_funnel_preview(site, funnel, request)
    } else {
        demo_chart_preview(site, request)
    }
}

fn demo_funnel_preview(
    site: &demo::DemoSiteDefinition,
    funnel: &FunnelDefinition,
    request: &WidgetRequest,
) -> WidgetPreviewResponse {
    let base = match site.profile {
        demo::DemoTrafficProfile::Massive => 284_310,
        demo::DemoTrafficProfile::Campaign => 91_720,
        demo::DemoTrafficProfile::Mixed => 44_105,
        demo::DemoTrafficProfile::LowVolume => 1_382,
        demo::DemoTrafficProfile::New => 137,
    };
    let counts = demo_funnel_counts(base, funnel.steps.len());
    let (current_range, previous_range, granularity) =
        widget_ranges(request, Utc::now()).expect("validated comparison");
    WidgetPreviewResponse {
        current: funnel_period(funnel, current_range.label, counts),
        granularity,
        map: None,
        previous: previous_range.map(|range| {
            funnel_period(
                funnel,
                range.label,
                demo_funnel_counts(base.saturating_mul(87) / 100, funnel.steps.len()),
            )
        }),
    }
}

fn demo_chart_preview(
    site: &demo::DemoSiteDefinition,
    request: &WidgetRequest,
) -> WidgetPreviewResponse {
    let (current_range, previous_range, granularity) =
        widget_ranges(request, Utc::now()).expect("validated comparison");
    let source = request
        .source
        .as_ref()
        .expect("validated chart widgets have a source");
    if request.visualization == "map" {
        let base = if source.kind == "future_event" {
            0
        } else {
            demo_chart_base(site, source)
        };
        let countries = [
            ("US", 31_u64),
            ("IN", 24),
            ("GB", 15),
            ("DE", 11),
            ("AU", 8),
            ("CA", 6),
            ("SG", 5),
        ];
        return WidgetPreviewResponse {
            current: PreviewPeriod {
                label: current_range.label,
                points: None,
                steps: None,
            },
            granularity,
            map: Some(
                countries
                    .into_iter()
                    .filter_map(|(country, share)| {
                        let value = base.saturating_mul(share) / 100;
                        (value > 0).then(|| MapPreviewPoint {
                            country_code: country.to_owned(),
                            name: country.to_owned(),
                            value,
                        })
                    })
                    .collect(),
            ),
            previous: previous_range.map(|range| PreviewPeriod {
                label: range.label,
                points: Some(
                    countries
                        .into_iter()
                        .filter_map(|(country, share)| {
                            let value = base.saturating_mul(share) * 84 / 10000;
                            (value > 0).then(|| ChartPreviewPoint {
                                period: country.into(),
                                value,
                            })
                        })
                        .collect(),
                ),
                steps: None,
            }),
        };
    }
    if request.breakdown != "time" {
        let labels: &[&str] = match request.breakdown.as_str() {
            "browser" => &["Chrome", "Safari", "Firefox"],
            "country" => &["US", "IN", "GB"],
            "city" => &["Mumbai (IN)", "New York (US)", "London (GB)"],
            "os" => &["Windows", "macOS", "Android"],
            "device" => &["Desktop", "Mobile", "Tablet"],
            "campaign" => &["spring-launch", "summer-campaign", "No campaign"],
            "source" => &["newsletter", "google", "No source"],
            "medium" => &["email", "cpc", "No medium"],
            "referrer" => &["Direct", "google.com", "example.com"],
            "page" => &["/", "/pricing", "/signup"],
            _ => unreachable!("validated breakdown"),
        };
        let period = |range: FunnelPeriodRange, factor: u64| PreviewPeriod {
            label: range.label,
            points: Some(
                labels
                    .iter()
                    .enumerate()
                    .filter_map(|(index, label)| {
                        let value = if source.kind == "future_event" {
                            0
                        } else {
                            demo_chart_base(site, source) * factor / (100 * (index as u64 + 2))
                        };
                        (value > 0).then(|| ChartPreviewPoint {
                            period: (*label).into(),
                            value,
                        })
                    })
                    .collect(),
            ),
            steps: None,
        };
        return WidgetPreviewResponse {
            current: period(current_range, 100),
            previous: previous_range.map(|range| period(range, 84)),
            granularity,
            map: None,
        };
    }
    let aligned = previous_range.is_some();
    let current = demo_chart_period(site, source, current_range, granularity, aligned, 100);
    let previous = previous_range
        .map(|range| demo_chart_period(site, source, range, granularity, aligned, 84));
    WidgetPreviewResponse {
        current,
        granularity,
        map: None,
        previous,
    }
}

fn demo_chart_period(
    site: &demo::DemoSiteDefinition,
    source: &WidgetSource,
    range: FunnelPeriodRange,
    granularity: &str,
    aligned: bool,
    factor: u64,
) -> PreviewPeriod {
    let buckets = preview_bucket_count(&range, granularity);
    let base = demo_chart_base(site, source).saturating_mul(factor) / 100;
    let per_bucket = base.checked_div(buckets as u64).unwrap_or_default();
    let salt = source
        .id
        .bytes()
        .fold(0_u64, |total, byte| total.wrapping_add(byte as u64));
    let points = (0..buckets)
        .map(|bucket| {
            let wobble = 78 + ((bucket as u64 * 17 + salt) % 47);
            ChartPreviewPoint {
                period: preview_bucket_label(&range, granularity, bucket, aligned),
                value: if source.kind == "future_event" {
                    0
                } else {
                    per_bucket.saturating_mul(wobble) / 100
                },
            }
        })
        .collect();
    PreviewPeriod {
        label: range.label,
        points: Some(points),
        steps: None,
    }
}

fn demo_chart_base(site: &demo::DemoSiteDefinition, source: &WidgetSource) -> u64 {
    let traffic = match site.profile {
        demo::DemoTrafficProfile::Massive => 1_914_750_u64,
        demo::DemoTrafficProfile::Campaign => 824_310,
        demo::DemoTrafficProfile::Mixed => 186_710,
        demo::DemoTrafficProfile::LowVolume => 8_460,
        demo::DemoTrafficProfile::New => 724,
    };
    let weight = 7 + source
        .id
        .bytes()
        .fold(0_u64, |total, byte| total.wrapping_add(byte as u64))
        % 21;
    traffic.saturating_mul(weight) / 100
}

fn demo_funnel_counts(first: u64, steps: usize) -> FunnelCountMetrics {
    let counts = (0..steps)
        .scan(first, |count, index| {
            if index > 0 {
                let retention = 72_u64.saturating_sub(index as u64 * 5).max(30);
                *count = count.saturating_mul(retention) / 100;
            }
            Some(*count)
        })
        .collect();
    FunnelCountMetrics {
        average_time_to_next_ms: (1..steps)
            .map(|index| Some(index as u64 * 3_600_000))
            .collect(),
        counts,
        median_time_to_next_ms: (1..steps)
            .map(|index| Some(index as u64 * 2_400_000))
            .collect(),
    }
}

fn timestamp(value: i64) -> String {
    chrono::DateTime::from_timestamp(value, 0)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn default_comparison() -> String {
    "none".to_owned()
}

fn default_operator() -> String {
    "exact".to_owned()
}

fn default_entry_mode() -> String {
    "closed".to_owned()
}

fn default_order_mode() -> String {
    "ordered".to_owned()
}

fn empty_object() -> Value {
    json!({})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_contract_supports_charts_comparisons_and_funnels() {
        let chart: WidgetRequest = serde_json::from_value(json!({
            "kind": "chart",
            "title": "Signups",
            "source": { "kind": "event", "id": "signup", "name": "Signup", "has_location": false },
            "visualization": "line",
            "comparison": "last_7_vs_previous_7",
            "filters": {},
            "display": { "order": 0, "size": "wide" }
        }))
        .unwrap();
        assert!(validate_widget(chart).is_ok());

        let funnel: WidgetRequest = serde_json::from_value(json!({
            "kind": "funnel",
            "title": "Activation",
            "visualization": "funnel",
            "funnel": {
                "conversion_window": "7d",
                "entry_mode": "closed",
                "order_mode": "ordered",
                "steps": [
                    {
                        "id": "signup",
                        "name": "Signup",
                        "condition": {
                            "id": "signup",
                            "kind": "event",
                            "name": "Signup",
                            "property_filters": {
                                "filters": [{
                                    "id": "plan",
                                    "key": "plan",
                                    "operator": "equals",
                                    "value": "pro"
                                }],
                                "logic": "and"
                            }
                        }
                    },
                    {
                        "id": "activated",
                        "name": "Activated",
                        "condition": {
                            "id": "activated",
                            "kind": "event",
                            "name": "Activated"
                        }
                    }
                ]
            }
        }))
        .unwrap();
        assert!(validate_widget(funnel).is_ok());
    }

    #[test]
    fn closed_funnel_query_sequences_steps_and_escapes_filter_values() {
        let request: WidgetRequest = serde_json::from_value(json!({
            "kind": "funnel",
            "title": "Activation",
            "visualization": "funnel",
            "funnel": {
                "conversion_window": "1d",
                "entry_mode": "closed",
                "order_mode": "exact",
                "steps": [
                    {
                        "id": "first",
                        "name": "First",
                        "condition": {
                            "id": "signup",
                            "kind": "event",
                            "name": "Signup",
                            "property_filters": {
                                "filters": [{
                                    "id": "plan",
                                    "key": "plan",
                                    "operator": "equals",
                                    "value": "pro' OR 1 = 1"
                                }],
                                "logic": "and"
                            }
                        }
                    },
                    {
                        "id": "second",
                        "name": "Second",
                        "condition": {
                            "id": "/welcome",
                            "kind": "page",
                            "name": "Welcome"
                        }
                    }
                ]
            }
        }))
        .unwrap();
        let funnel = request.funnel.as_ref().unwrap();
        let now = Utc::now();
        let sql = funnel_query(
            "owl_test",
            funnel,
            &FunnelPeriodRange {
                end: now,
                label: "Now".to_owned(),
                start: now - Duration::days(1),
            },
            now,
        );
        assert!(sql.contains("events.sequence_index = previous.step_1_index + 1"));
        assert!(sql.contains("JSONExtractString"));
        assert!(sql.contains("pro\\' OR 1 = 1"));
        assert!(sql.contains(crate::retention::ACTIVE_ROW_PREDICATE));
    }

    #[test]
    fn funnel_decoding_accepts_clickhouse_counts_but_never_invents_zero() {
        let row = json!({"step_1_count":"12","step_2_count":5,"step_1_avg_ms":"800","step_1_median_ms":700});
        let result = decode_funnel_counts(Some(&row), 2).unwrap();
        assert_eq!(result.counts, vec![12, 5]);
        assert_eq!(result.average_time_to_next_ms, vec![Some(800)]);
        assert!(decode_funnel_counts(None, 2).is_err());
        for invalid in [Value::Null, json!("invalid"), json!(-1)] {
            let mut malformed = row.clone();
            malformed["step_1_count"] = invalid;
            assert!(decode_funnel_counts(Some(&malformed), 2).is_err());
        }
        let zero =
            json!({"step_1_count":0,"step_2_count":0,"step_1_avg_ms":0,"step_1_median_ms":0});
        let result = decode_funnel_counts(Some(&zero), 2).unwrap();
        assert_eq!(result.counts, vec![0, 0]);
        assert_eq!(result.average_time_to_next_ms, vec![None]);
    }

    #[tokio::test]
    #[ignore = "requires loopback ClickHouse; uses read-only synthetic rows"]
    async fn real_clickhouse_funnel_preview_counts_ordered_steps_and_empty_results() {
        let base = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("loopback ClickHouse URL");
        let mut url = reqwest::Url::parse(&base).unwrap();
        assert!(matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        ));
        url.query_pairs_mut()
            .append_pair("output_format_json_quote_64bit_integers", "1");
        let clickhouse = crate::storage::clickhouse::ClickHouse::new(url.to_string()).unwrap();
        let funnel: FunnelDefinition = serde_json::from_value(json!({
            "conversion_window":"same_session", "entry_mode":"closed", "order_mode":"ordered",
            "steps":[
                {"id":"one","name":"Landing","condition":{"kind":"page","id":"/","name":"Landing"}},
                {"id":"two","name":"CTA","condition":{"kind":"event","id":"cta","name":"CTA"}},
                {"id":"three","name":"Signup","condition":{"kind":"event","id":"signup","name":"Signup"}}
            ]
        })).unwrap();
        let source = r#"(SELECT 'owl_funnel_test' AS site_id, cohort AS anon_session_id, cohort AS visitor_id, toString(step) AS event_id, now64(3)-INTERVAL 1 HOUR+toIntervalMinute(step) AS occurred_at, now64(3)+INTERVAL 10 DAY AS retention_active_until, if(name='land','pageview','external') AS event_type, name AS event_name, '' AS rule_id, '/' AS url_path, '' AS utm_source, '' AS utm_medium, '' AS utm_campaign, '' AS country, '' AS region, '' AS browser_name, '' AS os_name, '' AS device_type, '{}' AS payload_json FROM values('cohort String, name String, step UInt8',('one','land',1),('one','cta',2),('one','signup',3),('two','land',1),('two','cta',2),('three','signup',1),('three','land',2),('three','cta',3)))"#;
        let now = Utc::now();
        let period = FunnelPeriodRange {
            start: now - Duration::days(30),
            end: now,
            label: "Last 30 days".into(),
        };
        for (site, expected) in [("owl_funnel_test", vec![3, 3, 1]), ("empty", vec![0, 0, 0])] {
            let query = funnel_query(site, &funnel, &period, now)
                .replace("FROM owleye_events", &format!("FROM {source}"));
            let rows = clickhouse
                .query_json_each_row::<Value>(&query)
                .await
                .unwrap();
            assert!(
                rows[0]["step_1_count"].is_u64(),
                "query must control JSON integer formatting"
            );
            assert_eq!(
                decode_funnel_counts(rows.first(), 3).unwrap().counts,
                expected
            );
        }
    }

    #[test]
    fn open_entry_funnels_are_persistable_but_not_previewed() {
        let funnel = FunnelDefinition {
            conversion_window: "1d".to_owned(),
            entry_mode: "open".to_owned(),
            order_mode: "ordered".to_owned(),
            steps: vec![
                FunnelStep {
                    condition: FunnelCondition {
                        id: "signup".to_owned(),
                        kind: "event".to_owned(),
                        name: "Signup".to_owned(),
                        operator: "exact".to_owned(),
                        property_filters: None,
                    },
                    id: "signup".to_owned(),
                    name: "Signup".to_owned(),
                },
                FunnelStep {
                    condition: FunnelCondition {
                        id: "activated".to_owned(),
                        kind: "event".to_owned(),
                        name: "Activated".to_owned(),
                        operator: "exact".to_owned(),
                        property_filters: None,
                    },
                    id: "activated".to_owned(),
                    name: "Activated".to_owned(),
                },
            ],
        };
        assert!(validate_funnel(&funnel).is_ok());
        assert!(validate_previewable_funnel(&funnel).is_err());
    }

    #[test]
    fn chart_ranges_breakdowns_and_persistence_are_independent() {
        let base = json!({"kind":"chart", "title":"Signups", "source":{"kind":"event","id":"signup","name":"Signup"}, "visualization":"bar"});
        let now: DateTime<Utc> = "2026-09-23T16:12:34Z".parse().unwrap();
        for days in [1, 7, 45, 90, 365] {
            for compare in [false, true] {
                let mut value = base.clone();
                value["date_range"] = json!({"days":days,"compare_previous":compare});
                value["breakdown"] = json!("campaign");
                let request = validate_widget(serde_json::from_value(value).unwrap()).unwrap();
                let (current, previous, granularity) = widget_ranges(&request, now).unwrap();
                assert_eq!(current.end - current.start, Duration::days(days));
                assert_eq!(previous.is_some(), compare);
                assert_eq!(granularity, if days == 1 { "hour" } else { "day" });
                if let Some(previous) = previous {
                    assert_eq!(previous.end, current.start);
                    assert_eq!(previous.end - previous.start, current.end - current.start);
                }
                let stored: StoredWidgetConfig = serde_json::from_value(
                    serde_json::to_value(stored_widget_config(&request)).unwrap(),
                )
                .unwrap();
                assert_eq!(stored.date_range.unwrap().days, days as u16);
                assert_eq!(stored.breakdown, "campaign");
            }
        }
        for (field, invalid) in [
            ("date_range", json!({"days":0})),
            ("date_range", json!({"days":366})),
            ("breakdown", json!("country; DROP TABLE")),
        ] {
            let mut value = base.clone();
            value[field] = invalid;
            assert!(validate_widget(serde_json::from_value(value).unwrap()).is_err());
        }
        let legacy: WidgetRequest = serde_json::from_value(base.clone()).unwrap();
        assert!(legacy.date_range.is_none());
        assert_eq!(
            widget_ranges(&legacy, now).unwrap().0.start,
            now - Duration::days(30)
        );
        let mut map = base;
        map["visualization"] = json!("map");
        assert_eq!(
            validate_widget(serde_json::from_value(map).unwrap())
                .unwrap()
                .breakdown,
            "country"
        );
    }

    #[tokio::test]
    #[ignore = "requires loopback ClickHouse; read-only synthetic rows"]
    async fn real_clickhouse_category_comparisons_preserve_scope_and_missing_groups() {
        let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("loopback URL");
        assert!(matches!(
            reqwest::Url::parse(&url).unwrap().host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        ));
        let clickhouse = crate::storage::clickhouse::ClickHouse::new(url).unwrap();
        let now = Utc::now();
        let current = FunnelPeriodRange {
            start: now - Duration::days(1),
            end: now,
            label: "current".into(),
        };
        let previous = FunnelPeriodRange {
            start: now - Duration::days(2),
            end: current.start,
            label: "previous".into(),
        };
        let source = WidgetSource {
            id: "signup".into(),
            kind: "event".into(),
            name: "Signup".into(),
            has_location: false,
        };
        let rows = r#"(SELECT site_id,event_name,city,country,utm_campaign, now64(3)-toIntervalHour(age) AS occurred_at, now64(3)+toIntervalDay(active) AS retention_active_until, 'external' AS event_type, '' AS rule_id, '/' AS url_path, '' AS referrer_host, '' AS browser_name, '' AS os_name, '' AS device_type, '' AS utm_source, '' AS utm_medium FROM values('site_id String,event_name String,city String,country String,utm_campaign String,age UInt8,active Int8',('test','signup','London','GB','',1,1),('test','signup','London','CA','old',25,1),('other','signup','London','GB','',1,1),('test','noise','London','GB','',1,1),('test','signup','London','GB','expired',1,-1)))"#;
        for dimension in [
            "browser", "country", "os", "city", "device", "campaign", "referrer", "source",
            "medium", "page",
        ] {
            let sql = query::category_query(
                "test",
                &source,
                &json!({}),
                dimension,
                &current,
                Some(&previous),
            )
            .replace("FROM owleye_events", &format!("FROM {rows}"));
            let result = clickhouse.query_json_each_row::<Value>(&sql).await.unwrap();
            assert_eq!(
                result
                    .iter()
                    .map(|r| r["current_value"].as_u64().unwrap())
                    .sum::<u64>(),
                1
            );
            assert_eq!(
                result
                    .iter()
                    .map(|r| r["previous_value"].as_u64().unwrap())
                    .sum::<u64>(),
                1
            );
            if dimension == "campaign" {
                assert!(result.iter().any(|r| r["group_label"] == "No campaign"
                    && r["current_value"] == 1
                    && r["previous_value"] == 0));
                assert!(result.iter().any(|r| r["group_label"] == "old"
                    && r["current_value"] == 0
                    && r["previous_value"] == 1));
            }
            if dimension == "city" {
                assert_eq!(result.len(), 2);
            }
            if dimension == "referrer" {
                assert_eq!(result[0]["group_label"], "Direct");
            }
        }
        for (site, filter, count) in [
            ("test", json!({}), 1),
            ("test", json!({"country":"CA"}), 0),
            ("empty", json!({}), 0),
        ] {
            let sql = query::category_query(site, &source, &filter, "campaign", &current, None)
                .replace("FROM owleye_events", &format!("FROM {rows}"));
            let result = clickhouse.query_json_each_row::<Value>(&sql).await.unwrap();
            assert_eq!(result.len(), count);
        }
    }

    #[test]
    fn chart_preview_contract_covers_points_maps_and_escaped_filters() {
        let chart: WidgetRequest = serde_json::from_value(json!({
            "kind": "chart",
            "title": "Signups",
            "source": {
                "kind": "event",
                "id": "signup' OR 1 = 1",
                "name": "Signup",
                "has_location": true
            },
            "visualization": "line",
            "filters": { "country": "US' OR 1 = 1", "page": "/pricing" }
        }))
        .unwrap();
        let now = Utc::now();
        let range = FunnelPeriodRange {
            end: now,
            label: "Last 30 days".to_owned(),
            start: now - Duration::days(30),
        };
        let conditions = chart_conditions(
            "owl_test",
            chart.source.as_ref().unwrap(),
            &chart.filters,
            &range,
        );
        assert!(conditions.contains("signup\\' OR 1 = 1"));
        assert!(conditions.contains("US\\' OR 1 = 1"));
        assert!(conditions.contains(crate::retention::ACTIVE_ROW_PREDICATE));

        let preview = demo_widget_preview(&demo::DEMO_SITES[0], &chart);
        assert_eq!(preview.current.points.as_ref().unwrap().len(), 30);

        let map = WidgetRequest {
            date_range: None,
            breakdown: "country".into(),
            visualization: "map".to_owned(),
            ..chart
        };
        let preview = demo_widget_preview(&demo::DEMO_SITES[0], &map);
        assert!(preview.map.is_some_and(|points| !points.is_empty()));
    }
}
