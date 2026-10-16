use super::*;
use axum::{extract::Query, routing::post, Router};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn report_decoder_refuses_partial_or_oversized_output() {
    let rows = decode_report_rows::<Value>(b"{\"count\":0}\n{\"count\":12}\n").unwrap();
    assert_eq!(rows, vec![json!({"count":0}), json!({"count":12})]);
    assert!(decode_report_rows::<Value>(b"{\"count\":12}\nException: scan limit\n").is_err());
    assert!(decode_report_rows::<Value>("{}\n".repeat(REPORT_MAX_ROWS + 1).as_bytes()).is_err());
    assert!(decode_report_rows::<Value>(&vec![b' '; REPORT_MAX_BYTES + 1]).is_err());
}

#[tokio::test]
async fn report_budget_parameters_and_bound_values_reach_server() {
    let captured = Arc::new(Mutex::new(None));
    let capture = captured.clone();
    let app = Router::new().route(
        "/",
        post(
            move |Query(params): Query<HashMap<String, String>>, body: String| {
                *capture.lock().unwrap() = Some((params, body));
                async { "{\"value\":12}\n" }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = ClickHouse::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let rows: Vec<Value> = ch
        .query_report_with_params(
            "SELECT 12 AS value WHERE {site:String}='authorized'",
            &[("param_site", "authorized")],
        )
        .await
        .unwrap();
    assert_eq!(rows[0]["value"], 12);
    let (params, body) = captured.lock().unwrap().clone().unwrap();
    assert_eq!(params["readonly"], "1");
    assert_eq!(params["max_threads"], "1");
    assert_eq!(params["cancel_http_readonly_queries_on_client_close"], "1");
    assert_eq!(params["max_rows_to_read"], "5000000");
    assert_eq!(params["max_memory_usage"], "100663296");
    assert_eq!(params["result_overflow_mode"], "throw");
    assert_eq!(params["max_result_bytes"], "524288");
    assert_eq!(params["param_site"], "authorized");
    assert!(body.ends_with("FORMAT JSONEachRow"));
    assert!(ch
        .query_report_with_params::<Value>("SELECT 1", &[("readonly", "0")])
        .await
        .is_err());
    assert!(ch.query_report::<Value>("DROP TABLE events").await.is_err());
    server.abort();
}

#[tokio::test]
async fn report_capacity_is_shared_and_queue_has_a_hard_bound() {
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let counts = active.clone();
    let peaks = peak.clone();
    let app = Router::new().route(
        "/",
        post(move || {
            let counts = counts.clone();
            let peaks = peaks.clone();
            async move {
                let n = counts.fetch_add(1, Ordering::SeqCst) + 1;
                peaks.fetch_max(n, Ordering::SeqCst);
                time::sleep(Duration::from_millis(20)).await;
                counts.fetch_sub(1, Ordering::SeqCst);
                "{\"value\":1}\n"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = ClickHouse::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut requests = vec![];
    for index in 0..6 {
        let cloned = ch.clone();
        requests.push(tokio::spawn(async move {
            cloned
                .with_report_scope(&format!("site-{index}"), 1, async {
                    cloned.query_report::<Value>("SELECT 1").await
                })
                .await
        }));
    }
    for request in requests {
        request.await.unwrap().unwrap();
    }
    assert_eq!(peak.load(Ordering::SeqCst), REPORT_CONCURRENCY);
    let _all = ch
        .report_admission
        .acquire_many(REPORT_ADMISSION as u32)
        .await
        .unwrap();
    let rejected = ch.query_report::<Value>("SELECT 1").await.unwrap_err();
    assert!(rejected.to_string().contains("queue is full"));
    server.abort();
}

#[tokio::test]
async fn report_http_reader_rejects_an_oversized_response() {
    let app = Router::new().route("/", post(|| async { "x".repeat(REPORT_MAX_BYTES + 1) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = ClickHouse::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    assert!(ch
        .query_report::<Value>("SELECT 1")
        .await
        .unwrap_err()
        .to_string()
        .contains("byte budget"));
    server.abort();
}

#[tokio::test]
#[ignore = "requires OWLEYE_TEST_CLICKHOUSE_URL; bounded synthetic SELECT only"]
async fn real_clickhouse_report_limits_throw_instead_of_truncating() {
    let url = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("set the isolated ClickHouse URL");
    let ch = ClickHouse::new(url).unwrap();
    assert!(ch
        .query_report::<Value>("SELECT number FROM numbers(2001)")
        .await
        .is_err());
    assert!(ch
        .query_report::<Value>("SELECT sum(number) FROM numbers(5000001)")
        .await
        .is_err());
}

#[tokio::test]
async fn report_request_assigns_every_overview_statement_and_cannot_reset_after_failure() {
    let captured = Arc::new(Mutex::new(Vec::<HashMap<String, String>>::new()));
    let capture = captured.clone();
    let app = Router::new().route(
        "/",
        post(move |Query(params): Query<HashMap<String, String>>| {
            capture.lock().unwrap().push(params);
            async { "{\"value\":1}\n" }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = ClickHouse::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    ch.with_report_scope("overview-site", 14, async {
        for _ in 0..14 {
            ch.query_report::<Value>("SELECT 1").await?;
        }
        // A nested reader cannot get a fresh budget after exhausting fourteen.
        let extra = ch
            .with_report_scope("overview-site", 64, async {
                ch.query_report::<Value>("SELECT 1").await
            })
            .await
            .unwrap_err();
        assert!(extra.to_string().contains("shared statement budget"));
        Ok::<_, anyhow::Error>(())
    })
    .await
    .unwrap();
    let params = captured.lock().unwrap();
    assert_eq!(params.len(), 14);
    assert!(params
        .iter()
        .all(|p| p["max_rows_to_read"] == (20_000_000_u64 / 14).to_string()));
    assert!(params
        .iter()
        .all(|p| p["max_bytes_to_read"] == (1_073_741_824_u64 / 14).to_string()));
    server.abort();
}

#[tokio::test]
async fn report_streamed_output_budget_is_combined_and_isolated_between_requests() {
    let requests = Arc::new(AtomicUsize::new(0));
    let request_count = requests.clone();
    let app = Router::new().route(
        "/",
        post(move || {
            request_count.fetch_add(1, Ordering::SeqCst);
            async { format!("{{\"value\":\"{}\"}}\n", "x".repeat(450_000)) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = ClickHouse::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    ch.with_report_scope("large-result-site", 6, async {
        for _ in 0..4 {
            ch.query_report::<Value>("SELECT 1").await?;
        }
        let error = ch.query_report::<Value>("SELECT 1").await.unwrap_err();
        assert!(report_resource_message(&error)
            .unwrap()
            .contains("shared result budget"));
        assert!(ch.query_report::<Value>("SELECT 1").await.is_err());
        assert_eq!(requests.load(Ordering::SeqCst), 5);
        Ok::<_, anyhow::Error>(())
    })
    .await
    .unwrap();
    ch.with_report_scope("other-site", 1, ch.query_report::<Value>("SELECT 1"))
        .await
        .unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 6);
    server.abort();
}

#[tokio::test]
async fn report_busy_site_cannot_fill_global_queue_and_other_site_gets_next_slot() {
    let release = Arc::new(Semaphore::new(0));
    let handler_release = release.clone();
    let (started, mut starts) = mpsc::unbounded_channel::<String>();
    let app = Router::new().route(
        "/",
        post(move |Query(params): Query<HashMap<String, String>>| {
            let release = handler_release.clone();
            let started = started.clone();
            async move {
                started.send(params["param_site"].clone()).unwrap();
                release.acquire().await.unwrap().forget();
                "{\"value\":1}\n"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = ClickHouse::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut busy = Vec::new();
    for _ in 0..2 {
        let client = ch.clone();
        busy.push(tokio::spawn(async move {
            client
                .with_report_scope(
                    "busy",
                    1,
                    client.query_report_with_params::<Value>("SELECT 1", &[("param_site", "busy")]),
                )
                .await
        }));
    }
    for _ in 0..2 {
        assert_eq!(starts.recv().await.unwrap(), "busy");
    }
    let rejected = ch
        .with_report_scope("busy", 1, ch.query_report::<Value>("SELECT 1"))
        .await
        .unwrap_err();
    assert!(report_resource_message(&rejected)
        .unwrap()
        .contains("site's"));
    assert_eq!(
        ch.report_admission.available_permits(),
        REPORT_ADMISSION - 2
    );
    let other_client = ch.clone();
    let other = tokio::spawn(async move {
        other_client
            .with_report_scope(
                "other",
                1,
                other_client
                    .query_report_with_params::<Value>("SELECT 1", &[("param_site", "other")]),
            )
            .await
    });
    time::timeout(Duration::from_secs(1), async {
        while ch.report_admission.available_permits() != REPORT_ADMISSION - 3 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    release.add_permits(1);
    assert_eq!(
        time::timeout(Duration::from_secs(1), starts.recv())
            .await
            .unwrap()
            .unwrap(),
        "other"
    );
    release.add_permits(2);
    for request in busy {
        request.await.unwrap().unwrap();
    }
    other.await.unwrap().unwrap();
    assert_eq!(ch.report_admission.available_permits(), REPORT_ADMISSION);
    server.abort();
}

#[tokio::test]
async fn report_cancelled_wait_drops_admission_but_never_refunds_request_work() {
    let ch = ClickHouse::new("http://127.0.0.1:1".into()).unwrap();
    let all = ch
        .report_capacity
        .acquire_many(REPORT_CONCURRENCY as u32)
        .await
        .unwrap();
    ch.with_report_scope("cancelled-site", 1, async {
        assert!(time::timeout(
            Duration::from_millis(5),
            ch.query_report::<Value>("SELECT 1")
        )
        .await
        .is_err());
        assert_eq!(ch.report_admission.available_permits(), REPORT_ADMISSION);
        let next = ch.query_report::<Value>("SELECT 1").await.unwrap_err();
        assert!(next.to_string().contains("shared statement budget"));
        Ok::<_, anyhow::Error>(())
    })
    .await
    .unwrap();
    drop(all);
    assert_eq!(ch.report_capacity.available_permits(), REPORT_CONCURRENCY);
}

#[tokio::test]
async fn report_third_child_waits_for_valid_slow_reads_without_false_site_rejection() {
    let count = Arc::new(AtomicUsize::new(0));
    let starts = count.clone();
    let app = Router::new().route(
        "/",
        post(move || {
            let index = starts.fetch_add(1, Ordering::SeqCst);
            async move {
                if index < 2 {
                    time::sleep(Duration::from_millis(3_050)).await;
                }
                "{\"value\":1}\n"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ch = ClickHouse::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    ch.with_report_scope("performance-site", 3, async {
        tokio::try_join!(
            ch.query_report::<Value>("SELECT 1"),
            ch.query_report::<Value>("SELECT 1"),
            ch.query_report::<Value>("SELECT 1")
        )?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 3);
    server.abort();
}

#[tokio::test]
async fn report_failed_local_output_cannot_be_caught_to_reuse_shared_budget() {
    for ai in [false, true] {
        let byte_limit = if ai { 65_536 } else { REPORT_MAX_BYTES };
        let row_limit = if ai { 400 } else { REPORT_MAX_ROWS };
        let cases = [
            vec![b'x'; byte_limit + 1],
            "{}\n".repeat(row_limit + 1).into_bytes(),
            b"{}\nnot-json\n".to_vec(),
            vec![0xff],
        ];
        for body in cases {
            let requests = Arc::new(AtomicUsize::new(0));
            let count = requests.clone();
            let app = Router::new().route(
                "/",
                post(move || {
                    count.fetch_add(1, Ordering::SeqCst);
                    let body = body.clone();
                    async move {
                        // Chunked bodies exercise the streaming guard rather than
                        // relying solely on an advertised Content-Length.
                        axum::body::Body::from_stream(
                            axum::body::Body::from(body).into_data_stream(),
                        )
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let ch = ClickHouse::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
            let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            ch.with_report_scope("failed-output", 2, async {
                let failed = if ai {
                    ch.query_ai_report::<Value>("SELECT 1", &[], None).await
                } else {
                    ch.query_report::<Value>("SELECT 1").await
                };
                assert!(failed.is_err());
                let failed = failed.unwrap_err();
                let later = ch.query_report::<Value>("SELECT 1").await.unwrap_err();
                assert!(
                    report_resource_message(&later)
                        .unwrap()
                        .contains("shared result budget"),
                    "ai={ai}; first={failed}; later={later}"
                );
                assert_eq!(requests.load(Ordering::SeqCst), 1);
                Ok::<_, anyhow::Error>(())
            })
            .await
            .unwrap();
            // Failure does not poison another authorized request/site.
            ch.with_report_scope("unrelated", 1, async { Ok::<_, anyhow::Error>(()) })
                .await
                .unwrap();
            server.abort();
        }
    }
}
