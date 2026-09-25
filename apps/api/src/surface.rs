use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::{auth, ApiError, AppState};

pub async fn logout_all(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;

    if auth::is_demo_session(&session) {
        return Ok(Json(json!({ "status": "signed_out_all" })));
    }

    sqlx::query("UPDATE auth_sessions SET revoked_at = CURRENT_TIMESTAMP WHERE user_id = ?")
        .bind(session.id)
        .execute(&state.sqlite)
        .await?;

    Ok(Json(json!({ "status": "signed_out_all" })))
}

pub async fn user_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;

    Ok(no_store(Json(json!({
        "avatar_url": session.avatar_url,
        "email": session.email,
        "email_verified_at": session.email_verified_at,
        "id": session.id,
        "name": session.name,
        "two_factor_enabled": session.totp_secret.is_some()
    }))))
}

fn no_store(response: impl IntoResponse) -> Response {
    let mut response = response.into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .insert(header::EXPIRES, HeaderValue::from_static("0"));
    response
}

pub async fn health_live() -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "service": "owleye-api",
            "status": "live",
            "version": env!("CARGO_PKG_VERSION")
        })),
    )
}

pub async fn version() -> Json<Value> {
    Json(json!({
        "service": "owleye-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_user_me_payloads_are_never_cacheable() {
        let response = no_store(Json(json!({ "id": "user_test" })));

        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert_eq!(response.headers().get(header::PRAGMA).unwrap(), "no-cache");
        assert_eq!(response.headers().get(header::EXPIRES).unwrap(), "0");
    }
}
