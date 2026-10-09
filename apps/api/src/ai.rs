mod insights;
mod provider;
mod report;
mod requests;
mod sanitize;
pub use provider::AiSettings;
pub(crate) use requests::TABLES;

use crate::{auth, demo, sites, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

const MAX_PROMPT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ConversationRole {
    User,
    Assistant,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConversationTurn {
    pub role: ConversationRole,
    pub text: String,
}
fn validate_context(context: &[ConversationTurn]) -> Result<(), ApiError> {
    if context.len() > 8
        || context.iter().map(|t| t.text.len()).sum::<usize>() > 16000
        || context
            .iter()
            .any(|t| t.text.trim().is_empty() || t.text.len() > 4000)
    {
        return Err(ApiError::BadRequest(
            "Clarification context is too long. Start a new question.".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AiModeResponse {
    provider_available: bool,
    can_manage: bool,
    can_use: bool,
    enabled: bool,
    site_id: String,
    unlimited: bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateAiSettingsRequest {
    enabled: bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptRequest {
    prompt: String,
    #[serde(default)]
    context: Vec<ConversationTurn>,
    #[serde(default)]
    request_id: Option<uuid::Uuid>,
}
#[derive(Debug, Serialize)]
pub(crate) struct PromptResponse {
    accepted: bool,
    answer: String,
    explanation_source: &'static str,
    evidence: report::Evidence,
    request_id: String,
}

pub(crate) async fn get_ai_mode(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Json<AiModeResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        let site = demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Ok(Json(demo_mode(site)));
    }
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    scoped_mode(&state, &site, &session.id).await.map(Json)
}

pub(crate) async fn update_ai_mode(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(identifier): Path<String>,
    Json(request): Json<UpdateAiSettingsRequest>,
) -> Result<Json<AiModeResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    let site = sites::resolve_site_for_owner(&state.sqlite, &session.id, &identifier).await?;
    sqlx::query("INSERT INTO site_settings(site_id,ai_enabled) VALUES(?,?) ON CONFLICT(site_id) DO UPDATE SET ai_enabled=excluded.ai_enabled").bind(&site.id).bind(request.enabled).execute(&state.sqlite).await?;
    scoped_mode(&state, &site, &session.id).await.map(Json)
}

pub(crate) async fn consume_prompt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<PromptRequest>,
) -> Result<(StatusCode, Json<PromptResponse>), ApiError> {
    validate_prompt(&request.prompt)?;
    validate_context(&request.context)?;
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Err(ApiError::Forbidden(
            "AI execution is unavailable in the demo".into(),
        ));
    }

    let response = run_prompt_with_context(
        &state,
        &site_identifier,
        &session.id,
        &request.prompt,
        request.request_id,
        &request.context,
    )
    .await?;
    Ok((StatusCode::OK, Json(response)))
}

pub(crate) async fn user_has_ai_page_access(
    pool: &SqlitePool,
    site: &sites::ActiveSite,
    user_id: &str,
) -> Result<bool, ApiError> {
    sites::effective_site_role(pool, user_id, site).await?;
    Ok(true)
}

pub(crate) async fn consume_developer_prompt(
    state: &AppState,
    site_identifier: &str,
    user_id: &str,
    prompt: &str,
    request_id: Option<uuid::Uuid>,
) -> Result<Json<PromptResponse>, ApiError> {
    validate_prompt(prompt)?;
    run_prompt(state, site_identifier, user_id, prompt, request_id)
        .await
        .map(Json)
}

async fn scoped_mode(
    state: &AppState,
    site: &sites::ActiveSite,
    user: &str,
) -> Result<AiModeResponse, ApiError> {
    requests::scope_on(&mut *state.sqlite.acquire().await?, site, user).await?;
    let mut response = load_ai_mode(&state.sqlite, site, user).await?;
    response.provider_available = state.settings.ai.available();
    response.can_use &= response.provider_available;
    Ok(response)
}

async fn run_prompt(
    state: &AppState,
    identifier: &str,
    user: &str,
    prompt: &str,
    request_id: Option<uuid::Uuid>,
) -> Result<PromptResponse, ApiError> {
    run_prompt_with_context(state, identifier, user, prompt, request_id, &[]).await
}

async fn run_prompt_with_context(
    state: &AppState,
    identifier: &str,
    user: &str,
    prompt: &str,
    request_id: Option<uuid::Uuid>,
    context: &[ConversationTurn],
) -> Result<PromptResponse, ApiError> {
    validate_context(context)?;
    use std::{sync::LazyLock, time::Duration};
    static CAPACITY: LazyLock<tokio::sync::Semaphore> =
        LazyLock::new(|| tokio::sync::Semaphore::new(8));
    let site = sites::resolve_site_for_user(&state.sqlite, user, identifier).await?;
    requests::scope_on(&mut *state.sqlite.acquire().await?, &site, user).await?;
    if !state.settings.ai.available() {
        return Err(ApiError::ServiceUnavailable(
            "AI is not configured yet".into(),
        ));
    }
    let _permit = CAPACITY
        .try_acquire()
        .map_err(|_| ApiError::TooManyRequests {
            retry_after_seconds: 30,
        })?;
    let id = request_id.unwrap_or_else(uuid::Uuid::new_v4).to_string();
    requests::reserve(&state.sqlite, &site, user, &id).await?;
    let prompt = sanitize::redact(prompt);
    let context: Vec<ConversationTurn> = context
        .iter()
        .map(|turn| ConversationTurn {
            role: turn.role.clone(),
            text: sanitize::redact(&turn.text),
        })
        .collect();
    let result = tokio::time::timeout(Duration::from_secs(85), async {
        let fence = state.clickhouse.ingestion_fence(&site.tracking_id);
        let guard = fence.read_owned().await;
        let catalog = insights::discover(state, &site.tracking_id).await?;
        let plan =
            provider::plan_with_catalog(&state.settings.ai, &prompt, &catalog, &context).await?;
        if plan.report == report::Report::Clarification {
            let answer = plan
                .clarification
                .clone()
                .unwrap_or_else(|| "Which event or location should I use?".into());
            let evidence = report::Evidence {
                report: "clarification".into(),
                start_date: String::new(),
                end_date: String::new(),
                timezone: "UTC",
                period_label: String::new(),
                groups_suppressed_below_visitors: 5,
                country: vec![],
                browser: vec![],
                device: vec![],
                os: vec![],
                chart: report::Chart::None,
                metric: report::Metric::Visitors,
                output: report::Output::Text,
                rows: vec![],
                details: Default::default(),
                derived: vec![],
            };
            return Ok((answer, "clarification", evidence, guard, false));
        }
        requests::scope_on(&mut *state.sqlite.acquire().await?, &site, user).await?;
        let mut evidence = report::execute(state, &site.tracking_id, plan).await?;
        if let Some(bounds) = catalog.entries.iter().find(|r|r.kind == "coverage") {
            evidence.details.notes.push(format!("The app's currently retained analytics has observed dates from {} through {}. These bounds do not guarantee continuous or complete coverage.", bounds.name, bounds.qualifier));
        }
        requests::scope_on(&mut *state.sqlite.acquire().await?, &site, user).await?;
        let question = context
            .iter()
            .filter(|turn| matches!(turn.role, ConversationRole::User))
            .map(|turn| turn.text.as_str())
            .chain(std::iter::once(prompt.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        let (answer, explanation_source) = provider::explain(&state.settings.ai, &question, &evidence).await?;
        Ok::<_, ApiError>((answer, explanation_source, evidence, guard, true))
    })
    .await
    .unwrap_or_else(|_| {
        Err(ApiError::ServiceUnavailable(
            "AI timed out".into(),
        ))
    });
    let (answer, explanation_source, evidence, _guard, accepted) = match result {
        Ok(result) => result,
        Err(error) => {
            requests::fail(&state.sqlite, &id).await?;
            return Err(error);
        }
    };
    if !accepted {
        requests::fail(&state.sqlite, &id).await?;
        requests::scope_on(&mut *state.sqlite.acquire().await?, &site, user).await?;
        return Ok(PromptResponse {
            accepted,
            answer,
            explanation_source,
            evidence,
            request_id: id,
        });
    }
    if let Err(error) = requests::commit(&state.sqlite, &site, user, &id).await {
        requests::fail(&state.sqlite, &id).await?;
        return Err(error);
    }
    Ok(PromptResponse {
        accepted: true,
        answer,
        explanation_source,
        evidence,
        request_id: id,
    })
}

pub(crate) fn demo_user_has_ai_page_access(site: &demo::DemoSiteDefinition) -> bool {
    sites::demo_site_role(site).is_owner() || site.profile == demo::DemoTrafficProfile::Mixed
}

fn validate_prompt(value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() || value.len() > MAX_PROMPT_BYTES {
        return Err(ApiError::BadRequest(format!(
            "Prompt must be between 1 and {MAX_PROMPT_BYTES} bytes"
        )));
    }
    Ok(())
}

async fn load_ai_mode(
    pool: &SqlitePool,
    site: &sites::ActiveSite,
    user_id: &str,
) -> Result<AiModeResponse, ApiError> {
    let enabled: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT ai_enabled FROM site_settings WHERE site_id=?),0)",
    )
    .bind(&site.id)
    .fetch_one(pool)
    .await?;
    let role = sites::effective_site_role(pool, user_id, site).await?;
    Ok(AiModeResponse {
        provider_available: false,
        can_manage: role.is_owner(),
        can_use: enabled,
        enabled,
        site_id: site.tracking_id.clone(),
        unlimited: true,
    })
}
fn demo_mode(site: &demo::DemoSiteDefinition) -> AiModeResponse {
    AiModeResponse {
        provider_available: false,
        can_manage: false,
        can_use: false,
        enabled: false,
        site_id: site.tracking_id.into(),
        unlimited: true,
    }
}
