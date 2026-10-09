use super::*;
use crate::storage::clickhouse::ClickHouse;
use axum::{
    body::Body,
    http::{Request as HttpRequest, StatusCode},
    routing::post,
    Router,
};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

async fn fixture() -> AppState {
    let state = crate::self_hosted_tests::test_state().await;
    for sql in [
        "INSERT INTO users(id,email) VALUES('owner','owner@example.test')",
        "INSERT INTO organizations(id,name,slug) VALUES('org','Org','org')",
        "INSERT INTO sites(id,organization_id,tracking_id,name,domain,created_by_user_id) VALUES('site','org','owl_shared','Shared app','example.test','owner')",
    ] { sqlx::query(sql).execute(&state.sqlite).await.unwrap(); }
    state
}
fn query(id: &str) -> PublicQuery {
    PublicQuery {
        site_id: Some(id.into()),
        site: None,
        days: None,
    }
}
fn config() -> ShareConfig {
    ShareConfig {
        enabled: true,
        metrics: vec![Metric::Visitors],
        traffic: false,
        urls: vec!["https://example.test/".into()],
        ..Default::default()
    }
}
async fn save(state: &AppState, id: &str, config: ShareConfig) {
    let mut tx = state.sqlite.begin().await.unwrap();
    sqlx::query("INSERT INTO public_overview_shares(site_id,config_json,revision) VALUES(?,?,?) ON CONFLICT(site_id) DO UPDATE SET config_json=excluded.config_json, revision=excluded.revision")
        .bind(id).bind(serde_json::to_string(&config).unwrap()).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.unwrap();
    sqlx::query("DELETE FROM public_overview_urls WHERE site_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    if config.enabled {
        for url in config.urls {
            sqlx::query("INSERT INTO public_overview_urls(site_id,url) VALUES(?,?)")
                .bind(id)
                .bind(url)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
    }
    tx.commit().await.unwrap();
}
async fn mock_clickhouse(
    state: &mut AppState,
    revoke: bool,
) -> (tokio::task::JoinHandle<()>, Arc<Mutex<Vec<String>>>) {
    let queries = Arc::new(Mutex::new(Vec::new()));
    let captured = queries.clone();
    let pool = state.sqlite.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app=Router::new().route("/",post(move |body:String|{let captured=captured.clone();let pool=pool.clone();async move {
        captured.lock().unwrap().push(body.clone());
        if revoke {sqlx::query("UPDATE public_overview_shares SET revision='revoked',config_json=json_set(config_json,'$.enabled',json('false'))").execute(&pool).await.unwrap();}
        if body.contains("GROUP BY name") {"{\"name\":\"IN\",\"count\":7,\"visitors\":2,\"secret\":\"private\"}\n".into()}
        else if body.contains("GROUP BY date") {format!("{{\"date\":\"{}\",\"visitors\":6,\"events\":99,\"secret\":\"private\"}}\n",chrono::Utc::now().date_naive())}
        else {"{\"visitors\":6,\"pageviews\":80,\"events\":99,\"audience_pageviews\":80,\"audience_visitors\":6,\"secret\":\"private\"}\n".into()}
    }}));
    state.clickhouse = ClickHouse::new(format!("http://{addr}")).unwrap();
    (
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }),
        queries,
    )
}

#[tokio::test]
async fn cached_public_reads_share_aliases_but_recheck_permissions_and_revision() {
    let mut state = fixture().await;
    let (server, queries) = mock_clickhouse(&mut state, false).await;
    save(
        &state,
        "site",
        ShareConfig {
            max_days: 30,
            ..config()
        },
    )
    .await;
    let (first, second) = tokio::join!(
        overview(State(state.clone()), Query(query("site"))),
        overview(
            State(state.clone()),
            Query(PublicQuery {
                site_id: None,
                site: Some("https://example.test/".into()),
                days: Some(30),
            })
        ),
    );
    assert_eq!(first.unwrap().0, second.unwrap().0);
    assert_eq!(
        queries.lock().unwrap().len(),
        1,
        "aliases share the canonical cache key"
    );
    assert!(queries.lock().unwrap()[0].contains(&format!(
        "toUInt64({}) AS visitors",
        crate::stats::VISITORS_SQL
    )));
    let mut week = query("site");
    week.days = Some(7);
    let _ = overview(State(state.clone()), Query(week)).await.unwrap();
    assert_eq!(
        queries.lock().unwrap().len(),
        2,
        "different ranges never share results"
    );
    let response = crate::routes::router(state.clone())
        .oneshot(
            HttpRequest::builder()
                .uri("/v1/public/overview?site_id=site")
                .header("origin", "https://app.owleye.dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert_eq!(
        queries.lock().unwrap().len(),
        2,
        "router serves the same API cache"
    );

    // Cached data must not bypass lifecycle or URL ownership checks.
    for (deny, restore) in [
        (
            "UPDATE sites SET archived_at=CURRENT_TIMESTAMP WHERE id='site'",
            "UPDATE sites SET archived_at=NULL WHERE id='site'",
        ),
        (
            "UPDATE sites SET erasure_pending_at=CURRENT_TIMESTAMP WHERE id='site'",
            "UPDATE sites SET erasure_pending_at=NULL WHERE id='site'",
        ),
        (
            "UPDATE sites SET deleted_at=CURRENT_TIMESTAMP WHERE id='site'",
            "UPDATE sites SET deleted_at=NULL WHERE id='site'",
        ),
        (
            "UPDATE organizations SET archived_at=CURRENT_TIMESTAMP WHERE id='org'",
            "UPDATE organizations SET archived_at=NULL WHERE id='org'",
        ),
    ] {
        sqlx::query(deny).execute(&state.sqlite).await.unwrap();
        assert!(overview(State(state.clone()), Query(query("site")))
            .await
            .is_err());
        sqlx::query(restore).execute(&state.sqlite).await.unwrap();
    }
    sqlx::query("UPDATE sites SET domain='removed.test' WHERE id='site'")
        .execute(&state.sqlite)
        .await
        .unwrap();
    assert!(overview(
        State(state.clone()),
        Query(PublicQuery {
            site_id: None,
            site: Some("https://example.test/".into()),
            days: None
        })
    )
    .await
    .is_err());
    sqlx::query("UPDATE sites SET domain='example.test' WHERE id='site'")
        .execute(&state.sqlite)
        .await
        .unwrap();
    assert_eq!(
        queries.lock().unwrap().len(),
        2,
        "denied requests never hit ClickHouse"
    );

    save(
        &state,
        "site",
        ShareConfig {
            metrics: vec![Metric::Pageviews],
            ..config()
        },
    )
    .await;
    let changed = overview(State(state.clone()), Query(query("site")))
        .await
        .unwrap()
        .0;
    assert_eq!(changed["data"]["totals"], json!({"pageviews":80}));
    assert_eq!(
        queries.lock().unwrap().len(),
        3,
        "new revision invalidates old data"
    );
    save(&state, "site", ShareConfig::default()).await;
    assert!(overview(State(state.clone()), Query(query("site")))
        .await
        .is_err());
    assert_eq!(queries.lock().unwrap().len(), 3);
    server.abort();
}

#[tokio::test]
async fn in_flight_revocation_discards_data_and_stays_revoked_on_cache_hits() {
    let mut state = fixture().await;
    save(&state, "site", config()).await;
    let (server, queries) = mock_clickhouse(&mut state, true).await;
    assert!(overview(State(state.clone()), Query(query("site")))
        .await
        .is_err());
    assert!(overview(State(state.clone()), Query(query("site")))
        .await
        .is_err());
    assert_eq!(queries.lock().unwrap().len(), 1);
    server.abort();
}
