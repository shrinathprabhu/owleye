use super::types::UserRecord;
use crate::ApiError;
use sqlx::SqlitePool;
pub(super) async fn get_user_by_email(
    pool: &SqlitePool,
    email: &str,
) -> Result<Option<UserRecord>, ApiError> {
    Ok(sqlx::query_as::<_, UserRecord>(
        r#"
        SELECT id, email, email_verified_at, name, avatar_url, totp_secret
        FROM users
        WHERE email = ? AND status = 'active' AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await?)
}

pub(super) async fn get_user_by_id(pool: &SqlitePool, id: &str) -> Result<UserRecord, ApiError> {
    sqlx::query_as::<_, UserRecord>(
        r#"
        SELECT id, email, email_verified_at, name, avatar_url, totp_secret
        FROM users
        WHERE id = ? AND status = 'active' AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("User not found".to_owned()))
}
