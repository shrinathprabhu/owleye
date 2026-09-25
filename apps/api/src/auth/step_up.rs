use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::json;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    privacy::audit::{self, AuditEvent},
    ApiError, AppState,
};

use super::{
    sessions::{authenticated_session, is_demo_session},
    totp::verify_totp,
    types::SessionUserRecord,
};

const MAX_STEP_UP_ATTEMPTS: i64 = 5;
const MAX_STEP_UP_STARTS: i64 = 3;
const STEP_UP_START_WINDOW_SECONDS: i64 = 15 * 60;
const STEP_UP_TTL_MINUTES: i64 = 5;
const STEP_UP_VERIFIED_MINUTES: i64 = 10;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifyStepUpRequest {
    challenge_id: String,
    code: String,
}

#[derive(Debug, FromRow)]
struct StepUpChallenge {
    method: String,
}

pub(crate) async fn start_step_up(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    reject_preview(&session)?;

    let now = Utc::now();
    reserve_step_up_start(&state.sqlite, &session.session_id, &session.id, now).await?;

    let challenge_id = Uuid::new_v4().to_string();
    let expires_at = now + Duration::minutes(STEP_UP_TTL_MINUTES);
    let method = if session.totp_secret.is_some() {
        "totp"
    } else {
        "password"
    };
    let code_hash: Option<String> = None;

    replace_step_up_challenge(
        &state.sqlite,
        &challenge_id,
        &session,
        method,
        code_hash.as_deref(),
        &expires_at.to_rfc3339(),
        &now.to_rfc3339(),
    )
    .await?;

    Ok(no_store_json(json!({
        "challenge_id": challenge_id,
        "expires_at": expires_at.to_rfc3339(),
        "method": method,
        "status": "challenge_created"
    })))
}

pub(crate) async fn verify_step_up(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<VerifyStepUpRequest>,
) -> Result<Response, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    reject_preview(&session)?;
    let challenge_id = request.challenge_id.trim();
    if challenge_id.is_empty() || challenge_id.len() > 128 {
        return Err(invalid_step_up());
    }

    let challenge = reserve_challenge_attempt(
        &state.sqlite,
        challenge_id,
        &session.session_id,
        &session.id,
        Utc::now(),
    )
    .await?;
    let code = request.code.trim();
    let valid = match challenge.method.as_str() {
        "totp" => match session.totp_secret.as_deref() {
            Some(secret) => verify_totp(&state.settings.hash_salt, secret, code)?,
            None => false,
        },
        "password" => {
            super::password::verify_user_password(&state.sqlite, &session.id, &request.code).await?
        }
        _ => false,
    };
    if !valid {
        return Err(invalid_step_up());
    }

    let verified_at = Utc::now();
    let mut transaction = state.sqlite.begin().await?;
    let consumed = sqlx::query(
        r#"
        UPDATE auth_step_up_challenges
        SET consumed_at = ?, code_hash = NULL
        WHERE id = ?
            AND session_id = ?
            AND user_id = ?
            AND consumed_at IS NULL
            AND unixepoch(expires_at) > unixepoch(?)
        "#,
    )
    .bind(verified_at.to_rfc3339())
    .bind(challenge_id)
    .bind(&session.session_id)
    .bind(&session.id)
    .bind(verified_at.to_rfc3339())
    .execute(&mut *transaction)
    .await?;
    if consumed.rows_affected() != 1 {
        return Err(invalid_step_up());
    }
    let marked = sqlx::query(
        r#"
        UPDATE auth_sessions
        SET step_up_verified_at = ?
        WHERE id = ? AND user_id = ? AND revoked_at IS NULL
        "#,
    )
    .bind(verified_at.to_rfc3339())
    .bind(&session.session_id)
    .bind(&session.id)
    .execute(&mut *transaction)
    .await?;
    if marked.rows_affected() != 1 {
        return Err(ApiError::Unauthorized("Session expired".to_owned()));
    }
    audit::record_on(
        &mut transaction,
        AuditEvent {
            action: "security.step_up_verified",
            metadata: json!({ "method": challenge.method }),
            organization_id: None,
            target_id: None,
            target_type: Some("session"),
            user_id: Some(&session.id),
        },
    )
    .await?;
    transaction.commit().await?;

    Ok(no_store_json(json!({
        "status": "verified",
        "verified_until": (verified_at + Duration::minutes(STEP_UP_VERIFIED_MINUTES)).to_rfc3339()
    })))
}

pub(crate) async fn require_recent_step_up(
    pool: &SqlitePool,
    session_id: &str,
    user_id: &str,
) -> Result<(), ApiError> {
    let now = Utc::now().to_rfc3339();
    let verified = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM auth_sessions
            WHERE id = ?
                AND user_id = ?
                AND revoked_at IS NULL
                AND step_up_verified_at IS NOT NULL
                AND unixepoch(step_up_verified_at) >= unixepoch(?) - ?
                AND unixepoch(step_up_verified_at) <= unixepoch(?)
        )
        "#,
    )
    .bind(session_id)
    .bind(user_id)
    .bind(&now)
    .bind(STEP_UP_VERIFIED_MINUTES * 60)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    if verified {
        Ok(())
    } else {
        Err(ApiError::StepUpRequired)
    }
}

async fn reserve_step_up_start(
    pool: &SqlitePool,
    session_id: &str,
    user_id: &str,
    now: chrono::DateTime<Utc>,
) -> Result<(), ApiError> {
    let now = now.to_rfc3339();
    let reserved = sqlx::query_scalar::<_, String>(
        r#"
        UPDATE auth_sessions
        SET
            step_up_start_window_at = CASE
                WHEN step_up_start_window_at IS NULL
                    OR unixepoch(step_up_start_window_at) <= unixepoch(?1) - ?2
                    THEN ?1
                ELSE step_up_start_window_at
            END,
            step_up_start_count = CASE
                WHEN step_up_start_window_at IS NULL
                    OR unixepoch(step_up_start_window_at) <= unixepoch(?1) - ?2
                    THEN 1
                ELSE step_up_start_count + 1
            END
        WHERE id = ?3
            AND user_id = ?4
            AND revoked_at IS NULL
            AND (
                step_up_start_window_at IS NULL
                OR unixepoch(step_up_start_window_at) <= unixepoch(?1) - ?2
                OR step_up_start_count < ?5
            )
        RETURNING id
        "#,
    )
    .bind(now)
    .bind(STEP_UP_START_WINDOW_SECONDS)
    .bind(session_id)
    .bind(user_id)
    .bind(MAX_STEP_UP_STARTS)
    .fetch_optional(pool)
    .await?;

    if reserved.is_some() {
        Ok(())
    } else {
        Err(ApiError::TooManyRequests {
            retry_after_seconds: STEP_UP_START_WINDOW_SECONDS as u64,
        })
    }
}

async fn replace_step_up_challenge(
    pool: &SqlitePool,
    challenge_id: &str,
    session: &SessionUserRecord,
    method: &str,
    code_hash: Option<&str>,
    expires_at: &str,
    now: &str,
) -> Result<(), ApiError> {
    let mut transaction = pool.begin().await?;
    sqlx::query(
        r#"
        UPDATE auth_step_up_challenges
        SET consumed_at = COALESCE(consumed_at, ?), code_hash = NULL
        WHERE session_id = ? AND user_id = ? AND consumed_at IS NULL
        "#,
    )
    .bind(now)
    .bind(&session.session_id)
    .bind(&session.id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO auth_step_up_challenges (
            id, session_id, user_id, method, code_hash, expires_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(challenge_id)
    .bind(&session.session_id)
    .bind(&session.id)
    .bind(method)
    .bind(code_hash)
    .bind(expires_at)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn reserve_challenge_attempt(
    pool: &SqlitePool,
    challenge_id: &str,
    session_id: &str,
    user_id: &str,
    now: chrono::DateTime<Utc>,
) -> Result<StepUpChallenge, ApiError> {
    sqlx::query_as::<_, StepUpChallenge>(
        r#"
        UPDATE auth_step_up_challenges
        SET attempt_count = attempt_count + 1
        WHERE id = ?
            AND session_id = ?
            AND user_id = ?
            AND consumed_at IS NULL
            AND unixepoch(expires_at) > unixepoch(?)
            AND attempt_count < ?
        RETURNING method, code_hash
        "#,
    )
    .bind(challenge_id)
    .bind(session_id)
    .bind(user_id)
    .bind(now.to_rfc3339())
    .bind(MAX_STEP_UP_ATTEMPTS)
    .fetch_optional(pool)
    .await?
    .ok_or_else(invalid_step_up)
}

fn reject_preview(session: &SessionUserRecord) -> Result<(), ApiError> {
    if is_demo_session(session) {
        Err(ApiError::Forbidden(
            "Sensitive account actions are unavailable in this workspace".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn invalid_step_up() -> ApiError {
    ApiError::Unauthorized("Invalid, expired, or exhausted identity confirmation code".to_owned())
}

fn no_store_json(body: serde_json::Value) -> Response {
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}
