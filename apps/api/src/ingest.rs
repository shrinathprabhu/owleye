use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::Utc;
use serde_json::{json, Value};

use crate::{
    api_keys::IngestScope,
    errors::ApiError,
    models::{
        normalize_site_id, validate_event, EventRow, EventRowContext, IngestPayload, IngestRequest,
    },
    privacy::{
        geoip::GeoLocation,
        identity::anonymous_ids,
        request_ip::client_ip_from_request,
        user_agent::{parse_user_agent, ParsedUserAgent},
    },
    rate_limit, retention, site_configuration,
    sites::{self, ActiveSite},
    storage::sqlite,
    AppState,
};

pub(crate) async fn ingest_event(
    State(state): State<AppState>,
    scope: Option<Extension<IngestScope>>,
    ConnectInfo(connect_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<IngestPayload>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let requested_site_id = normalize_site_id(payload.site_id())?;
    let client_ip =
        client_ip_from_request(&headers, connect_addr.ip(), &state.settings.trusted_proxies);
    let site = sites::resolve_active_site(&state.sqlite, &requested_site_id).await?;
    authorize_ingest(&state, &headers, scope.map(|scope| scope.0), &site).await?;
    let policy = site_configuration::ingestion_policy(&state, &site).await?;
    if policy.tracking_paused {
        return Err(ApiError::Forbidden(
            "Event tracking is paused for this site".to_owned(),
        ));
    }
    if has_privacy_signal(&headers) {
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({
                "accepted": 0,
                "event_ids": [],
                "status": "privacy_signal_honored"
            })),
        ));
    }

    let user_agent_header = headers
        .get("user-agent")
        .and_then(|value| value.to_str().ok());
    // Every event in an SDK batch comes from the same HTTP request. Parse the
    // user agent and consult MaxMind once, then reuse those immutable facts.
    let parsed_user_agent = parse_user_agent(user_agent_header);
    let location = state.geoip.lookup(client_ip);
    let mut requests = payload.into_requests()?;
    // Only a resolved, authorized canonical site can allocate per-site buckets.
    let site_ceiling = 100_000;
    rate_limit::check_sdk_events_for_site(
        &state,
        client_ip,
        &site.id,
        requests.len(),
        site_ceiling,
    )
    .await?;
    let now = Utc::now();
    let retention_stamp = retention::stamp_for_ingest(&state, &site, now).await?;
    let mut rows = Vec::with_capacity(requests.len());

    for request in &mut requests {
        request.site_id.clone_from(&site.tracking_id);
    }
    let event_context = IngestEventContext {
        client_ip,
        location,
        now,
        parsed_user_agent,
        retention_days: policy.retention_days,
        retention_stamp,
        user_agent_header,
    };
    for request in requests {
        rows.push(build_event_row(&state, request, &event_context)?);
    }
    let event_ids = rows
        .iter()
        .map(|row| row.event_id.clone())
        .collect::<Vec<_>>();
    let catalog_values = rows
        .iter()
        .map(|row| {
            (
                row.site_id.clone(),
                row.event_type.clone(),
                row.event_name.clone(),
                row.occurred_at.clone(),
            )
        })
        .collect::<Vec<_>>();
    state
        .clickhouse
        .insert_events_after(&site.tracking_id, rows, || {
            sites::ensure_site_accepts_ingest(&state.sqlite, &site.id)
        })
        .await?;
    let catalog_entries = catalog_values
        .iter()
        .map(
            |(site_id, event_type, event_name, occurred_at)| sqlite::CatalogEvent {
                event_name,
                event_type,
                occurred_at,
                site_id,
            },
        )
        .collect::<Vec<_>>();
    if let Err(error) = sqlite::record_event_catalog_batch(&state.sqlite, &catalog_entries).await {
        tracing::warn!(%error, site_id = %site.tracking_id, "failed to update event catalog");
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": event_ids.len(),
            "dropped": 0,
            "event_ids": event_ids,
            "status": "accepted"
        })),
    ))
}

struct IngestEventContext<'a> {
    client_ip: std::net::IpAddr,
    location: GeoLocation,
    now: chrono::DateTime<Utc>,
    parsed_user_agent: ParsedUserAgent,
    retention_days: u16,
    retention_stamp: retention::RetentionStamp,
    user_agent_header: Option<&'a str>,
}

fn build_event_row(
    state: &AppState,
    request: IngestRequest,
    context: &IngestEventContext<'_>,
) -> Result<EventRow, ApiError> {
    validate_event(&request.event, context.now)?;
    let occurred_at = request.event.timestamp.unwrap_or_else(Utc::now);
    let timezone = request.event.environment.timezone.as_deref();
    let ids = anonymous_ids(
        &request.site_id,
        context.client_ip,
        context.user_agent_header,
        timezone,
        &state.settings.hash_salt,
        occurred_at,
    );
    EventRow::from_request(
        request,
        EventRowContext {
            anonymous_ids: ids,
            location: context.location.clone(),
            occurred_at,
            received_at: context.now,
            retention_days: context.retention_days,
            retention_stamp: context.retention_stamp,
            user_agent: context.parsed_user_agent.clone(),
        },
    )
}

fn has_privacy_signal(headers: &HeaderMap) -> bool {
    ["dnt", "sec-gpc"].iter().any(|name| {
        headers.get_all(*name).iter().any(|value| {
            value
                .to_str()
                .ok()
                .is_some_and(|value| value.split(',').any(|value| value.trim() == "1"))
        })
    })
}

async fn authorize_ingest(
    state: &AppState,
    headers: &HeaderMap,
    scope: Option<IngestScope>,
    site: &ActiveSite,
) -> Result<(), ApiError> {
    if let Some(scope) = scope {
        return scope.allows_site(site).then_some(()).ok_or_else(|| {
            ApiError::Forbidden("The supplied key is not authorized for this site".to_owned())
        });
    }

    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ApiError::Forbidden(
                "Browser ingestion requires an Origin allowed by the site".to_owned(),
            )
        })?;
    if !sites::origin_is_allowed(
        &state.sqlite,
        &site.id,
        origin,
        state.settings.allow_loopback_origins,
    )
    .await?
    {
        return Err(ApiError::Forbidden(
            "Origin is not allowed for this site".to_owned(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::has_privacy_signal;

    #[test]
    fn dnt_and_global_privacy_control_are_honored() {
        let mut headers = HeaderMap::new();
        assert!(!has_privacy_signal(&headers));

        headers.insert("dnt", HeaderValue::from_static("1"));
        assert!(has_privacy_signal(&headers));

        headers.remove("dnt");
        headers.insert("sec-gpc", HeaderValue::from_static("1"));
        assert!(has_privacy_signal(&headers));

        headers.insert("sec-gpc", HeaderValue::from_static("0"));
        assert!(!has_privacy_signal(&headers));

        headers.append("sec-gpc", HeaderValue::from_static("1"));
        assert!(has_privacy_signal(&headers));
    }
}
