pub mod account;
pub mod ai;
pub mod api_keys;
pub mod auth;
pub mod dashboard_sharing;
pub mod dashboards;
mod demo;
pub mod developer_access;
pub mod errors;
pub mod events;
pub mod ingest;
pub mod insights;
mod live;
pub mod members;
pub mod models;
pub mod privacy;
pub mod public_dashboard;
pub mod rate_limit;
pub mod retention;
pub mod routes;
pub mod rules;
pub mod security;
pub mod settings;
pub mod site_configuration;
pub mod sites;
pub mod stats;
pub mod storage;
pub mod surface;
pub mod uptime;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use sqlx::SqlitePool;

pub use errors::ApiError;
pub use models::EventRow;

use crate::{
    privacy::geoip::GeoIp,
    rate_limit::RateLimiter,
    settings::Settings,
    storage::{clickhouse::ClickHouse, sqlite},
};

#[derive(Clone)]
pub struct AppState {
    pub(crate) live_cache: live::LiveCache,
    pub(crate) clickhouse: ClickHouse,
    pub(crate) geoip: Arc<GeoIp>,
    pub(crate) http: reqwest::Client,
    pub(crate) rate_limiter: RateLimiter,
    pub(crate) settings: Settings,
    pub(crate) sqlite: SqlitePool,
}

pub async fn run(settings: Settings) -> anyhow::Result<()> {
    let sqlite = sqlite::connect(&settings.sqlite_url).await?;
    let startup_cleanup = auth::cleanup_auth_rows(&sqlite, chrono::Utc::now()).await?;
    if startup_cleanup.total() > 0 {
        tracing::info!(
            removed = startup_cleanup.total(),
            "removed stale authentication records during startup"
        );
    }
    let clickhouse = ClickHouse::new(settings.clickhouse_url.clone())?;
    clickhouse.init().await?;
    let shutdown_clickhouse = clickhouse.clone();

    let state = AppState {
        live_cache: Default::default(),
        clickhouse,
        geoip: Arc::new(GeoIp::open(settings.maxmind_db.as_deref())?),
        http: reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .build()?,
        rate_limiter: RateLimiter::default(),
        settings: settings.clone(),
        sqlite,
    };
    let (cleanup_shutdown, cleanup_shutdown_receiver) = tokio::sync::watch::channel(false);
    let cleanup_task = tokio::spawn(auth::run_periodic_auth_cleanup(
        state.sqlite.clone(),
        cleanup_shutdown_receiver.clone(),
    ));
    let erasure_task = tokio::spawn(retention::run_erasure_worker(
        state.clone(),
        cleanup_shutdown_receiver.clone(),
    ));
    let uptime_task = tokio::spawn(uptime::run(
        state.clone(),
        cleanup_shutdown_receiver.clone(),
    ));
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(settings.api_addr).await?;
    tracing::info!("OwlEye API listening on http://{}", settings.api_addr);

    let server_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await;
    let _ = cleanup_shutdown.send(true);
    match tokio::time::timeout(Duration::from_secs(5), cleanup_task).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(%error, "authentication cleanup task stopped"),
        Err(_) => tracing::warn!("timed out while stopping the authentication cleanup task"),
    }
    for (name, task) in [
        ("uptime scheduler", uptime_task),
        ("site erasures", erasure_task),
    ] {
        match tokio::time::timeout(Duration::from_secs(5), task).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, worker = name, "background worker stopped"),
            Err(_) => tracing::warn!(worker = name, "timed out while stopping background worker"),
        }
    }
    shutdown_clickhouse.shutdown().await;
    server_result?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl+C signal handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "failed to install terminate signal handler"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

mod updates;

#[cfg(test)]
mod self_hosted_tests;
