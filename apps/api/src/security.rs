use axum::{
    extract::{Request, State},
    http::{header, HeaderName, HeaderValue, Method},
    middleware::Next,
    response::Response,
};

use crate::{auth, demo, sites, ApiError, AppState};

#[derive(Clone, Debug)]
pub(crate) struct ConsoleRateLimitIdentity(pub(crate) String);

/// Establishes the minimum authentication and tenant-membership invariant for
/// the entire console surface. Individual handlers may require stronger roles,
/// but no site-scoped route can be added without passing this gate.
pub async fn require_console_access(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if request.uri().path() == "/v1/auth/session" {
        return Ok(next.run(request).await);
    }

    let session = auth::authenticated_session(&state, request.headers()).await?;
    if let Some(identifier) = site_identifier(request.uri().path()) {
        if auth::is_demo_session(&session) {
            demo::demo_site(identifier).ok_or_else(|| {
                ApiError::Forbidden("You do not have access to this site".to_owned())
            })?;
        } else {
            sites::resolve_site_for_user(&state.sqlite, &session.id, identifier).await?;
        }
    }
    request
        .extensions_mut()
        .insert(ConsoleRateLimitIdentity(session.session_id));

    Ok(next.run(request).await)
}

fn site_identifier(path: &str) -> Option<&str> {
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    match segments.as_slice() {
        ["v1", "sites", identifier, ..] | ["sites", identifier, ..] => Some(identifier),
        _ => None,
    }
}

/// Cookie-authenticated writes must come from an explicitly trusted browser origin.
/// CORS controls which responses a browser may read; this check also prevents
/// cross-site form submissions from causing a state change.
pub async fn require_console_origin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if method_requires_origin(request.method()) {
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                ApiError::Forbidden("A trusted console origin is required".to_owned())
            })?;

        if !state
            .settings
            .auth
            .console_origins()
            .any(|allowed| allowed == origin)
        {
            return Err(ApiError::Forbidden(
                "The request origin is not allowed for the console".to_owned(),
            ));
        }
    }

    Ok(next.run(request).await)
}

fn method_requires_origin(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

pub async fn security_headers(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    // Only public rules, the API contract, and the favicon may opt into caching.
    // This also covers errors, aliases and future authenticated routes.
    let private_response =
        is_auth_surface(path) || !matches!(path, "/v1/rules" | "/v1/openapi.json" | "/favicon.ico");
    let is_console = !path.starts_with("/v1/") && !path.starts_with("/health");
    let mut response = next.run(request).await;
    let private_response = private_response || !response.status().is_success();
    let headers = response.headers_mut();

    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );

    if private_response {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
        headers.insert(header::EXPIRES, HeaderValue::from_static("0"));
    }
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=(), usb=()"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            if is_console { "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src 'self'; worker-src 'self' blob:; frame-ancestors 'none'; base-uri 'self'; form-action 'self'" } else { "default-src 'none'; frame-ancestors 'none'; base-uri 'none'" },
        ),
    );

    if state.settings.auth.cookie_secure {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }

    response
}

fn is_auth_surface(path: &str) -> bool {
    path.contains("/auth/") || path.ends_with("/auth") || path == "/user/me"
}

#[cfg(test)]
mod tests {
    use axum::http::Method;

    use super::{is_auth_surface, method_requires_origin, site_identifier};

    #[test]
    fn site_identifier_only_matches_console_site_routes() {
        assert_eq!(site_identifier("/v1/sites/site-a/events"), Some("site-a"));
        assert_eq!(site_identifier("/sites/site-a/domains"), Some("site-a"));
        assert_eq!(site_identifier("/v1/developer/sites/site-a/events"), None);
        assert_eq!(site_identifier("/v1/sites"), None);
    }

    #[test]
    fn legacy_user_alias_is_treated_as_an_auth_surface() {
        assert!(is_auth_surface("/user/me"));
        assert!(is_auth_surface("/v1/auth/session"));
        assert!(!is_auth_surface("/v1/sites"));
    }

    #[test]
    fn console_origin_is_required_only_for_unsafe_methods() {
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(method_requires_origin(&method));
        }
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(!method_requires_origin(&method));
        }
    }
}
