use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Deserialize)]
pub(crate) struct VerifyTwoFactorRequest {
    pub(super) challenge_id: String,
    pub(super) code: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VerifyTfaRequest {
    pub(super) challenge_id: Option<String>,
    pub(super) code: String,
    pub(super) secret: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EnableTwoFactorRequest {
    pub(super) code: String,
    pub(super) secret: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DisableTwoFactorRequest {
    pub(super) code: String,
}

#[derive(Debug, FromRow, Serialize)]
pub(super) struct UserRecord {
    pub(super) avatar_url: Option<String>,
    pub(super) email: String,
    pub(super) email_verified_at: Option<String>,
    pub(super) id: String,
    pub(super) name: Option<String>,
    #[serde(skip_serializing)]
    pub(super) totp_secret: Option<String>,
}

#[derive(Debug, FromRow)]
pub(super) struct ChallengeRecord {
    pub(super) expires_at: String,
    pub(super) user_id: String,
}

#[derive(Debug, FromRow)]
pub(crate) struct SessionUserRecord {
    pub(crate) avatar_url: Option<String>,
    pub(crate) email: String,
    pub(crate) email_verified_at: Option<String>,
    pub(crate) expires_at: String,
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) onboarding_2fa_handled_at: Option<String>,
    pub(crate) session_id: String,
    pub(crate) totp_secret: Option<String>,
}
