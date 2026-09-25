use axum::{
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    DateTime(#[from] chrono::ParseError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("recent authentication required")]
    StepUpRequired,
    #[error("too many requests; retry after {retry_after_seconds} seconds")]
    TooManyRequests { retry_after_seconds: u64 },
    #[error("unauthorized: {0}")]
    Unauthorized(String),
}

impl ApiError {
    /// A stable, non-sensitive diagnostic class for server logs. Inner error
    /// displays can contain request URLs, database values, provider responses, or
    /// other operator secrets, so the HTTP boundary never logs them verbatim.
    fn diagnostic_kind(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::Conflict(_) => "conflict",

            Self::DateTime(_) => "datetime",
            Self::Database(_) => "database",
            Self::Forbidden(_) => "forbidden",
            Self::Http(_) => "http_client",
            Self::Internal(_) => "internal",
            Self::Json(_) => "json",
            Self::NotImplemented(_) => "not_implemented",
            Self::ServiceUnavailable(_) => "service_unavailable",
            Self::StepUpRequired => "step_up_required",
            Self::TooManyRequests { .. } => "rate_limited",
            Self::Unauthorized(_) => "unauthorized",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::StepUpRequired => StatusCode::PRECONDITION_REQUIRED,
            Self::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::DateTime(_)
            | Self::Database(_)
            | Self::Http(_)
            | Self::Internal(_)
            | Self::Json(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::Conflict(_) => "conflict",

            Self::Forbidden(_) => "forbidden",
            Self::NotImplemented(_) => "not_implemented",
            Self::ServiceUnavailable(_) => "service_unavailable",
            Self::StepUpRequired => "step_up_required",
            Self::TooManyRequests { .. } => "rate_limited",
            Self::Unauthorized(_) => "unauthorized",
            Self::DateTime(_)
            | Self::Database(_)
            | Self::Http(_)
            | Self::Internal(_)
            | Self::Json(_) => "internal_error",
        }
    }

    fn public_message(&self) -> String {
        match self {
            Self::BadRequest(message)
            | Self::Conflict(message)
            | Self::Forbidden(message)
            | Self::NotImplemented(message)
            | Self::Unauthorized(message) => message.clone(),
            Self::ServiceUnavailable(message) => message.clone(),
            Self::StepUpRequired => {
                "Confirm your identity before continuing with this sensitive action".to_owned()
            }
            Self::TooManyRequests { .. } => "Too many requests. Try again later.".to_owned(),
            Self::DateTime(_)
            | Self::Database(_)
            | Self::Http(_)
            | Self::Internal(_)
            | Self::Json(_) => "Internal server error".to_owned(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code();
        let message = self.public_message();
        let retry_after_seconds = match &self {
            Self::TooManyRequests {
                retry_after_seconds,
            } => Some(*retry_after_seconds),
            _ => None,
        };

        if status.is_server_error() {
            tracing::error!(error_kind = self.diagnostic_kind(), "api request failed");
        }

        let mut response = (
            status,
            Json(json!({
                "error": {
                    "code": code,
                    "message": message
                }
            })),
        )
            .into_response();
        if let Some(retry_after_seconds) = retry_after_seconds {
            response.headers_mut().insert(
                RETRY_AFTER,
                HeaderValue::from_str(&retry_after_seconds.to_string())
                    .expect("integer Retry-After is always a valid header value"),
            );
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_responses_include_retry_after() {
        let response = ApiError::TooManyRequests {
            retry_after_seconds: 17,
        }
        .into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "17");
    }

    #[test]
    fn unavailable_features_return_not_implemented() {
        let response = ApiError::NotImplemented("Unavailable".to_owned()).into_response();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }
}
