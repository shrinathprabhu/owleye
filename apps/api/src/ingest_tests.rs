use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    http::{Request, StatusCode},
    routing::post,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{
    privacy::geoip::GeoIp,
    settings::{AuthSettings, Settings},
    storage::{clickhouse::ClickHouse, sqlite},
    AppState,
};

type StoredRows = Arc<Mutex<Vec<Value>>>;

// Exercise the real insertion queue against an isolated HTTP sink, never a live database.
pub(crate) async fn test_state() -> (AppState, StoredRows) {
    let rows: StoredRows = Arc::default();
    let received = rows.clone();
    let sink = Router::new().route(
        "/",
        post(move |body: String| {
            let received = received.clone();
            async move {
                let mut lines = body.lines();
                assert!(lines
                    .next()
                    .unwrap()
                    .starts_with("INSERT INTO owleye_events"));
                received
                    .lock()
                    .unwrap()
                    .extend(lines.map(|line| serde_json::from_str::<Value>(line).unwrap()));
                StatusCode::OK
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let clickhouse_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, sink).await.unwrap();
    });
    let settings = Settings {
        ai: Default::default(),
        allow_loopback_origins: false,
        api_addr: "127.0.0.1:0".parse().unwrap(),
        auth: AuthSettings::default(),

        clickhouse_url: clickhouse_url.clone(),
        hash_salt: "ingestion-regression-test-salt-not-production".into(),
        maxmind_db: None,
        sqlite_url: "sqlite::memory:".into(),
        trusted_proxies: vec![],
    };
    let sqlite = sqlite::connect(&settings.sqlite_url).await.unwrap();
    sqlx::query(
        "INSERT INTO organizations (id, name, slug) VALUES ('ingest-org', 'Ingest', 'ingest')",
    )
    .execute(&sqlite)
    .await
    .unwrap();
    (
        AppState {
            live_cache: Default::default(),
            public_overview_cache: Default::default(),
            clickhouse: ClickHouse::new(clickhouse_url).unwrap(),
            geoip: Arc::new(GeoIp::open(None).unwrap()),
            http: reqwest::Client::new(),
            rate_limiter: Default::default(),
            settings,
            sqlite,
        },
        rows,
    )
}

async fn send(
    app: &Router,
    payload: Value,
    signals: &[(&str, &str)],
    origin: &str,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/v1/events")
        .header("content-type", "application/json")
        .header("origin", origin)
        .extension(ConnectInfo(
            "127.0.0.1:39000".parse::<SocketAddr>().unwrap(),
        ));
    for (name, value) in signals {
        request = request.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn privacy_headers_do_not_suppress_single_or_batch_ingestion() {
    let (state, stored) = test_state().await;
    sqlx::query("INSERT INTO sites (id, organization_id, tracking_id, name, domain) VALUES ('ingest-site', 'ingest-org', 'owl_ingest', 'Ingest', 'example.test')").execute(&state.sqlite).await.unwrap();
    let app = crate::routes::router(state.clone());
    let event =
        json!({"type":"pageview", "name":"pageview", "page":{"url":"https://example.test/"}});
    let mut count = 0;
    for signals in [
        vec![],
        vec![("dnt", "1")],
        vec![("sec-gpc", "1")],
        vec![("dnt", "1"), ("sec-gpc", "1")],
        vec![("sec-gpc", "0, 1")],
    ] {
        for batch in [false, true] {
            let payload = if batch {
                json!({"site_id":"owl_ingest", "events":[event.clone(),event.clone()]})
            } else {
                json!({"site_id":"owl_ingest", "event":event})
            };
            let (status, body) = send(&app, payload, &signals, "https://example.test").await;
            let accepted = if batch { 2 } else { 1 };
            assert_eq!(status, StatusCode::ACCEPTED, "{signals:?}: {body}");
            assert_eq!(body["status"], "accepted");
            assert_eq!(body["accepted"], accepted);
            assert_eq!(body["event_ids"].as_array().unwrap().len(), accepted);
            count += accepted;
            assert_eq!(stored.lock().unwrap().len(), count);
        }
    }
    let signals = [("dnt", "1"), ("sec-gpc", "1")];
    let (status, _) = send(
        &app,
        json!({"site_id":"owl_ingest", "event":{"type":"pageview", "name":""}}),
        &signals,
        "https://example.test",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(
        &app,
        json!({"site_id":"owl_ingest", "event":event}),
        &signals,
        "https://untrusted.test",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    sqlx::query("INSERT INTO site_settings (site_id, tracking_paused) VALUES ('ingest-site', 1)")
        .execute(&state.sqlite)
        .await
        .unwrap();
    let (status, _) = send(
        &app,
        json!({"site_id":"owl_ingest", "event":event}),
        &signals,
        "https://example.test",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(stored.lock().unwrap().len(), count);
}
