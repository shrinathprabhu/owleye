use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{demo, ApiError, AppState};

use super::{
    common::{hash_secret, parse_time, random_token, LOGIN_CHALLENGE_TTL_MINUTES},
    tokens::{create_session_token, validate_session_token},
    types::{SessionUserRecord, UserRecord},
};

const NO_STORE: &str = "no-store";
const PREVIOUS_ACCESS_GRACE_SECONDS: i64 = 30;

#[derive(Debug)]
pub(super) struct IssuedSession {
    pub(super) access_cookie: String,
    pub(super) access_expires_at: String,
    pub(super) refresh_cookie: String,
    pub(super) refresh_expires_at: String,
}

#[derive(Debug, FromRow)]
struct RefreshSessionRecord {
    access_expires_at: String,
    id: String,
    refresh_expires_at: String,
    user_deleted_at: Option<i64>,
    user_id: String,
    user_status: String,
}

pub(crate) async fn get_session(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let session = match authenticated_session(&state, &headers).await {
        Ok(session) => session,
        Err(ApiError::Unauthorized(_)) => {
            return Ok(no_store_json(json!({ "authenticated": false })))
        }
        Err(error) => return Err(error),
    };
    let onboarding = onboarding_json(&state, &session).await?;

    Ok(no_store_json(json!({
        "access_expires_at": session.expires_at,
        "authenticated": true,
        "onboarding": onboarding,
        "user": {"id":session.id,"email":session.email,"name":session.name,"avatar_url":session.avatar_url,"two_factor_enabled":session.totp_secret.is_some(),"is_admin": sqlx::query_scalar::<_,bool>("SELECT is_admin FROM users WHERE id=?").bind(&session.id).fetch_one(&state.sqlite).await?}
    })))
}

pub(crate) async fn refresh(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let refresh_tokens = read_cookies(&headers, &state.settings.auth.refresh_cookie_name);
    let Some(refresh_token) = refresh_tokens.first() else {
        return Ok(cleared_unauthorized(
            &state,
            "Refresh credential is required",
        ));
    };

    let refresh_hash = hash_secret(&state.settings.hash_salt, &["refresh", refresh_token]);
    let current = sqlx::query_as::<_, RefreshSessionRecord>(
        r#"
        SELECT
            auth_sessions.id,
            auth_sessions.user_id,
            users.status AS user_status,
            users.deleted_at AS user_deleted_at,
            COALESCE(auth_sessions.access_expires_at, auth_sessions.expires_at) AS access_expires_at,
            COALESCE(auth_sessions.refresh_expires_at, auth_sessions.expires_at) AS refresh_expires_at
        FROM auth_sessions
        INNER JOIN users ON users.id = auth_sessions.user_id
        WHERE refresh_token_hash = ? AND revoked_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(&refresh_hash)
    .fetch_optional(&state.sqlite)
    .await?;

    let Some(current) = current else {
        if revoke_reused_refresh(&state, &refresh_hash).await? {
            return Ok(cleared_unauthorized(
                &state,
                "Refresh credential reuse was detected",
            ));
        }
        return Ok(cleared_unauthorized(&state, "Session expired"));
    };

    if current.user_status != "active" || current.user_deleted_at.is_some() {
        revoke_session(&state, &current.id, false).await?;
        return Ok(cleared_unauthorized(&state, "Session expired"));
    }

    if parse_time(&current.refresh_expires_at)? < Utc::now() {
        revoke_session(&state, &current.id, false).await?;
        return Ok(cleared_unauthorized(&state, "Session expired"));
    }

    let issued = build_session_credentials(&state, &current.user_id, &current.id)?;
    let next_refresh_token = cookie_value(&issued.refresh_cookie).ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!("generated refresh cookie is invalid"))
    })?;
    let next_access_token = cookie_value(&issued.access_cookie)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("generated access cookie is invalid")))?;
    let next_refresh_hash =
        hash_secret(&state.settings.hash_salt, &["refresh", next_refresh_token]);
    let next_access_hash = hash_secret(&state.settings.hash_salt, &["session", next_access_token]);
    let previous_access_expires_at = std::cmp::min(
        parse_time(&current.access_expires_at)?,
        Utc::now() + Duration::seconds(PREVIOUS_ACCESS_GRACE_SECONDS),
    )
    .to_rfc3339();
    let now = Utc::now().to_rfc3339();

    let updated = sqlx::query(
        r#"
        UPDATE auth_sessions
        SET
            previous_access_token_hash = token_hash,
            previous_access_expires_at = ?,
            token_hash = ?,
            access_expires_at = ?,
            previous_refresh_token_hash = refresh_token_hash,
            refresh_token_hash = ?,
            refresh_expires_at = ?,
            expires_at = ?,
            refresh_rotated_at = ?,
            last_seen_at = ?
        WHERE id = ? AND refresh_token_hash = ? AND revoked_at IS NULL
        "#,
    )
    .bind(previous_access_expires_at)
    .bind(next_access_hash)
    .bind(&issued.access_expires_at)
    .bind(next_refresh_hash)
    .bind(&issued.refresh_expires_at)
    .bind(&issued.refresh_expires_at)
    .bind(&now)
    .bind(&now)
    .bind(&current.id)
    .bind(&refresh_hash)
    .execute(&state.sqlite)
    .await?;

    if updated.rows_affected() != 1 {
        // A concurrent request already rotated this token. Treat the second use as
        // a possible replay and revoke the whole session family.
        revoke_session(&state, &current.id, true).await?;
        return Ok(cleared_unauthorized(
            &state,
            "Refresh credential reuse was detected",
        ));
    }

    Ok(session_response(
        StatusCode::OK,
        json!({
            "access_expires_at": issued.access_expires_at,
            "refresh_expires_at": issued.refresh_expires_at,
            "status": "refreshed"
        }),
        &issued,
        &state.settings.auth,
    ))
}

pub(crate) async fn logout(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(access_token) = read_cookie(&headers, &state.settings.auth.cookie_name) {
        if let Ok(claims) = validate_session_token(&state.settings.hash_salt, &access_token) {
            revoke_session(&state, &claims.session_id, false).await?;
        }
    }
    if let Some(refresh_token) = read_cookie(&headers, &state.settings.auth.refresh_cookie_name) {
        let refresh_hash = hash_secret(&state.settings.hash_salt, &["refresh", &refresh_token]);
        sqlx::query(
            r#"
            UPDATE auth_sessions
            SET revoked_at = COALESCE(revoked_at, ?)
            WHERE refresh_token_hash = ? OR previous_refresh_token_hash = ?
            "#,
        )
        .bind(Utc::now().to_rfc3339())
        .bind(&refresh_hash)
        .bind(&refresh_hash)
        .execute(&state.sqlite)
        .await?;
    }

    Ok(cleared_session_json(
        &state,
        json!({ "status": "signed_out" }),
    ))
}

pub(crate) async fn logout_all(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    if !is_demo_session(&session) {
        sqlx::query(
            "UPDATE auth_sessions SET revoked_at = COALESCE(revoked_at, ?) WHERE user_id = ?",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(&session.id)
        .execute(&state.sqlite)
        .await?;
    }
    Ok(cleared_session_json(
        &state,
        json!({ "status": "signed_out_all" }),
    ))
}

pub(crate) async fn skip_two_factor_onboarding(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let session = authenticated_session(&state, &headers).await?;
    if is_demo_session(&session) {
        return Err(ApiError::Forbidden(
            "Onboarding is unavailable in this workspace".to_owned(),
        ));
    }
    sqlx::query("UPDATE users SET onboarding_2fa_handled_at = ?, updated_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .bind(&session.id)
        .execute(&state.sqlite)
        .await?;

    Ok(no_store_json(json!({ "status": "skipped" })))
}

pub(super) async fn complete_primary_auth(
    state: &AppState,
    user: UserRecord,
    registered: bool,
) -> Result<Response, ApiError> {
    if user.totp_secret.is_some() {
        let challenge_id = create_login_challenge(&state.sqlite, &user.id).await?;
        return Ok(no_store_json(json!({
            "challenge_id": challenge_id,
            "register": registered,
            "tfa": true,
            "status": "requires_2fa"
        })));
    }

    issue_session_response(state, user, registered).await
}

pub(super) async fn issue_session_response(
    state: &AppState,
    user: UserRecord,
    registered: bool,
) -> Result<Response, ApiError> {
    let issued = create_session(state, &user.id).await?;

    Ok(session_response(
        StatusCode::OK,
        json!({
            "access_expires_at": issued.access_expires_at,
            "refresh_expires_at": issued.refresh_expires_at,
            "register": registered,
            "status": "authenticated",
            "tfa": false,
            "user": user_json(&user)
        }),
        &issued,
        &state.settings.auth,
    ))
}

pub(crate) async fn authenticated_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<SessionUserRecord, ApiError> {
    let token = read_cookie(headers, &state.settings.auth.cookie_name)
        .ok_or_else(|| ApiError::Unauthorized("Not signed in".to_owned()))?;
    let claims = validate_session_token(&state.settings.hash_salt, &token)?;
    if claims.subject == demo::DEMO_USER_ID {
        return Err(ApiError::Unauthorized("Session expired".to_owned()));
    }
    let token_hash = hash_secret(&state.settings.hash_salt, &["session", &token]);
    let session = sqlx::query_as::<_, SessionUserRecord>(
        r#"
        SELECT
            users.id,
            users.email,
            users.email_verified_at,
            users.name,
            users.avatar_url,
            users.totp_secret,
            users.onboarding_2fa_handled_at,
            auth_sessions.id AS session_id,
            CASE
                WHEN auth_sessions.token_hash = ?
                    THEN COALESCE(auth_sessions.access_expires_at, auth_sessions.expires_at)
                ELSE auth_sessions.previous_access_expires_at
            END AS expires_at
        FROM auth_sessions
        INNER JOIN users ON users.id = auth_sessions.user_id
        WHERE auth_sessions.id = ?
            AND auth_sessions.user_id = ?
            AND (
                auth_sessions.token_hash = ?
                OR (
                    auth_sessions.previous_access_token_hash = ?
                    AND auth_sessions.previous_access_expires_at >= ?
                )
            )
            AND auth_sessions.revoked_at IS NULL
            AND users.status = 'active'
            AND users.deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(&token_hash)
    .bind(&claims.session_id)
    .bind(&claims.subject)
    .bind(&token_hash)
    .bind(&token_hash)
    .bind(Utc::now().to_rfc3339())
    .fetch_optional(&state.sqlite)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("Session expired".to_owned()))?;

    if parse_time(&session.expires_at)? < Utc::now() {
        return Err(ApiError::Unauthorized("Session expired".to_owned()));
    }

    Ok(session)
}

pub(crate) fn is_demo_session(session: &SessionUserRecord) -> bool {
    session.id == demo::DEMO_USER_ID
}

pub(super) fn user_json(user: &UserRecord) -> Value {
    json!({
        "avatar_url": user.avatar_url,
        "email": user.email,
        "email_verified_at": user.email_verified_at,
        "id": user.id,
        "name": user.name,
        "two_factor_enabled": user.totp_secret.is_some()
    })
}

pub(super) async fn create_login_challenge(
    pool: &sqlx::SqlitePool,
    user_id: &str,
) -> Result<String, ApiError> {
    let id = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::minutes(LOGIN_CHALLENGE_TTL_MINUTES);

    sqlx::query(
        r#"
        INSERT INTO auth_login_challenges (id, user_id, expires_at)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(expires_at.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(id)
}

pub(super) async fn create_session(
    state: &AppState,
    user_id: &str,
) -> Result<IssuedSession, ApiError> {
    let session_id = Uuid::new_v4().to_string();
    let issued = build_session_credentials(state, user_id, &session_id)?;
    let access_token = cookie_value(&issued.access_cookie)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("generated access cookie is invalid")))?;
    let refresh_token = cookie_value(&issued.refresh_cookie).ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!("generated refresh cookie is invalid"))
    })?;
    let access_hash = hash_secret(&state.settings.hash_salt, &["session", access_token]);
    let refresh_hash = hash_secret(&state.settings.hash_salt, &["refresh", refresh_token]);

    sqlx::query(
        r#"
        INSERT INTO auth_sessions (
            id,
            user_id,
            token_hash,
            expires_at,
            access_expires_at,
            refresh_token_hash,
            refresh_expires_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(session_id)
    .bind(user_id)
    .bind(access_hash)
    .bind(&issued.refresh_expires_at)
    .bind(&issued.access_expires_at)
    .bind(refresh_hash)
    .bind(&issued.refresh_expires_at)
    .execute(&state.sqlite)
    .await?;

    Ok(issued)
}

async fn onboarding_json(state: &AppState, session: &SessionUserRecord) -> Result<Value, ApiError> {
    if is_demo_session(session) {
        return Ok(json!({
            "has_site": true,
            "next_step": "complete",
            "two_factor_prompt_handled": true
        }));
    }

    let has_site = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM sites AS site
            WHERE site.archived_at IS NULL
                AND site.deleted_at IS NULL
                AND (
                    site.created_by_user_id = ?
                    OR EXISTS (
                        SELECT 1 FROM site_memberships AS member
                        WHERE member.site_id = site.id AND member.user_id = ?
                    )
                )
        )
        "#,
    )
    .bind(&session.id)
    .bind(&session.id)
    .fetch_one(&state.sqlite)
    .await?;
    let two_factor_prompt_handled =
        session.totp_secret.is_some() || session.onboarding_2fa_handled_at.is_some();
    let next_step = if !two_factor_prompt_handled {
        "two_factor"
    } else if !has_site {
        "create_site"
    } else {
        "complete"
    };

    Ok(json!({
        "has_site": has_site,
        "next_step": next_step,
        "two_factor_prompt_handled": two_factor_prompt_handled
    }))
}

fn build_session_credentials(
    state: &AppState,
    user_id: &str,
    session_id: &str,
) -> Result<IssuedSession, ApiError> {
    let access_token = create_session_token(
        &state.settings.hash_salt,
        user_id,
        session_id,
        state.settings.auth.access_minutes,
    )?;
    let refresh_token = random_token(64);
    let access_expires_at =
        validate_session_token(&state.settings.hash_salt, &access_token)?.expires_at;
    let refresh_expires_at = Utc::now() + Duration::days(state.settings.auth.refresh_days);

    Ok(IssuedSession {
        access_cookie: access_cookie(&state.settings.auth, &access_token),
        access_expires_at,
        refresh_cookie: refresh_cookie(&state.settings.auth, &refresh_token),
        refresh_expires_at: refresh_expires_at.to_rfc3339(),
    })
}

async fn revoke_reused_refresh(state: &AppState, refresh_hash: &str) -> Result<bool, ApiError> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE auth_sessions
        SET revoked_at = COALESCE(revoked_at, ?), refresh_reuse_detected_at = ?
        WHERE previous_refresh_token_hash = ? AND revoked_at IS NULL
        "#,
    )
    .bind(&now)
    .bind(&now)
    .bind(refresh_hash)
    .execute(&state.sqlite)
    .await?;
    Ok(result.rows_affected() > 0)
}

async fn revoke_session(state: &AppState, session_id: &str, reuse: bool) -> Result<(), ApiError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE auth_sessions
        SET
            revoked_at = COALESCE(revoked_at, ?),
            refresh_reuse_detected_at = CASE WHEN ? THEN ? ELSE refresh_reuse_detected_at END
        WHERE id = ?
        "#,
    )
    .bind(&now)
    .bind(reuse)
    .bind(&now)
    .bind(session_id)
    .execute(&state.sqlite)
    .await?;
    Ok(())
}

fn session_response(
    status: StatusCode,
    body: Value,
    issued: &IssuedSession,
    settings: &crate::settings::AuthSettings,
) -> Response {
    let mut response = (status, Json(body)).into_response();
    attach_session_cookies(&mut response, issued, settings);
    response
}

pub(super) fn attach_session_cookies(
    response: &mut Response,
    issued: &IssuedSession,
    settings: &crate::settings::AuthSettings,
) {
    append_no_store(response.headers_mut());
    append_cookie(response.headers_mut(), &issued.access_cookie);
    append_cookie(response.headers_mut(), &issued.refresh_cookie);
    append_cookie(
        response.headers_mut(),
        &clear_cookie(&settings.refresh_cookie_name, "/", settings),
    );
}

fn no_store_json(body: Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    append_no_store(response.headers_mut());
    response
}

pub(crate) fn cleared_session_json(state: &AppState, body: Value) -> Response {
    let mut response = no_store_json(body);
    append_cleared_session_cookies(response.headers_mut(), &state.settings.auth);
    response
}

fn cleared_unauthorized(state: &AppState, message: &str) -> Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": { "code": "unauthorized", "message": message } })),
    )
        .into_response();
    append_no_store(response.headers_mut());
    append_cleared_session_cookies(response.headers_mut(), &state.settings.auth);
    response
}

fn append_cleared_session_cookies(
    headers: &mut HeaderMap,
    settings: &crate::settings::AuthSettings,
) {
    append_cookie(headers, &clear_cookie(&settings.cookie_name, "/", settings));
    append_cookie(
        headers,
        &clear_cookie(&settings.refresh_cookie_name, "/", settings),
    );
    append_cookie(
        headers,
        &clear_cookie(&settings.refresh_cookie_name, "/v1/auth", settings),
    );
}

fn append_no_store(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(NO_STORE));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(header::EXPIRES, HeaderValue::from_static("0"));
}

fn append_cookie(headers: &mut HeaderMap, cookie: &str) {
    if let Ok(value) = HeaderValue::from_str(cookie) {
        headers.append(header::SET_COOKIE, value);
    }
}

fn read_cookie(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    read_cookies(headers, cookie_name).into_iter().next()
}

fn read_cookies(headers: &HeaderMap, cookie_name: &str) -> Vec<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .filter_map(|cookie| {
            let (name, value) = cookie.trim().split_once('=')?;
            (name == cookie_name).then(|| value.to_owned())
        })
        .collect()
}

fn cookie_value(cookie: &str) -> Option<&str> {
    cookie.split_once('=')?.1.split(';').next()
}

fn access_cookie(settings: &crate::settings::AuthSettings, token: &str) -> String {
    cookie(
        &settings.cookie_name,
        token,
        settings.access_minutes * 60,
        "/",
        settings,
    )
}

fn refresh_cookie(settings: &crate::settings::AuthSettings, token: &str) -> String {
    cookie(
        &settings.refresh_cookie_name,
        token,
        settings.refresh_days * 24 * 60 * 60,
        "/v1/auth",
        settings,
    )
}

fn cookie(
    name: &str,
    token: &str,
    max_age: i64,
    path: &str,
    settings: &crate::settings::AuthSettings,
) -> String {
    let secure = if settings.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    format!("{name}={token}; Path={path}; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}")
}

fn clear_cookie(name: &str, path: &str, settings: &crate::settings::AuthSettings) -> String {
    let secure = if settings.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    format!("{name}=; Path={path}; HttpOnly; SameSite=Lax; Max-Age=0{secure}")
}
