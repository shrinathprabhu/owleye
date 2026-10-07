use crate::{
    auth,
    settings::{AuthSettings, Settings},
    storage::{clickhouse::ClickHouse, sqlite},
    AppState,
};
use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    http::{Request, StatusCode},
    response::Response,
    Router,
};
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceExt;
pub(crate) async fn test_state() -> AppState {
    let sqlite = sqlite::connect("sqlite::memory:").await.unwrap();
    AppState {
        live_cache: Default::default(),
        public_overview_cache: Default::default(),
        clickhouse: ClickHouse::new("http://127.0.0.1:1".into()).unwrap(),
        geoip: Arc::new(crate::privacy::geoip::GeoIp::open(None).unwrap()),
        http: reqwest::Client::new(),
        rate_limiter: Default::default(),
        settings: Settings {
            ai: Default::default(),
            allow_loopback_origins: true,
            api_addr: "127.0.0.1:8527".parse().unwrap(),
            auth: AuthSettings::default(),
            clickhouse_url: "http://127.0.0.1:1".into(),
            hash_salt: "self-hosted-test-salt-at-least-32-characters".into(),
            maxmind_db: None,
            sqlite_url: "sqlite::memory:".into(),
            trusted_proxies: vec![],
        },
        sqlite,
    }
}
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: &str,
    body: Value,
    origin: &str,
) -> Response {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("origin", origin)
        .header("content-type", "application/json")
        .header("cookie", cookie)
        .extension(ConnectInfo(
            "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
        ))
        .body(Body::from(body.to_string()))
        .unwrap();
    app.clone().oneshot(req).await.unwrap()
}
async fn json_body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
fn cookie(response: &Response) -> String {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ")
}
async fn login(app: &Router, email: &str, password: &str) -> Response {
    request(
        app,
        "POST",
        "/v1/auth/login",
        "",
        json!({"email":email,"password":password}),
        "http://localhost:8527",
    )
    .await
}
#[tokio::test]
async fn setup_password_auth_admin_membership_and_revocation() {
    let state = test_state().await;
    assert!(
        auth::bootstrap(&state.sqlite, "root@example.test", "1234567")
            .await
            .is_err()
    );
    auth::bootstrap(&state.sqlite, "root@example.test", "aaaaaaaa")
        .await
        .unwrap();
    assert!(
        auth::bootstrap(&state.sqlite, "again@example.test", "aaaaaaaa")
            .await
            .is_err()
    );
    assert!(
        sqlx::query("INSERT INTO organizations(id,name,slug) VALUES('other','Other','other')")
            .execute(&state.sqlite)
            .await
            .is_err()
    );
    let stored: String = sqlx::query_scalar("SELECT password_hash FROM users")
        .fetch_one(&state.sqlite)
        .await
        .unwrap();
    assert!(stored.starts_with("$argon2id$"));
    assert!(!stored.contains("aaaaaaaa"));
    let app = crate::routes::router(state.clone());
    assert_eq!(
        login(&app, "unknown@example.test", "aaaaaaaa")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "root@example.test", "wrongpass").await.status(),
        StatusCode::UNAUTHORIZED
    );
    let signed = login(&app, "root@example.test", "aaaaaaaa").await;
    assert_eq!(signed.status(), StatusCode::OK);
    let root = cookie(&signed);
    assert!(signed
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("HttpOnly"));
    let session = json_body(
        request(
            &app,
            "GET",
            "/v1/auth/session",
            &root,
            json!(null),
            "http://localhost:8527",
        )
        .await,
    )
    .await;
    assert_eq!(session["user"]["is_admin"], true);
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/admin/users",
            &root,
            json!({"email":"reader@example.test","password":"bbbbbbbb"}),
            "https://untrusted.example"
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let added = request(
        &app,
        "POST",
        "/v1/admin/users",
        &root,
        json!({"email":"reader@example.test","password":"bbbbbbbb"}),
        "http://localhost:8527",
    )
    .await;
    assert_eq!(added.status(), StatusCode::OK);
    let user_id = json_body(added).await["id"].as_str().unwrap().to_owned();
    let signed = login(&app, "reader@example.test", "bbbbbbbb").await;
    assert_eq!(signed.status(), StatusCode::OK);
    let reader = cookie(&signed);
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/admin/users",
            &reader,
            json!(null),
            "http://localhost:8527"
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    for i in 0..12 {
        let site = request(
            &app,
            "POST",
            "/v1/sites",
            &root,
            json!({"name":format!("App {i}"),"domain":format!("app{i}.example.test")}),
            "http://localhost:8527",
        )
        .await;
        assert_eq!(site.status(), StatusCode::OK, "{}", json_body(site).await);
    }
    let sites = json_body(
        request(
            &app,
            "GET",
            "/v1/sites",
            &reader,
            json!(null),
            "http://localhost:8527",
        )
        .await,
    )
    .await;
    assert_eq!(sites.as_array().unwrap().len(), 12);
    assert!(sites
        .as_array()
        .unwrap()
        .iter()
        .all(|site| site["permissions"]["ai_use"] == true));
    let changed = request(
        &app,
        "PUT",
        "/v1/auth/password",
        &reader,
        json!({"current_password":"bbbbbbbb","password":"cccccccc"}),
        "http://localhost:8527",
    )
    .await;
    assert_eq!(changed.status(), StatusCode::OK);
    let session = json_body(
        request(
            &app,
            "GET",
            "/v1/auth/session",
            &reader,
            json!(null),
            "http://localhost:8527",
        )
        .await,
    )
    .await;
    assert_eq!(session["authenticated"], false);
    assert_eq!(
        login(&app, "reader@example.test", "bbbbbbbb")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let signed = login(&app, "reader@example.test", "cccccccc").await;
    assert_eq!(signed.status(), StatusCode::OK);
    let reader = cookie(&signed);
    let start = json_body(
        request(
            &app,
            "POST",
            "/v1/auth/step-up/start",
            &root,
            json!({}),
            "http://localhost:8527",
        )
        .await,
    )
    .await;
    assert_eq!(start["method"], "password");
    let verified = request(
        &app,
        "POST",
        "/v1/auth/step-up/verify",
        &root,
        json!({"challenge_id":start["challenge_id"],"code":"aaaaaaaa"}),
        "http://localhost:8527",
    )
    .await;
    assert_eq!(verified.status(), StatusCode::OK);
    let changed = request(
        &app,
        "PUT",
        &format!("/v1/admin/users/{user_id}"),
        &root,
        json!({"enabled":false}),
        "http://localhost:8527",
    )
    .await;
    assert_eq!(changed.status(), StatusCode::OK);
    assert_eq!(
        login(&app, "reader@example.test", "cccccccc")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/sites",
            &reader,
            json!(null),
            "http://localhost:8527"
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM organizations")
        .fetch_one(&state.sqlite)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
#[tokio::test]
async fn console_static_routes_and_api_origins_are_isolated() {
    let state = test_state().await;
    let dir = std::env::temp_dir().join(format!("owleye-console-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("index.html"), "<html>OwlEye</html>").unwrap();
    std::fs::write(dir.join("test.js"), "export {};").unwrap();
    let app = crate::routes::router_with_console(state, dir.to_str().unwrap().into());
    let page = request(
        &app,
        "GET",
        "/profile",
        "",
        json!(null),
        "http://localhost:8527",
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK);
    assert!(page.headers()["content-type"]
        .to_str()
        .unwrap()
        .contains("text/html"));
    assert!(page.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("connect-src 'self'"));
    let missing = request(
        &app,
        "GET",
        "/v1/does-not-exist",
        "",
        json!(null),
        "http://localhost:8527",
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert!(missing.headers()["content-type"]
        .to_str()
        .unwrap()
        .contains("application/json"));
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/auth/login",
            "",
            json!({"email":"a@example.test","password":"password"}),
            "https://other.example"
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let same = request(
        &app,
        "GET",
        "/v1/auth/session",
        "",
        json!(null),
        "http://localhost:8527",
    )
    .await;
    assert_eq!(
        same.headers()["access-control-allow-origin"],
        "http://localhost:8527"
    );
    let other = request(
        &app,
        "GET",
        "/v1/auth/session",
        "",
        json!(null),
        "https://other.example",
    )
    .await;
    assert!(!other.headers().contains_key("access-control-allow-origin"));
    std::fs::remove_dir_all(dir).unwrap();
}
#[tokio::test]
async fn new_events_never_get_an_expiry() {
    let state = test_state().await;
    let site = crate::sites::ActiveSite {
        id: "site".into(),
        tracking_id: "owl_test".into(),
        organization_id: Some("default".into()),
        created_by_user_id: None,
        team_id: None,
    };
    let stamp = crate::retention::stamp_for_ingest(&state, &site, chrono::Utc::now())
        .await
        .unwrap();
    assert!(stamp.active_until.is_none());
    assert!(stamp.purge_after.is_none());
}

#[tokio::test]
async fn local_admin_recovery_revokes_sessions_and_never_promotes_users() {
    let state = test_state().await;
    auth::bootstrap(&state.sqlite, "root@example.test", "password")
        .await
        .unwrap();
    let app = crate::routes::router(state.clone());
    let signed = login(&app, "root@example.test", "password").await;
    let root = cookie(&signed);
    sqlx::query("UPDATE users SET totp_secret='test-secret',status='disabled'")
        .execute(&state.sqlite)
        .await
        .unwrap();
    assert!(
        auth::recover_admin(&state.sqlite, "other@example.test", "new pass")
            .await
            .is_err()
    );
    auth::recover_admin(&state.sqlite, "root@example.test", "new pass")
        .await
        .unwrap();
    let secret: Option<String> = sqlx::query_scalar("SELECT totp_secret FROM users")
        .fetch_one(&state.sqlite)
        .await
        .unwrap();
    assert!(secret.is_none());
    assert_eq!(
        login(&app, "root@example.test", "password").await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "root@example.test", "new pass").await.status(),
        StatusCode::OK
    );
    let old_session = json_body(
        request(
            &app,
            "GET",
            "/v1/auth/session",
            &root,
            json!(null),
            "http://localhost:8527",
        )
        .await,
    )
    .await;
    assert_eq!(old_session["authenticated"], false);
}
