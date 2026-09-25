use std::time::Duration;

use pasetors::{
    claims::{Claims, ClaimsValidationRules},
    keys::SymmetricKey,
    local,
    token::{Local, UntrustedToken},
    version4::V4,
};

use crate::ApiError;

const SESSION_AUDIENCE: &str = "owleye-console";
const SESSION_ISSUER: &str = "owleye-api";
const SESSION_TOKEN_CONTEXT: &str = "owleye session paseto v4 local key";
const TOTP_SECRET_AUDIENCE: &str = "owleye-totp-secret";
const TOTP_SECRET_CONTEXT: &str = "owleye totp secret paseto v4 local key";
const TOTP_SECRET_PREFIX: &str = "enc:v1:";

pub(super) struct SessionTokenClaims {
    pub(super) expires_at: String,
    pub(super) session_id: String,
    pub(super) subject: String,
}

pub(super) fn create_session_token(
    hash_salt: &str,
    user_id: &str,
    session_id: &str,
    access_minutes: i64,
) -> Result<String, ApiError> {
    let mut claims =
        Claims::new_expires_in(&Duration::from_secs(access_minutes.max(1) as u64 * 60))
            .map_err(paseto_error)?;
    claims.audience(SESSION_AUDIENCE).map_err(paseto_error)?;
    claims.issuer(SESSION_ISSUER).map_err(paseto_error)?;
    claims.subject(user_id).map_err(paseto_error)?;
    claims.token_identifier(session_id).map_err(paseto_error)?;

    local::encrypt(&session_key(hash_salt)?, &claims, None, None).map_err(paseto_error)
}

pub(super) fn validate_session_token(
    hash_salt: &str,
    token: &str,
) -> Result<SessionTokenClaims, ApiError> {
    let untrusted_token =
        UntrustedToken::<Local, V4>::try_from(token).map_err(|_| invalid_session())?;
    let mut validation_rules = ClaimsValidationRules::new();
    validation_rules.validate_audience_with(SESSION_AUDIENCE);
    validation_rules.validate_issuer_with(SESSION_ISSUER);

    let trusted_token = local::decrypt(
        &session_key(hash_salt)?,
        &untrusted_token,
        &validation_rules,
        None,
        None,
    )
    .map_err(|_| invalid_session())?;
    let claims = trusted_token.payload_claims().ok_or_else(invalid_session)?;
    let expires_at = claims
        .get_claim("exp")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(invalid_session)?;
    let subject = claims
        .get_claim("sub")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(invalid_session)?;
    let session_id = claims
        .get_claim("jti")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(invalid_session)?;

    Ok(SessionTokenClaims {
        expires_at: expires_at.to_owned(),
        session_id: session_id.to_owned(),
        subject: subject.to_owned(),
    })
}

pub(super) fn seal_totp_secret(hash_salt: &str, secret: &str) -> Result<String, ApiError> {
    let mut claims = Claims::new().map_err(paseto_error)?;
    claims.non_expiring();
    claims
        .audience(TOTP_SECRET_AUDIENCE)
        .map_err(paseto_error)?;
    claims.issuer(SESSION_ISSUER).map_err(paseto_error)?;
    claims
        .add_additional("secret", secret)
        .map_err(paseto_error)?;
    let encrypted =
        local::encrypt(&totp_secret_key(hash_salt)?, &claims, None, None).map_err(paseto_error)?;
    Ok(format!("{TOTP_SECRET_PREFIX}{encrypted}"))
}

pub(super) fn open_totp_secret(hash_salt: &str, stored: &str) -> Result<String, ApiError> {
    let Some(encrypted) = stored.strip_prefix(TOTP_SECRET_PREFIX) else {
        // Compatibility for installations created before encrypted TOTP storage.
        // The next successful 2FA login upgrades the row in place.
        return Ok(stored.to_owned());
    };
    let untrusted_token =
        UntrustedToken::<Local, V4>::try_from(encrypted).map_err(|_| invalid_totp_secret())?;
    let mut validation_rules = ClaimsValidationRules::new();
    validation_rules.allow_non_expiring();
    validation_rules.validate_audience_with(TOTP_SECRET_AUDIENCE);
    validation_rules.validate_issuer_with(SESSION_ISSUER);
    let trusted_token = local::decrypt(
        &totp_secret_key(hash_salt)?,
        &untrusted_token,
        &validation_rules,
        None,
        None,
    )
    .map_err(|_| invalid_totp_secret())?;
    trusted_token
        .payload_claims()
        .and_then(|claims| claims.get_claim("secret"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(invalid_totp_secret)
}

fn session_key(hash_salt: &str) -> Result<SymmetricKey<V4>, ApiError> {
    let key = blake3::derive_key(SESSION_TOKEN_CONTEXT, hash_salt.as_bytes());
    SymmetricKey::<V4>::from(&key).map_err(paseto_error)
}

fn totp_secret_key(hash_salt: &str) -> Result<SymmetricKey<V4>, ApiError> {
    let key = blake3::derive_key(TOTP_SECRET_CONTEXT, hash_salt.as_bytes());
    SymmetricKey::<V4>::from(&key).map_err(paseto_error)
}

fn invalid_session() -> ApiError {
    ApiError::Unauthorized("Session expired".to_owned())
}

fn invalid_totp_secret() -> ApiError {
    ApiError::Internal(anyhow::anyhow!("stored TOTP secret could not be decrypted"))
}

fn paseto_error(error: pasetors::errors::Error) -> ApiError {
    ApiError::Internal(anyhow::anyhow!("PASETO session token error: {error}"))
}
