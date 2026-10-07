use super::*;
use axum::{extract::Query, http::StatusCode, routing::post, Router};

type CapturedInserts = Arc<Mutex<Vec<(HashMap<String, String>, String)>>>;

async fn mock_writer(
    statuses: Vec<StatusCode>,
) -> (
    ClickHouseWriter,
    CapturedInserts,
    tokio::task::JoinHandle<()>,
) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = calls.clone();
    let app = Router::new()
        .route(
            "/",
            post(
                move |Query(params): Query<HashMap<String, String>>, body: String| {
                    let captured = captured.clone();
                    let statuses = statuses.clone();
                    async move {
                        let mut calls = captured.lock().unwrap();
                        let status = statuses[calls.len().min(statuses.len() - 1)];
                        calls.push((params, body));
                        status
                    }
                },
            ),
        )
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (
        ClickHouseWriter {
            auth: None,
            client: Client::new(),
            url,
        },
        calls,
        server,
    )
}

#[tokio::test]
async fn retries_three_times_with_identical_body_and_token() {
    let (writer, calls, server) = mock_writer(vec![
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::BAD_GATEWAY,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::OK,
    ])
    .await;
    writer
        .insert_body_with_retries("fixed batch".into(), &[Duration::ZERO; 3])
        .await
        .unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|call| call == &calls[0]));
    assert_eq!(calls[0].0["insert_deduplicate"], "1");
    assert!(!calls[0].0["insert_deduplication_token"].is_empty());
    server.abort();
}

#[tokio::test]
async fn retries_are_bounded_and_permanent_rejections_are_not_replayed() {
    for (status, expected) in [
        (StatusCode::SERVICE_UNAVAILABLE, 4),
        (StatusCode::BAD_REQUEST, 1),
        (StatusCode::UNAUTHORIZED, 1),
        (StatusCode::FORBIDDEN, 1),
    ] {
        let (writer, calls, server) = mock_writer(vec![status]).await;
        assert!(writer
            .insert_body_with_retries("batch".into(), &[Duration::ZERO; 3])
            .await
            .is_err());
        assert_eq!(calls.lock().unwrap().len(), expected);
        server.abort();
    }
}

#[tokio::test]
async fn connection_failures_are_retryable() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let writer = ClickHouseWriter {
        auth: None,
        client: Client::new(),
        url,
    };
    let start = time::Instant::now();
    assert!(writer
        .insert_body_with_retries("batch".into(), &[Duration::from_millis(10); 3])
        .await
        .is_err());
    assert!(start.elapsed() >= Duration::from_millis(30));
}

#[tokio::test]
#[ignore = "requires disposable loopback ClickHouse"]
async fn real_clickhouse_lost_acknowledgement_does_not_duplicate_rows() {
    let base = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").unwrap();
    let mut url = Url::parse(&base).unwrap();
    assert!(matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]")
    ));
    let database = format!("insert_retry_test_{}", uuid::Uuid::new_v4().simple());
    let admin = ClickHouse::new(base).unwrap();
    admin
        .execute(&format!("CREATE DATABASE {database}"))
        .await
        .unwrap();
    url.query_pairs_mut().append_pair("database", &database);
    let ch = ClickHouse::new(url.to_string()).unwrap();
    ch.init().await.unwrap();
    // Apply startup twice to cover existing deployments as well as fresh tables.
    ch.init().await.unwrap();
    let calls = Arc::new(Mutex::new(0usize));
    let captured = calls.clone();
    let app = Router::new().route(
        "/",
        post(
            move |Query(params): Query<HashMap<String, String>>, body: String| {
                let url = url.clone();
                let captured = captured.clone();
                async move {
                    let result = Client::new()
                        .post(url)
                        .query(&params)
                        .body(body)
                        .send()
                        .await
                        .unwrap();
                    assert!(
                        result.status().is_success(),
                        "{}",
                        result.text().await.unwrap()
                    );
                    let mut calls = captured.lock().unwrap();
                    *calls += 1;
                    // Simulate a gateway error after ClickHouse has already committed.
                    if *calls % 2 == 1 {
                        StatusCode::BAD_GATEWAY
                    } else {
                        StatusCode::OK
                    }
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let writer = ClickHouseWriter {
        auth: None,
        client: Client::new(),
        url: format!("http://{}", listener.local_addr().unwrap()),
    };
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let outcome = async {
        for table in [EVENTS_TABLE, PERFORMANCE_TABLE] {
            let id = uuid::Uuid::new_v4();
            let body = format!("INSERT INTO {table} (event_id, site_id, event_type, occurred_at) SETTINGS async_insert=0 VALUES ('{id}', 'retry-site', 'pageview', now()), ('{id}', 'retry-site', 'pageview', now() - INTERVAL 1 MONTH)");
            writer.insert_body_with_retries(body.clone(), &[Duration::ZERO; 3]).await?;
            let count = ch.query_json_each_row::<serde_json::Value>(&format!("SELECT toUInt32(count()) AS count FROM {table}")).await?;
            assert_eq!(count[0]["count"], 2, "retry must not duplicate either partition in {table}");
            // A separate accepted batch must not accidentally share the token.
            writer.insert_body_with_retries(body, &[Duration::ZERO; 3]).await?;
            let count = ch.query_json_each_row::<serde_json::Value>(&format!("SELECT toUInt32(count()) AS count FROM {table}")).await?;
            assert_eq!(count[0]["count"], 4);
        }
        Ok::<_, anyhow::Error>(())
    }.await;
    server.abort();
    ch.shutdown().await;
    admin
        .execute(&format!("DROP DATABASE {database}"))
        .await
        .unwrap();
    admin.shutdown().await;
    outcome.unwrap();
    assert_eq!(*calls.lock().unwrap(), 8);
}

fn pending_rows(count: usize) -> (InsertCommand, oneshot::Receiver<Result<(), String>>) {
    let (completion, completed) = oneshot::channel();
    let rows = (0..count)
        .map(|i| super::tests::event_row(&format!("queued-{i}")))
        .collect();
    (
        InsertCommand::Batch(PendingInsert {
            completion,
            rows,
            fence: None,
        }),
        completed,
    )
}

#[tokio::test]
async fn writer_coalesces_requests_and_starts_timer_after_idle() {
    let (writer, calls, server) = mock_writer(vec![StatusCode::OK]).await;
    let (sender, receiver) = mpsc::channel(8);
    let worker = tokio::spawn(writer.run(receiver));
    time::sleep(INSERT_BATCH_INTERVAL + Duration::from_millis(100)).await;
    let (first, mut first_done) = pending_rows(1);
    let (second, second_done) = pending_rows(1);
    sender.send(first).await.unwrap();
    sender.send(second).await.unwrap();
    time::sleep(Duration::from_millis(150)).await;
    assert!(
        calls.lock().unwrap().is_empty(),
        "idle timer must not immediately flush new rows"
    );
    assert!(matches!(
        first_done.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    time::timeout(Duration::from_secs(3), first_done)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    second_done.await.unwrap().unwrap();
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(calls.lock().unwrap()[0].1.lines().skip(1).count(), 2);
    drop(sender);
    worker.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn writer_flushes_at_row_threshold_without_waiting_for_timer() {
    let (writer, calls, server) = mock_writer(vec![StatusCode::OK]).await;
    let (sender, receiver) = mpsc::channel(8);
    // A long deadline makes the row trigger distinguishable without tight timing assertions.
    let worker = tokio::spawn(writer.run_with_interval(receiver, Duration::from_secs(60)));
    let (first, first_done) = pending_rows(2_500);
    let (second, second_done) = pending_rows(2_500);
    sender.send(first).await.unwrap();
    sender.send(second).await.unwrap();
    time::timeout(Duration::from_secs(10), first_done)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    second_done.await.unwrap().unwrap();
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(calls.lock().unwrap()[0].1.lines().skip(1).count(), 5_000);
    drop(sender);
    worker.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn shutdown_drains_and_reports_the_database_outcome() {
    for status in [StatusCode::OK, StatusCode::BAD_REQUEST] {
        let (writer, calls, server) = mock_writer(vec![status]).await;
        let (sender, receiver) = mpsc::channel(8);
        let worker = tokio::spawn(writer.run_with_interval(receiver, Duration::from_secs(60)));
        let (batch, completed) = pending_rows(2);
        let (shutdown, stopped) = oneshot::channel();
        sender.send(batch).await.unwrap();
        sender
            .send(InsertCommand::Shutdown(shutdown))
            .await
            .unwrap();
        time::timeout(Duration::from_secs(3), stopped)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(completed.await.unwrap().is_ok(), status.is_success());
        assert_eq!(calls.lock().unwrap().len(), 1);
        worker.await.unwrap();
        server.abort();
    }
}

#[tokio::test]
async fn full_queue_rejects_with_backpressure_without_waiting() {
    let (sender, _receiver) = mpsc::channel(1);
    let (batch, _completed) = pending_rows(1);
    sender.try_send(batch).ok().unwrap();
    let clickhouse = ClickHouse {
        auth: None,
        client: Client::new(),
        ingestion_fences: Arc::new(Mutex::new(HashMap::new())),
        insert_queue: sender,
        url: "http://127.0.0.1:1".to_owned(),
    };
    let result = clickhouse
        .enqueue_events(vec![super::tests::event_row("overflow")], None)
        .await;
    assert!(
        matches!(result, Err(ApiError::ServiceUnavailable(message)) if message.contains("backlog is full"))
    );
}
