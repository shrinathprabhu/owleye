use super::{authenticated_session, normalize_email, sessions, users};
use crate::{ApiError, AppState};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credentials {
    email: String,
    password: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PasswordChange {
    current_password: String,
    password: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserUpdate {
    password: Option<String>,
    enabled: Option<bool>,
    is_admin: Option<bool>,
}

fn invalid() -> ApiError {
    ApiError::Unauthorized("Invalid email or password".into())
}
async fn password_job<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    static CAPACITY: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
    let permit = CAPACITY
        .try_acquire()
        .map_err(|_| ApiError::TooManyRequests {
            retry_after_seconds: 2,
        })?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        job()
    })
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    result
}
async fn hash(password: &str) -> Result<String, ApiError> {
    if password.chars().count() < 8 {
        return Err(ApiError::BadRequest(
            "Password must contain at least 8 characters".into(),
        ));
    }
    let password = password.to_owned();
    password_job(move || {
        Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|h| h.to_string())
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("Password hashing failed: {e}")))
    })
    .await
}
async fn verify(encoded: String, password: &str) -> Result<bool, ApiError> {
    let password = password.to_owned();
    password_job(move || {
        Ok(PasswordHash::new(&encoded).is_ok_and(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        }))
    })
    .await
}
pub(super) async fn verify_user_password(
    pool: &SqlitePool,
    id: &str,
    password: &str,
) -> Result<bool, ApiError> {
    let encoded: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id=? AND status='active'")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .flatten();
    match encoded {
        Some(encoded) => verify(encoded, password).await,
        None => Ok(false),
    }
}
pub(crate) async fn login(
    State(state): State<AppState>,
    Json(input): Json<Credentials>,
) -> Result<Response, ApiError> {
    let email = normalize_email(&input.email).map_err(|_| invalid())?;
    let user = users::get_user_by_email(&state.sqlite, &email).await?;
    // Unknown addresses still perform the expensive password verification.
    static DUMMY: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    let dummy = DUMMY
        .get_or_try_init(|| async { hash(&uuid::Uuid::new_v4().to_string()).await })
        .await?;
    let encoded: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE email=? AND status='active'")
            .bind(&email)
            .fetch_optional(&state.sqlite)
            .await?
            .flatten();
    let valid = verify(encoded.unwrap_or_else(|| dummy.clone()), &input.password).await?;
    let user = user.filter(|_| valid).ok_or_else(invalid)?;
    sessions::complete_primary_auth(&state, user, false).await
}
pub(crate) async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PasswordChange>,
) -> Result<Response, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    let old: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=?")
        .bind(&session.id)
        .fetch_one(&state.sqlite)
        .await?;
    if !verify(old.clone(), &input.current_password).await? {
        return Err(invalid());
    }
    let encoded = hash(&input.password).await?;
    let mut tx = state.sqlite.begin_with("BEGIN IMMEDIATE").await?;
    let updated = sqlx::query("UPDATE users SET password_hash=?, updated_at=CURRENT_TIMESTAMP WHERE id=? AND password_hash=?").bind(encoded).bind(&session.id).bind(old).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::Conflict("Password changed; sign in again".into()));
    }
    sqlx::query("DELETE FROM auth_sessions WHERE user_id=?")
        .bind(&session.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM auth_login_challenges WHERE user_id=?")
        .bind(&session.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(super::cleared_session_json(
        &state,
        json!({"status":"password_changed"}),
    ))
}
pub(crate) async fn require_admin(pool: &SqlitePool, id: &str) -> Result<(), ApiError> {
    let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id=? AND is_admin=1 AND status='active' AND deleted_at IS NULL)").bind(id).fetch_one(pool).await?;
    if allowed {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "Instance administrator access is required".into(),
        ))
    }
}
pub(crate) async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    require_admin(&state.sqlite, &session.id).await?;
    let rows: Vec<(String,String,Option<String>,bool,String)> = sqlx::query_as("SELECT id,email,name,is_admin,status FROM users WHERE deleted_at IS NULL ORDER BY created_at,id").fetch_all(&state.sqlite).await?;
    Ok(Json(
        json!({"users": rows.into_iter().map(|(id,email,name,is_admin,status)|json!({"id":id,"email":email,"name":name,"is_admin":is_admin,"enabled":status=="active"})).collect::<Vec<_>>()}),
    ))
}
pub(crate) async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<Json<Value>, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    require_admin(&state.sqlite, &session.id).await?;
    let email = normalize_email(&input.email)?;
    let encoded = hash(&input.password).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut tx = state.sqlite.begin_with("BEGIN IMMEDIATE").await?;
    let caller: bool =
        sqlx::query_scalar("SELECT is_admin=1 AND status='active' FROM users WHERE id=?")
            .bind(&session.id)
            .fetch_one(&mut *tx)
            .await?;
    if !caller {
        return Err(ApiError::Forbidden(
            "Administrator access was revoked".into(),
        ));
    }
    insert_user(&mut tx, &id, &email, &encoded, false).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id,"email":email})))
}
async fn insert_user(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
    email: &str,
    encoded: &str,
    admin: bool,
) -> Result<(), ApiError> {
    if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE email=?)")
        .bind(email)
        .fetch_one(&mut *conn)
        .await?
    {
        return Err(ApiError::Conflict("This email is already assigned".into()));
    }
    sqlx::query("INSERT INTO users(id,email,password_hash,is_admin,email_verified_at,onboarding_2fa_handled_at) VALUES(?,?,?,?,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").bind(id).bind(email).bind(encoded).bind(admin).execute(&mut *conn).await?;
    sqlx::query(
        "INSERT INTO organization_members(organization_id,user_id,role) VALUES('default',?,?)",
    )
    .bind(id)
    .bind(if admin { "owner" } else { "member" })
    .execute(&mut *conn)
    .await?;
    sqlx::query("INSERT INTO site_memberships(site_id,user_id,role) SELECT id,?,? FROM sites WHERE deleted_at IS NULL AND archived_at IS NULL").bind(id).bind(if admin {"owner"}else{"read_only"}).execute(conn).await?;
    Ok(())
}
pub(crate) async fn update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<UserUpdate>,
) -> Result<Json<Value>, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    require_admin(&state.sqlite, &session.id).await?;
    super::require_recent_step_up(&state.sqlite, &session.session_id, &session.id).await?;
    if id == session.id && (input.enabled == Some(false) || input.is_admin == Some(false)) {
        return Err(ApiError::BadRequest(
            "You cannot disable or demote your own administrator account".into(),
        ));
    }
    let encoded = match input.password {
        Some(p) => Some(hash(&p).await?),
        None => None,
    };
    let mut tx = state.sqlite.begin_with("BEGIN IMMEDIATE").await?;
    let caller: bool =
        sqlx::query_scalar("SELECT is_admin=1 AND status='active' FROM users WHERE id=?")
            .bind(&session.id)
            .fetch_one(&mut *tx)
            .await?;
    if !caller {
        return Err(ApiError::Forbidden(
            "Administrator access was revoked".into(),
        ));
    }
    let affected=sqlx::query("UPDATE users SET password_hash=COALESCE(?,password_hash),status=COALESCE(?,status),is_admin=COALESCE(?,is_admin),updated_at=CURRENT_TIMESTAMP WHERE id=? AND deleted_at IS NULL").bind(&encoded).bind(input.enabled.map(|v|if v{"active"}else{"disabled"})).bind(input.is_admin).bind(&id).execute(&mut *tx).await?.rows_affected();
    if affected != 1 {
        return Err(ApiError::BadRequest("Unknown user".into()));
    }
    if let Some(admin) = input.is_admin {
        if !admin {
            sqlx::query("UPDATE sites SET created_by_user_id=? WHERE created_by_user_id=?")
                .bind(&session.id)
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE organizations SET created_by_user_id=? WHERE created_by_user_id=?")
                .bind(&session.id)
                .bind(&id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE organization_members SET role=? WHERE user_id=?")
            .bind(if admin { "owner" } else { "member" })
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE site_memberships SET role=? WHERE user_id=?")
            .bind(if admin { "owner" } else { "read_only" })
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM auth_sessions WHERE user_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM auth_login_challenges WHERE user_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"status":"updated"})))
}
/// One-shot installer entry point. Password comes from stdin, never process arguments.
pub async fn bootstrap(pool: &SqlitePool, email: &str, password: &str) -> anyhow::Result<()> {
    let email = normalize_email(email)?;
    let encoded = hash(password).await?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users)")
        .fetch_one(&mut *tx)
        .await?;
    anyhow::ensure!(
        !exists,
        "Instance is already initialized; setup never resets passwords"
    );
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name,slug) VALUES('default','OwlEye','default')")
        .execute(&mut *tx)
        .await?;
    insert_user(&mut tx, &id, &email, &encoded, true).await?;
    sqlx::query("UPDATE organizations SET created_by_user_id=? WHERE id='default'")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("CREATE TRIGGER IF NOT EXISTS single_workspace BEFORE INSERT ON organizations WHEN NEW.id != 'default' BEGIN SELECT RAISE(ABORT, 'Only one workspace is supported'); END").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Local recovery requires filesystem access to the configured database.
pub async fn recover_admin(pool: &SqlitePool, email: &str, password: &str) -> anyhow::Result<()> {
    let email = normalize_email(email)?;
    let encoded = hash(password).await?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM users WHERE email=? AND is_admin=1 AND deleted_at IS NULL",
    )
    .bind(email)
    .fetch_optional(&mut *tx)
    .await?;
    let id = id.ok_or_else(|| anyhow::anyhow!("No administrator exists with this email"))?;
    sqlx::query("UPDATE users SET password_hash=?,status='active',totp_secret=NULL,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(encoded).bind(&id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM auth_sessions WHERE user_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM auth_login_challenges WHERE user_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
