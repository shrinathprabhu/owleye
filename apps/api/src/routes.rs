use axum::{
    extract::{DefaultBodyLimit, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderName, HeaderValue, Method,
    },
    middleware,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use tower_http::{
    cors::{AllowOrigin, Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{
    account, ai, api_keys, auth, dashboard_sharing, dashboards, developer_access, errors::ApiError,
    events, ingest, insights, members, rate_limit, rules, security, site_configuration, sites,
    stats, surface, AppState,
};

const INGEST_BODY_LIMIT_BYTES: usize = 1024 * 1024;

pub fn router(state: AppState) -> Router {
    router_with_console(
        state,
        std::env::var("OWLEYE_CONSOLE_DIR")
            .unwrap_or_else(|_| "apps/console/.output/public".into()),
    )
}
pub(crate) fn router_with_console(state: AppState, console_dir: String) -> Router {
    let health_routes = Router::new()
        .route("/health", get(health))
        .route("/health/live", get(surface::health_live))
        .route("/health/ready", get(health_ready))
        .layer(health_cors(&state));

    let public_routes = Router::new()
        .route("/version", get(surface::version))
        .route("/v1/openapi.json", get(developer_access::openapi))
        .route(
            "/v1/developer/sites/{siteId}/stats",
            get(developer_access::developer_stats).layer(middleware::from_fn_with_state(
                state.clone(),
                rate_limit::developer,
            )),
        )
        .route(
            "/v1/events",
            post(ingest::ingest_event)
                .layer(DefaultBodyLimit::max(INGEST_BODY_LIMIT_BYTES))
                .layer::<_, std::convert::Infallible>(middleware::from_fn_with_state(
                    state.clone(),
                    rate_limit::sdk_ingest,
                ))
                .layer(sdk_cors()),
        )
        .route(
            "/v1/events/api",
            post(ingest::ingest_event)
                .layer(DefaultBodyLimit::max(INGEST_BODY_LIMIT_BYTES))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    api_keys::require_api_key,
                ))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    rate_limit::sdk_ingest,
                )),
        )
        .route(
            "/track",
            post(ingest::ingest_event)
                .layer(DefaultBodyLimit::max(INGEST_BODY_LIMIT_BYTES))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    api_keys::require_api_key,
                ))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    rate_limit::sdk_ingest,
                )),
        )
        .route(
            "/v1/rules",
            get(rules::list_rules)
                .layer::<_, std::convert::Infallible>(middleware::from_fn_with_state(
                    state.clone(),
                    rate_limit::sdk_rules,
                ))
                .layer(sdk_cors()),
        )
        .route(
            "/v1/developer/sites/{siteId}/events",
            get(developer_access::developer_events).layer(middleware::from_fn_with_state(
                state.clone(),
                rate_limit::developer,
            )),
        )
        .route(
            "/v1/developer/sites/{siteId}/prompt",
            post(developer_access::developer_prompt)
                .layer(DefaultBodyLimit::max(20 * 1024))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    rate_limit::developer,
                )),
        );

    // Keep the legacy aliases used by the current console while making `/v1/auth/*`
    // the canonical public surface.
    let auth_routes = Router::new()
        .route("/v1/auth/login", post(auth::login))
        .route("/auth/tfa/verify", post(auth::verify_tfa))
        .route("/v1/auth/refresh", post(auth::refresh))
        .route("/v1/auth/2fa/verify", post(auth::verify_two_factor))
        .route("/v1/auth/tfa/verify", post(auth::verify_tfa))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::auth,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            security::require_console_origin,
        ))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(console_cors(&state));

    let shared_overview_routes = Router::new()
        .route(
            "/v1/public/overview",
            get(crate::public_dashboard::overview),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::public_overview,
        ))
        .route_layer(middleware::from_fn(crate::public_dashboard::require_origin))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::predicate(|origin, _| {
                    origin
                        .to_str()
                        .is_ok_and(crate::public_dashboard::allowed_origin)
                }))
                .allow_methods([Method::GET, Method::OPTIONS])
                .expose_headers([HeaderName::from_static("retry-after")]),
        );

    let console_routes = Router::new()
        .route("/v1/auth/password", put(auth::change_password))
        .route(
            "/v1/admin/users",
            get(auth::list_users).post(auth::create_user),
        )
        .route("/v1/admin/users/{userId}", put(auth::update_user))
        .route("/v1/system/release", get(crate::updates::latest))
        .route(
            "/v1/sites/{siteId}/public-overview",
            get(crate::public_dashboard::settings).put(crate::public_dashboard::update),
        )
        .route("/auth/logout", post(auth::logout))
        .route("/auth/logout-all", post(auth::logout_all))
        .route("/auth/tfa", delete(auth::disable_two_factor))
        .route("/auth/tfa/enable", post(auth::enable_two_factor))
        .route("/auth/tfa/register", post(auth::setup_two_factor))
        .route("/user/me", get(surface::user_me))
        .route("/v1/auth/2fa/disable", post(auth::disable_two_factor))
        .route("/v1/auth/2fa/enable", post(auth::enable_two_factor))
        .route("/v1/auth/2fa/setup", post(auth::setup_two_factor))
        .route("/v1/auth/logout", post(auth::logout))
        .route("/v1/auth/logout-all", post(auth::logout_all))
        .route("/v1/auth/step-up/start", post(auth::start_step_up))
        .route("/v1/auth/step-up/verify", post(auth::verify_step_up))
        .route(
            "/v1/auth/onboarding/2fa/skip",
            post(auth::skip_two_factor_onboarding),
        )
        .route("/v1/auth/session", get(auth::get_session))
        .route("/v1/account", delete(account::delete_account))
        .route(
            "/v1/account/profile",
            get(account::get_profile).put(account::update_profile),
        )
        .route("/v1/account/export", get(account::export_account))
        .route("/v1/stats/overview", get(stats::stats_overview))
        .route("/v1/sites/{siteId}/live", get(crate::live::overview))
        .route(
            "/v1/sites/{siteId}/uptime",
            get(crate::uptime::list).post(crate::uptime::create),
        )
        .route(
            "/v1/sites/{siteId}/uptime/{monitorId}",
            put(crate::uptime::update).delete(crate::uptime::delete),
        )
        .route(
            "/v1/sites/{siteId}/uptime/{monitorId}/history",
            get(crate::uptime::history),
        )
        .route("/v1/sites", get(sites::list_sites).post(sites::create_site))
        .route(
            "/v1/sites/{siteId}",
            get(sites::get_site)
                .put(sites::update_site)
                .delete(sites::delete_site),
        )
        .route(
            "/v1/sites/{siteId}/domains",
            get(sites::list_domains).post(sites::create_domain),
        )
        .route(
            "/v1/sites/{siteId}/domains/{domainId}",
            delete(sites::delete_domain),
        )
        .route(
            "/v1/sites/{siteId}/api-keys",
            get(api_keys::list_api_keys).post(api_keys::create_api_key),
        )
        .route(
            "/v1/sites/{siteId}/api-keys/{keyId}",
            put(api_keys::update_api_key).delete(api_keys::revoke_api_key),
        )
        .route(
            "/v1/sites/{siteId}/rules",
            get(rules::list_site_rules).post(rules::create_site_rule),
        )
        .route(
            "/v1/sites/{siteId}/rules/{ruleId}",
            get(rules::get_site_rule)
                .put(rules::update_site_rule)
                .delete(rules::delete_site_rule),
        )
        .route("/v1/sites/{siteId}/events", get(events::explore_events))
        .route(
            "/v1/sites/{siteId}/campaigns",
            get(insights::list_campaigns).post(insights::create_campaign),
        )
        .route(
            "/v1/sites/{siteId}/campaigns/{campaignId}",
            delete(insights::delete_campaign),
        )
        .route(
            "/v1/sites/{siteId}/performance",
            get(insights::performance_overview),
        )
        .route(
            "/v1/sites/{siteId}/members",
            get(members::list_members).post(members::add_member),
        )
        .route(
            "/v1/sites/{siteId}/members/{userId}",
            put(members::update_member).delete(members::remove_member),
        )
        .route(
            "/v1/sites/{siteId}/settings",
            get(site_configuration::get_site_configuration)
                .put(site_configuration::update_site_configuration),
        )
        .route(
            "/v1/sites/{siteId}/developer",
            get(developer_access::get_developer_settings)
                .put(developer_access::update_developer_settings),
        )
        .route(
            "/v1/sites/{siteId}/developer/keys",
            get(developer_access::list_developer_keys).post(developer_access::create_developer_key),
        )
        .route(
            "/v1/sites/{siteId}/developer/keys/{keyId}",
            delete(developer_access::revoke_developer_key),
        )
        .route(
            "/v1/sites/{siteId}/developer/backup",
            get(developer_access::download_control_plane_backup),
        )
        .route(
            "/v1/sites/{siteId}/developer/events-export",
            get(developer_access::download_event_export),
        )
        .route(
            "/v1/sites/{siteId}/ai",
            get(ai::get_ai_mode).put(ai::update_ai_mode),
        )
        .route("/v1/sites/{siteId}/prompt", post(ai::consume_prompt))
        .route(
            "/v1/sites/{siteId}/dashboards",
            get(dashboards::list_dashboards).post(dashboards::create_dashboard),
        )
        .route(
            "/v1/sites/{siteId}/dashboards/preview",
            post(dashboards::preview_draft_widget),
        )
        .route(
            "/v1/sites/{siteId}/dashboard-invites",
            get(dashboard_sharing::list_dashboard_invitations),
        )
        .route(
            "/v1/sites/{siteId}/dashboard-invites/{dashboardId}",
            get(dashboard_sharing::preview_dashboard_invitation),
        )
        .route(
            "/v1/sites/{siteId}/dashboard-invites/{dashboardId}/accept",
            post(dashboard_sharing::accept_dashboard_invitation),
        )
        .route(
            "/v1/sites/{siteId}/dashboard-invites/{dashboardId}/dismiss",
            post(dashboard_sharing::dismiss_dashboard_invitation),
        )
        .route(
            "/v1/sites/{siteId}/dashboards/{dashboardId}",
            get(dashboards::get_dashboard)
                .put(dashboards::update_dashboard)
                .delete(dashboards::delete_dashboard),
        )
        .route(
            "/v1/sites/{siteId}/dashboards/{dashboardId}/shares",
            get(dashboard_sharing::list_dashboard_shares)
                .post(dashboard_sharing::share_dashboard)
                .delete(dashboard_sharing::revoke_all_dashboard_shares),
        )
        .route(
            "/v1/sites/{siteId}/dashboards/{dashboardId}/shares/me",
            delete(dashboard_sharing::remove_dashboard_for_me),
        )
        .route(
            "/v1/sites/{siteId}/dashboards/{dashboardId}/shares/{userId}",
            delete(dashboard_sharing::revoke_dashboard_share),
        )
        .route(
            "/v1/sites/{siteId}/dashboards/{dashboardId}/widgets",
            post(dashboards::create_widget),
        )
        .route(
            "/v1/sites/{siteId}/dashboards/{dashboardId}/widgets/preview",
            post(dashboards::preview_widget),
        )
        .route(
            "/v1/sites/{siteId}/dashboards/{dashboardId}/widgets/{widgetId}",
            put(dashboards::update_widget).delete(dashboards::delete_widget),
        )
        // Compatibility aliases for the earlier console contract.
        .route("/sites", get(sites::list_sites).post(sites::create_site))
        .route(
            "/sites/{siteId}",
            get(sites::get_site)
                .put(sites::update_site)
                .delete(sites::delete_site),
        )
        .route(
            "/sites/{siteId}/domains",
            get(sites::list_domains).post(sites::create_domain),
        )
        .route(
            "/sites/{siteId}/domains/{domainId}",
            delete(sites::delete_domain),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::console,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            security::require_console_origin,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            security::require_console_access,
        ))
        .layer(console_cors(&state));

    public_routes
        .merge(health_routes)
        .merge(shared_overview_routes)
        .merge(auth_routes)
        .merge(console_routes)
        .route(
            "/v1/{*path}",
            axum::routing::any(|| async {
                (
                    axum::http::StatusCode::NOT_FOUND,
                    Json(json!({"error":"not_found"})),
                )
            }),
        )
        .fallback_service(tower_http::services::ServeDir::new(&console_dir).fallback(
            tower_http::services::ServeFile::new(format!("{console_dir}/index.html")),
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security::security_headers,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    sqlx::query("SELECT 1").execute(&state.sqlite).await?;

    Ok(Json(json!({
        "service": "owleye-api",
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    })))
}

async fn health_ready(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    sqlx::query("SELECT 1").execute(&state.sqlite).await?;
    let _: Vec<ReadyRow> = state
        .clickhouse
        .query_json_each_row("SELECT 1 AS value")
        .await?;

    Ok(Json(json!({
        "checks": {
            "clickhouse": "ok",
            "sqlite": "ok"
        },
        "service": "owleye-api",
        "status": "ready",
        "version": env!("CARGO_PKG_VERSION")
    })))
}

#[derive(serde::Deserialize)]
struct ReadyRow {
    #[allow(dead_code)]
    value: u8,
}

fn sdk_cors() -> CorsLayer {
    // The site ID is in the POST body, unavailable during preflight. Handlers
    // authorize actual ingestion/rule reads against the site's allowed domains.
    // Secret-key and console routes never use this public policy.
    CorsLayer::new()
        .allow_headers([CONTENT_TYPE])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .expose_headers([HeaderName::from_static("retry-after")])
        .allow_origin(Any)
}

fn health_cors(state: &AppState) -> CorsLayer {
    // Public status reads use the same trusted origins without enabling cookies.
    let origins = state
        .settings
        .auth
        .console_origins()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::HEAD, Method::OPTIONS])
        .allow_headers([CONTENT_TYPE])
}

fn console_cors(state: &AppState) -> CorsLayer {
    let origins = state
        .settings
        .auth
        .console_origins()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();

    CorsLayer::new()
        .allow_headers([CONTENT_TYPE])
        .allow_methods([
            Method::DELETE,
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::OPTIONS,
        ])
        .allow_origin(AllowOrigin::list(origins))
        .expose_headers([
            CONTENT_DISPOSITION,
            HeaderName::from_static("retry-after"),
            HeaderName::from_static("x-owleye-row-count"),
            HeaderName::from_static("x-owleye-next-before"),
            HeaderName::from_static("x-owleye-next-event-id"),
        ])
        .allow_credentials(true)
}
