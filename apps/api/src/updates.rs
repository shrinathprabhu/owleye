use crate::{auth, ApiError, AppState};
use axum::{extract::State, http::HeaderMap, Json};
use serde_json::{json, Value};
/// Outbound-only polling works behind firewalls; no public webhook is necessary.
pub(crate) async fn latest(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::require_admin(&state.sqlite, &session.id).await?;
    static CACHE: tokio::sync::Mutex<Option<(std::time::Instant, Value)>> =
        tokio::sync::Mutex::const_new(None);
    let mut cache = CACHE.lock().await;
    if let Some((at, value)) = &*cache {
        if at.elapsed() < std::time::Duration::from_secs(3600) {
            return Ok(Json(value.clone()));
        }
    }
    let response = state
        .http
        .get("https://api.github.com/repos/shrinathprabhu/owleye/releases/latest")
        .header("User-Agent", "OwlEye-self-hosted")
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable("Release check unavailable".into()))?;
    let value = if response.status() == reqwest::StatusCode::NOT_FOUND {
        json!({"current":env!("CARGO_PKG_VERSION"),"latest":null,"url":"https://github.com/shrinathprabhu/owleye/releases","update_available":false})
    } else {
        if !response.status().is_success() {
            return Err(ApiError::ServiceUnavailable(
                "GitHub release check unavailable; try later".into(),
            ));
        }
        let release: Value = response
            .json()
            .await
            .map_err(|_| ApiError::ServiceUnavailable("Invalid release response".into()))?;
        let tag = release["tag_name"].as_str().unwrap_or("");
        let latest = tag.strip_prefix('v').unwrap_or(tag);
        let parse = |v: &str| -> Option<Vec<u64>> {
            let result = v
                .split('.')
                .map(str::parse)
                .collect::<Result<Vec<u64>, _>>()
                .ok()?;
            (result.len() == 3).then_some(result)
        };
        let available = match (parse(latest), parse(env!("CARGO_PKG_VERSION"))) {
            (Some(a), Some(b)) => a > b,
            _ => false,
        };
        json!({"current":env!("CARGO_PKG_VERSION"),"latest":latest,"url":"https://github.com/shrinathprabhu/owleye/releases/latest","update_available":available})
    };
    *cache = Some((std::time::Instant::now(), value.clone()));
    Ok(Json(value))
}
