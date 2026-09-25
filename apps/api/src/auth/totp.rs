use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha1::Sha1;
use sqlx::SqlitePool;

use crate::{ApiError, AppState};

use super::{
    common::{encode, parse_time, unix_timestamp},
    sessions::{authenticated_session, is_demo_session, issue_session_response, user_json},
    tokens::{open_totp_secret, seal_totp_secret},
    types::{
        ChallengeRecord, DisableTwoFactorRequest, EnableTwoFactorRequest, VerifyTfaRequest,
        VerifyTwoFactorRequest,
    },
    users::get_user_by_id,
};

type HmacSha1 = Hmac<Sha1>;
const MAX_TOTP_LOGIN_ATTEMPTS: i64 = 5;

pub(crate) async fn verify_two_factor(
    State(state): State<AppState>,
    Json(request): Json<VerifyTwoFactorRequest>,
) -> Result<Response, ApiError> {
    verify_login_challenge(&state, &request.challenge_id, &request.code).await
}

pub(crate) async fn verify_tfa(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<VerifyTfaRequest>,
) -> Result<Response, ApiError> {
    if let Some(challenge_id) = request.challenge_id {
        return verify_login_challenge(&state, &challenge_id, &request.code).await;
    }

    if let Some(secret) = request.secret {
        let response =
            enable_two_factor_for_session(&state, &headers, &secret, &request.code).await?;
        return Ok(Json(response).into_response());
    }

    Err(ApiError::BadRequest(
        "A 2FA challenge or setup secret is required".to_owned(),
    ))
}

async fn verify_login_challenge(
    state: &AppState,
    challenge_id: &str,
    code: &str,
) -> Result<Response, ApiError> {
    let challenge = reserve_login_challenge_attempt(&state.sqlite, challenge_id).await?;

    if parse_time(&challenge.expires_at)? < Utc::now() {
        return Err(invalid_login_challenge());
    }

    let user = get_user_by_id(&state.sqlite, &challenge.user_id).await?;
    let secret = user
        .totp_secret
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("2FA is not enabled for this user".to_owned()))?;

    if !verify_totp(&state.settings.hash_salt, secret, code)? {
        return Err(ApiError::Unauthorized("Invalid 2FA code".to_owned()));
    }

    if !secret.starts_with("enc:v1:") {
        let sealed = seal_totp_secret(&state.settings.hash_salt, secret)?;
        sqlx::query("UPDATE users SET totp_secret = ?, updated_at = ? WHERE id = ?")
            .bind(sealed)
            .bind(Utc::now().to_rfc3339())
            .bind(&user.id)
            .execute(&state.sqlite)
            .await?;
    }

    let verified = sqlx::query(
        "UPDATE auth_login_challenges SET verified_at = ? WHERE id = ? AND verified_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(challenge_id)
    .execute(&state.sqlite)
    .await?;
    if verified.rows_affected() != 1 {
        return Err(ApiError::Unauthorized(
            "2FA challenge was already used".to_owned(),
        ));
    }

    issue_session_response(state, user, false).await
}

async fn reserve_login_challenge_attempt(
    pool: &SqlitePool,
    challenge_id: &str,
) -> Result<ChallengeRecord, ApiError> {
    sqlx::query_as::<_, ChallengeRecord>(
        r#"
        UPDATE auth_login_challenges
        SET attempt_count = attempt_count + 1
        WHERE id = ?
          AND verified_at IS NULL
          AND unixepoch(expires_at) > unixepoch(?)
          AND attempt_count < ?
        RETURNING user_id, expires_at
        "#,
    )
    .bind(challenge_id)
    .bind(Utc::now().to_rfc3339())
    .bind(MAX_TOTP_LOGIN_ATTEMPTS)
    .fetch_optional(pool)
    .await?
    .ok_or_else(invalid_login_challenge)
}

fn invalid_login_challenge() -> ApiError {
    ApiError::Unauthorized("Invalid, expired, or exhausted 2FA challenge".to_owned())
}

pub(crate) async fn setup_two_factor(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    reject_demo_two_factor(&session)?;
    if session.totp_secret.is_some() {
        return Err(ApiError::BadRequest("2FA is already enabled".to_owned()));
    }

    let secret = generate_totp_secret();
    let label = format!("OwlEye:{}", session.email);
    let otpauth_url = format!(
        "otpauth://totp/{}?secret={}&issuer={}",
        encode(&label),
        secret,
        encode("OwlEye")
    );

    Ok(Json(json!({
        "otpauth_url": otpauth_url,
        "secret": secret,
        "status": "setup"
    })))
}

pub(crate) async fn enable_two_factor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<EnableTwoFactorRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        enable_two_factor_for_session(&state, &headers, &request.secret, &request.code).await?,
    ))
}

async fn enable_two_factor_for_session(
    state: &AppState,
    headers: &HeaderMap,
    secret: &str,
    code: &str,
) -> Result<Value, ApiError> {
    let session = authenticated_session(state, headers).await?;
    reject_demo_two_factor(&session)?;
    if session.totp_secret.is_some() {
        return Err(ApiError::BadRequest("2FA is already enabled".to_owned()));
    }

    let secret = normalize_totp_secret(secret)?;
    if !verify_totp(&state.settings.hash_salt, &secret, code)? {
        return Err(ApiError::Unauthorized("Invalid 2FA code".to_owned()));
    }
    let sealed_secret = seal_totp_secret(&state.settings.hash_salt, &secret)?;

    sqlx::query(
        "UPDATE users SET totp_secret = ?, onboarding_2fa_handled_at = ?, updated_at = ? WHERE id = ?",
    )
        .bind(sealed_secret)
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .bind(&session.id)
        .execute(&state.sqlite)
        .await?;

    let user = get_user_by_id(&state.sqlite, &session.id).await?;
    Ok(json!({
        "status": "enabled",
        "user": user_json(&user)
    }))
}

pub(crate) async fn disable_two_factor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<DisableTwoFactorRequest>,
) -> Result<Json<Value>, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    reject_demo_two_factor(&session)?;
    let secret = session
        .totp_secret
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("2FA is not enabled".to_owned()))?;

    if !verify_totp(&state.settings.hash_salt, secret, &request.code)? {
        return Err(ApiError::Unauthorized("Invalid 2FA code".to_owned()));
    }

    sqlx::query("UPDATE users SET totp_secret = NULL, updated_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(&session.id)
        .execute(&state.sqlite)
        .await?;

    let user = get_user_by_id(&state.sqlite, &session.id).await?;
    Ok(Json(json!({
        "status": "disabled",
        "user": user_json(&user)
    })))
}

fn reject_demo_two_factor(session: &super::types::SessionUserRecord) -> Result<(), ApiError> {
    if is_demo_session(session) {
        Err(ApiError::Forbidden(
            "Two-factor settings are unavailable in this workspace".to_owned(),
        ))
    } else {
        Ok(())
    }
}

pub(super) fn verify_totp(
    hash_salt: &str,
    stored_secret: &str,
    code: &str,
) -> Result<bool, ApiError> {
    let code = code.trim();
    if code.len() != 6 || !code.chars().all(|character| character.is_ascii_digit()) {
        return Ok(false);
    }

    let secret = open_totp_secret(hash_salt, stored_secret)?;
    let normalized_secret = normalize_totp_secret(&secret)?;
    let secret_bytes = BASE32_NOPAD
        .decode(normalized_secret.as_bytes())
        .map_err(|_| ApiError::BadRequest("Invalid stored 2FA secret".to_owned()))?;
    let current_step = unix_timestamp() / 30;

    for offset in [-1_i64, 0, 1] {
        let step = current_step.saturating_add_signed(offset);
        if super::constant_time_eq(totp_code(&secret_bytes, step)?.as_bytes(), code.as_bytes()) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn totp_code(secret: &[u8], counter: u64) -> Result<String, ApiError> {
    let mut mac = HmacSha1::new_from_slice(secret)
        .map_err(|_| ApiError::BadRequest("Invalid stored 2FA secret".to_owned()))?;
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();
    let offset = (result[19] & 0x0f) as usize;
    let binary = (((result[offset] & 0x7f) as u32) << 24)
        | ((result[offset + 1] as u32) << 16)
        | ((result[offset + 2] as u32) << 8)
        | (result[offset + 3] as u32);

    Ok(format!("{:06}", binary % 1_000_000))
}

fn generate_totp_secret() -> String {
    let bytes: [u8; 20] = rand::random();
    BASE32_NOPAD.encode(&bytes)
}

fn normalize_totp_secret(secret: &str) -> Result<String, ApiError> {
    let normalized = secret.replace(' ', "").to_uppercase();
    if normalized.len() < 16 || normalized.len() > 128 {
        return Err(ApiError::BadRequest("Invalid 2FA secret".to_owned()));
    }

    BASE32_NOPAD
        .decode(normalized.as_bytes())
        .map_err(|_| ApiError::BadRequest("Invalid 2FA secret".to_owned()))?;

    Ok(normalized)
}
