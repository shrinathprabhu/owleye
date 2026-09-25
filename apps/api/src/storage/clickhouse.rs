use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use reqwest::{Client, RequestBuilder, Url};
use serde::de::DeserializeOwned;
use tokio::{
    sync::{mpsc, oneshot, OwnedRwLockReadGuard, RwLock},
    time::{self, MissedTickBehavior},
};

use crate::{models::clickhouse_string, ApiError, EventRow};

const EVENTS_TABLE: &str = "owleye_events";
const INSERT_BATCH_MAX_ROWS: usize = 1_000;
const INSERT_BATCH_INTERVAL: Duration = Duration::from_millis(100);
const INSERT_QUEUE_CAPACITY: usize = 512;
const CLICKHOUSE_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
// Bound request waiting; the queued batch retains its erasure fence until settled.
const INSERT_COMPLETION_TIMEOUT: Duration = Duration::from_secs(45);
#[cfg(test)]
const FRESH_FACT_TABLES: &[&str] = &[EVENTS_TABLE, "owleye_uptime_checks"];

const EVENT_COLUMNS: &[&str] = &[
    "event_id",
    "site_id",
    "event_type",
    "event_name",
    "visitor_id",
    "occurred_at",
    "received_at",
    "url",
    "url_host",
    "url_path",
    "url_search",
    "url_hash",
    "page_title",
    "referrer",
    "referrer_host",
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "timezone",
    "locale",
    "color_scheme",
    "viewport_width",
    "viewport_height",
    "screen_width",
    "screen_height",
    "device_pixel_ratio",
    "browser_name",
    "browser_version",
    "os_name",
    "os_version",
    "device_type",
    "continent",
    "country",
    "region",
    "retention_active_until",
    "retention_days",
    "retention_policy_version",
    "retention_purge_after",
    "retention_tier",
    "tier_at_ingestion",
    "city",
    "latitude",
    "longitude",
    "anon_user_id",
    "anon_session_id",
    "duration_ms",
    "rule_id",
    "rule_type",
    "payload_json",
    "sdk_name",
    "sdk_version",
];

fn recover_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Clone)]
pub struct ClickHouse {
    auth: Option<ClickHouseAuth>,
    client: Client,
    ingestion_fences: Arc<Mutex<HashMap<String, Arc<RwLock<()>>>>>,
    insert_queue: mpsc::Sender<InsertCommand>,
    url: String,
}

#[derive(Clone)]
struct ClickHouseAuth {
    password: Option<String>,
    username: String,
}

impl ClickHouse {
    pub fn new(url: String) -> anyhow::Result<Self> {
        let (url, auth) = sanitized_clickhouse_url(&url)?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(CLICKHOUSE_REQUEST_TIMEOUT)
            .build()?;
        let (insert_queue, receiver) = mpsc::channel(INSERT_QUEUE_CAPACITY);
        let writer = ClickHouseWriter {
            auth: auth.clone(),
            client: client.clone(),
            url: url.clone(),
        };

        tokio::spawn(async move {
            writer.run(receiver).await;
        });

        Ok(Self {
            auth,
            client,
            ingestion_fences: Arc::new(Mutex::new(HashMap::new())),
            insert_queue,
            url,
        })
    }

    pub async fn init(&self) -> anyhow::Result<()> {
        // Cleanup snapshots contain only daily counters and an opaque retry key.
        // No site, account, visitor, URL, or payload columns are retained.
        for ddl in [
            "CREATE TABLE IF NOT EXISTS owleye_deleted_event_daily_totals (batch_id String, event_day Date, events UInt64, pageviews UInt64, errors UInt64, performance UInt64) ENGINE = ReplacingMergeTree ORDER BY (batch_id, event_day)",
            "CREATE TABLE IF NOT EXISTS owleye_event_deletion_snapshots (batch_id String, completed_at DateTime64(3, 'UTC') DEFAULT now64(3)) ENGINE = ReplacingMergeTree ORDER BY batch_id",
            "CREATE TABLE IF NOT EXISTS owleye_event_deletion_completions (batch_id String) ENGINE = ReplacingMergeTree ORDER BY batch_id",
            "CREATE OR REPLACE VIEW owleye_deleted_event_totals AS SELECT event_day, sum(events) AS events, sum(pageviews) AS pageviews, sum(errors) AS errors, sum(performance) AS performance FROM owleye_deleted_event_daily_totals FINAL WHERE batch_id IN (SELECT batch_id FROM owleye_event_deletion_completions) GROUP BY event_day",
        ] {
            self.execute(ddl).await?;
        }
        let history_days = crate::uptime::HISTORY_DAYS;
        self.execute(&format!("CREATE TABLE IF NOT EXISTS owleye_uptime_checks (site_id String, monitor_id String, checked_at UInt32, status_code Nullable(UInt16), duration_ms UInt64, state LowCardinality(String), failure LowCardinality(String)) ENGINE = MergeTree ORDER BY (site_id, monitor_id, checked_at) TTL toDateTime(checked_at) + INTERVAL {history_days} DAY DELETE")).await?;
        // CREATE IF NOT EXISTS does not change older installations' 30-day TTL.
        // Only migrate when necessary to avoid re-materializing TTL at each startup.
        let ttl = format!("toDateTime(checked_at) + toIntervalDay({history_days})");
        let definition = self.query_json_each_row::<serde_json::Value>("SELECT create_table_query FROM system.tables WHERE database = currentDatabase() AND name = 'owleye_uptime_checks'").await?;
        if !definition
            .first()
            .and_then(|row| row["create_table_query"].as_str())
            .is_some_and(|query| query.contains(&ttl))
        {
            self.execute(&format!(
                "ALTER TABLE owleye_uptime_checks MODIFY TTL {ttl} DELETE"
            ))
            .await?;
        }

        self.execute(
            r#"
            CREATE TABLE IF NOT EXISTS owleye_events (
                event_id UUID,
                site_id String,
                event_type LowCardinality(String),
                event_name LowCardinality(String),
                occurred_at DateTime64(3, 'UTC'),
                received_at DateTime64(3, 'UTC') DEFAULT now64(3),
                url String,
                url_host String,
                url_path String,
                url_search String,
                url_hash String,
                page_title String,
                referrer String,
                referrer_host String,
                timezone LowCardinality(String),
                locale LowCardinality(String),
                color_scheme LowCardinality(String),
                viewport_width Nullable(UInt32),
                viewport_height Nullable(UInt32),
                screen_width Nullable(UInt32),
                screen_height Nullable(UInt32),
                device_pixel_ratio Nullable(Float64),
                browser_name LowCardinality(String),
                browser_version String,
                os_name LowCardinality(String),
                os_version String,
                device_type LowCardinality(String),
                continent LowCardinality(String),
                country LowCardinality(String),
                region String,
                retention_active_until Nullable(DateTime64(3, 'UTC')),
                retention_days UInt16 DEFAULT 90,
                retention_edition LowCardinality(String) DEFAULT 'self_hosted',
                retention_policy_version UInt16 DEFAULT 0,
                retention_purge_after Nullable(DateTime64(3, 'UTC')),
                retention_tier LowCardinality(String) DEFAULT 'self_hosted',
                tier_at_ingestion LowCardinality(String) DEFAULT 'self_hosted',
                city String,
                latitude Nullable(Float64),
                longitude Nullable(Float64),
                anon_user_id String,
                anon_session_id String,
                duration_ms Nullable(UInt64),
                rule_id String,
                rule_type LowCardinality(String),
                payload_json String,
                sdk_name LowCardinality(String),
                sdk_version String
            )
            ENGINE = MergeTree
            PARTITION BY toYYYYMM(occurred_at)
            ORDER BY (site_id, toDate(occurred_at), event_type, event_name, occurred_at, event_id)
            "#,
        )
        .await?;

        for statement in [
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS timestamp DateTime64(3, 'UTC') DEFAULT occurred_at AFTER site_id",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS visitor_id String DEFAULT anon_user_id AFTER event_name",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS session_id String DEFAULT anon_session_id AFTER visitor_id",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS pathname String DEFAULT url_path AFTER url",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS hostname LowCardinality(String) DEFAULT url_host AFTER pathname",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS referrer_domain LowCardinality(String) DEFAULT referrer_host AFTER referrer",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS utm_source LowCardinality(String) DEFAULT '' AFTER referrer_domain",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS utm_medium LowCardinality(String) DEFAULT '' AFTER utm_source",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS utm_campaign LowCardinality(String) DEFAULT '' AFTER utm_medium",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS utm_term String DEFAULT '' AFTER utm_campaign",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS utm_content String DEFAULT '' AFTER utm_term",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS browser LowCardinality(String) DEFAULT browser_name AFTER city",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS os LowCardinality(String) DEFAULT os_name AFTER browser_version",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS device LowCardinality(String) DEFAULT device_type AFTER os_version",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS screen_class LowCardinality(String) DEFAULT '' AFTER device",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS language LowCardinality(String) DEFAULT locale AFTER screen_class",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS custom_props Map(String, String) DEFAULT map() AFTER language",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS duration UInt32 DEFAULT toUInt32(ifNull(duration_ms, 0)) AFTER custom_props",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS tier_at_ingestion LowCardinality(String) DEFAULT 'self_hosted' AFTER duration",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS retention_days UInt16 DEFAULT 90 AFTER tier_at_ingestion",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS retention_edition LowCardinality(String) DEFAULT 'self_hosted' AFTER retention_days",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS retention_tier LowCardinality(String) DEFAULT tier_at_ingestion AFTER retention_edition",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS retention_policy_version UInt16 DEFAULT 0 AFTER retention_tier",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS retention_active_until Nullable(DateTime64(3, 'UTC')) AFTER retention_policy_version",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS retention_purge_after Nullable(DateTime64(3, 'UTC')) AFTER retention_active_until",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS received_at DateTime64(3, 'UTC') DEFAULT now64(3) AFTER occurred_at",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS url String AFTER received_at",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS url_hash String AFTER url_search",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS page_title String AFTER url_hash",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS referrer String AFTER page_title",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS color_scheme LowCardinality(String) AFTER locale",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS viewport_width Nullable(UInt32) AFTER color_scheme",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS viewport_height Nullable(UInt32) AFTER viewport_width",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS screen_width Nullable(UInt32) AFTER viewport_height",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS screen_height Nullable(UInt32) AFTER screen_width",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS device_pixel_ratio Nullable(Float64) AFTER screen_height",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS duration_ms Nullable(UInt64) AFTER anon_session_id",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS rule_id String AFTER duration_ms",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS rule_type LowCardinality(String) AFTER rule_id",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS sdk_name LowCardinality(String) DEFAULT 'owleye-js' AFTER payload_json",
            "ALTER TABLE owleye_events ADD COLUMN IF NOT EXISTS sdk_version String AFTER sdk_name",
        ] {
            self.execute(statement).await?;
        }
        // Older development schemas used a table TTL driven by
        // `retention_days`. Remove it to retain events until explicit deletion.
        let legacy_ttl = self
            .query_json_each_row::<LegacyTtlRow>(
                "SELECT toUInt8(positionCaseInsensitive(create_table_query, 'TTL occurred_at') > 0) AS has_legacy_ttl FROM system.tables WHERE database = currentDatabase() AND name = 'owleye_events'",
            )
            .await?;
        if legacy_ttl
            .first()
            .is_some_and(|row| row.has_legacy_ttl == 1)
        {
            self.execute("ALTER TABLE owleye_events REMOVE TTL").await?;
        }

        Ok(())
    }

    /// Validate the site's durable erasure fence while holding the shared
    /// ingestion side of the gate. A site deletion takes the exclusive side,
    /// so every locally in-flight batch either finishes before the deletion
    /// mutation or is rejected before it enters the writer queue.
    pub async fn insert_events_after<F, Fut>(
        &self,
        site_id: &str,
        rows: Vec<EventRow>,
        validate: F,
    ) -> Result<(), ApiError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), ApiError>>,
    {
        let fence = self.ingestion_fence(site_id);
        let fence = fence.read_owned().await;
        validate().await?;
        self.enqueue_events(rows, Some(fence)).await
    }

    async fn enqueue_events(
        &self,
        rows: Vec<EventRow>,
        fence: Option<OwnedRwLockReadGuard<()>>,
    ) -> Result<(), ApiError> {
        if rows.is_empty() {
            return Ok(());
        }

        let (completion, completed) = oneshot::channel();
        self.insert_queue
            .try_send(InsertCommand::Batch(PendingInsert {
                completion,
                rows,
                fence,
            }))
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => ApiError::ServiceUnavailable(
                    "Ingestion backlog is full. Retry later.".to_owned(),
                ),
                mpsc::error::TrySendError::Closed(_) => {
                    ApiError::Internal(anyhow::anyhow!("ClickHouse insert queue is closed"))
                }
            })?;

        await_insert_completion(completed, INSERT_COMPLETION_TIMEOUT).await
    }

    /// Synchronously erase a site's analytics. `mutations_sync = 2` waits for
    /// every replica, making a successful response usable as deletion evidence.
    pub async fn delete_site_events(&self, site_id: &str, batch_id: &str) -> Result<(), ApiError> {
        let fence = self.ingestion_fence(site_id);
        let _fence = fence.write().await;
        self.snapshot_deleted_event_totals(
            batch_id,
            &format!("site_id = {}", clickhouse_string(site_id)),
        )
        .await?;
        for query in [
            format!("ALTER TABLE owleye_uptime_checks DELETE WHERE site_id = {} SETTINGS mutations_sync = 2", clickhouse_string(site_id)),
            site_deletion_query(site_id),
        ] {
            self.execute(&query).await.map_err(|_| {
                tracing::error!(site_id, "ClickHouse rejected a site deletion mutation");
                ApiError::ServiceUnavailable(
                    "Analytics deletion is queued for retry. Collection remains paused until cleanup completes.".to_owned(),
                )
            })?;
        }
        self.complete_event_deletion(batch_id).await
    }

    async fn complete_event_deletion(&self, batch_id: &str) -> Result<(), ApiError> {
        self.execute(&format!(
            "INSERT INTO owleye_event_deletion_completions (batch_id) VALUES ({})",
            clickhouse_string(batch_id)
        ))
        .await
        .map_err(|_| {
            ApiError::ServiceUnavailable(
                "Deletion completion could not be recorded; cleanup will retry".into(),
            )
        })
    }

    async fn snapshot_deleted_event_totals(
        &self,
        batch_id: &str,
        predicate: &str,
    ) -> Result<(), ApiError> {
        let batch = clickhouse_string(batch_id);
        // A completed snapshot survives ambiguous/partially completed DELETE
        // retries. Before completion, replacement rows make INSERT retries safe.
        // Callers keep the site's durable erasure fence closed until completion.
        let completed = self
            .query_json_each_row::<serde_json::Value>(&format!(
            "SELECT batch_id FROM owleye_event_deletion_snapshots WHERE batch_id = {batch} LIMIT 1"
        ))
            .await
            .map_err(|_| {
                ApiError::ServiceUnavailable(
                    "Deletion totals could not be checked; cleanup will retry".into(),
                )
            })?;
        if !completed.is_empty() {
            return Ok(());
        }
        self.execute(&format!(
            "INSERT INTO owleye_deleted_event_daily_totals SELECT {batch} AS batch_id, toDate(occurred_at, 'UTC') AS event_day, count() AS events, countIf(event_type = 'pageview') AS pageviews, countIf(event_type = 'error' OR (event_type = 'external' AND event_name IN ('error', 'exception', 'unhandledrejection'))) AS errors, countIf(event_type = 'performance') AS performance FROM owleye_events WHERE {predicate} GROUP BY event_day"
        )).await.map_err(|_| ApiError::ServiceUnavailable("Deletion totals could not be saved; cleanup will retry".into()))?;
        self.execute(&format!(
            "INSERT INTO owleye_event_deletion_snapshots (batch_id) VALUES ({batch})"
        ))
        .await
        .map_err(|_| {
            ApiError::ServiceUnavailable(
                "Deletion totals could not be confirmed; cleanup will retry".into(),
            )
        })?;
        Ok(())
    }

    pub(crate) fn ingestion_fence(&self, site_id: &str) -> Arc<RwLock<()>> {
        let mut fences = recover_mutex(&self.ingestion_fences);
        fences
            .entry(site_id.to_owned())
            .or_insert_with(|| Arc::new(RwLock::new(())))
            .clone()
    }

    pub async fn shutdown(&self) {
        let (completion, completed) = oneshot::channel();
        if self
            .insert_queue
            .send(InsertCommand::Shutdown(completion))
            .await
            .is_err()
        {
            return;
        }

        match time::timeout(CLICKHOUSE_REQUEST_TIMEOUT, completed).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => tracing::warn!("ClickHouse writer stopped during shutdown"),
            Err(_) => tracing::warn!("timed out while draining the ClickHouse insert queue"),
        }
    }

    /// Reserved for the fixed AI report compiler, with bound tenant/date values.
    pub(crate) async fn query_ai_report<T: DeserializeOwned>(
        &self,
        sql: &str,
        parameters: &[(&str, &str)],
        read_url: Option<&str>,
    ) -> anyhow::Result<Vec<T>> {
        let override_connection = read_url.map(sanitized_clickhouse_url).transpose()?;
        let request = if let Some((url, auth)) = &override_connection {
            authenticated_post(&self.client, url, auth.as_ref())
        } else {
            self.request()
        };
        let mut response = request
            .query(parameters)
            .query(&[
                ("readonly", "1"),
                ("max_execution_time", "10"),
                ("max_rows_to_read", "20000000"),
                ("max_bytes_to_read", "1000000000"),
                ("max_memory_usage", "268435456"),
                ("max_result_rows", "400"),
                ("max_result_bytes", "65536"),
                ("max_threads", "2"),
                ("read_overflow_mode", "throw"),
                ("result_overflow_mode", "throw"),
                ("timeout_overflow_mode", "throw"),
                ("output_format_json_quote_64bit_integers", "0"),
                ("wait_end_of_query", "1"),
            ])
            .timeout(Duration::from_secs(15))
            .body(format!("{sql}\nFORMAT JSONEachRow"))
            .send()
            .await?;
        #[cfg(test)]
        if !response.status().is_success() {
            anyhow::bail!("AI test query failed: {}", response.text().await?);
        }
        response.error_for_status_ref()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(bytes.len() + chunk.len() <= 65536, "AI result too large");
            bytes.extend_from_slice(&chunk);
        }
        let text = std::str::from_utf8(&bytes)?;
        let mut rows = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            anyhow::ensure!(rows.len() < 400, "AI result has too many rows");
            rows.push(serde_json::from_str(line)?);
        }
        Ok(rows)
    }

    /// Public dashboards use the same bounded, read-only aggregate query limits.
    pub(crate) async fn query_public_overview<T: DeserializeOwned>(
        &self,
        sql: &str,
    ) -> anyhow::Result<Vec<T>> {
        self.query_ai_report(sql, &[], None).await
    }

    pub async fn query_json_each_row<T>(&self, sql: &str) -> anyhow::Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let body = format!("{sql}\nFORMAT JSONEachRow");
        let text = self
            .request()
            .body(body)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let mut rows = Vec::new();

        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            rows.push(serde_json::from_str(line)?);
        }

        Ok(rows)
    }

    pub(crate) async fn insert_uptime_check(&self, row: &serde_json::Value) -> anyhow::Result<()> {
        self.execute(&format!(
            "INSERT INTO owleye_uptime_checks FORMAT JSONEachRow\n{}",
            serde_json::to_string(row)?
        ))
        .await
    }

    async fn execute(&self, sql: &str) -> anyhow::Result<()> {
        let text = self
            .request()
            .query(&[("wait_end_of_query", "1")])
            .body(sql.to_owned())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        if !text.trim().is_empty() {
            anyhow::bail!(
                "ClickHouse command returned an unexpected response body: {}",
                text.chars().take(256).collect::<String>()
            );
        }
        Ok(())
    }

    fn request(&self) -> RequestBuilder {
        authenticated_post(&self.client, &self.url, self.auth.as_ref())
    }
}

fn sanitized_clickhouse_url(value: &str) -> anyhow::Result<(String, Option<ClickHouseAuth>)> {
    let mut url = Url::parse(value)?;
    let mut username = (!url.username().is_empty()).then(|| url.username().to_owned());
    let mut password = url.password().map(str::to_owned);
    url.set_username("")
        .map_err(|_| anyhow::anyhow!("invalid ClickHouse URL username"))?;
    url.set_password(None)
        .map_err(|_| anyhow::anyhow!("invalid ClickHouse URL password"))?;

    let query_pairs = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    for (key, value) in query_pairs {
        match key.as_str() {
            "user" | "username" => username = Some(value),
            "password" => password = Some(value),
            _ => {
                url.query_pairs_mut().append_pair(&key, &value);
            }
        }
    }
    let auth = username
        .or_else(|| password.as_ref().map(|_| "default".to_owned()))
        .map(|username| ClickHouseAuth { password, username });
    Ok((url.to_string(), auth))
}

fn authenticated_post(client: &Client, url: &str, auth: Option<&ClickHouseAuth>) -> RequestBuilder {
    let request = client.post(url);
    match auth {
        Some(auth) => request.basic_auth(&auth.username, auth.password.as_deref()),
        None => request,
    }
}

#[derive(Debug, serde::Deserialize)]
struct LegacyTtlRow {
    has_legacy_ttl: u8,
}

async fn await_insert_completion(
    completed: oneshot::Receiver<Result<(), String>>,
    timeout: Duration,
) -> Result<(), ApiError> {
    match time::timeout(timeout, completed).await {
        Ok(Ok(result)) => result.map_err(ApiError::ServiceUnavailable),
        Ok(Err(_)) => Err(ApiError::Internal(anyhow::anyhow!(
            "ClickHouse writer stopped before acknowledging an insert"
        ))),
        Err(_) => Err(ApiError::ServiceUnavailable(
            "Timed out waiting for ClickHouse to persist the event batch. Retry later.".to_owned(),
        )),
    }
}

struct ClickHouseWriter {
    auth: Option<ClickHouseAuth>,
    client: Client,
    url: String,
}

enum InsertCommand {
    Batch(PendingInsert),
    Shutdown(oneshot::Sender<()>),
}

struct PendingInsert {
    completion: oneshot::Sender<Result<(), String>>,
    rows: Vec<EventRow>,
    fence: Option<OwnedRwLockReadGuard<()>>,
}

impl ClickHouseWriter {
    async fn run(self, mut receiver: mpsc::Receiver<InsertCommand>) {
        let mut pending = Vec::new();
        let mut pending_rows = 0usize;
        let mut interval = time::interval_at(
            time::Instant::now() + INSERT_BATCH_INTERVAL,
            INSERT_BATCH_INTERVAL,
        );
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                command = receiver.recv() => {
                    match command {
                        Some(InsertCommand::Batch(batch)) => {
                            pending_rows += batch.rows.len();
                            pending.push(batch);
                            if pending_rows >= INSERT_BATCH_MAX_ROWS {
                                self.flush_batch(&mut pending).await;
                                pending_rows = 0;
                            }
                        }
                        Some(InsertCommand::Shutdown(completion)) => {
                            self.flush_batch(&mut pending).await;
                            let _ = completion.send(());
                            return;
                        }
                        None => break,
                    }
                }
                _ = interval.tick(), if !pending.is_empty() => {
                    self.flush_batch(&mut pending).await;
                    pending_rows = 0;
                }
            }
        }

        if !pending.is_empty() {
            self.flush_batch(&mut pending).await;
        }
    }

    async fn flush_batch(&self, pending: &mut Vec<PendingInsert>) {
        let pending: Vec<_> = std::mem::take(pending)
            .into_iter()
            .filter(|batch| !batch.completion.is_closed())
            .collect();
        if pending.is_empty() {
            return;
        }

        let row_count = pending.iter().map(|batch| batch.rows.len()).sum();
        let mut rows = Vec::with_capacity(row_count);
        let mut completions = Vec::with_capacity(pending.len());
        let mut fences = Vec::with_capacity(pending.len());
        for batch in pending {
            fences.push(batch.fence);
            rows.extend(batch.rows);
            completions.push(batch.completion);
        }

        // A lost HTTP acknowledgement does not prove an insert failed. MergeTree
        // does not deduplicate UUIDs; retrying here would double-count events.
        let result = self.insert_events(&rows).await.map_err(|error| {
            tracing::error!(%error, rows = rows.len(), "ClickHouse insert outcome requires reconciliation");
            "ClickHouse did not acknowledge the event batch".to_owned()
        });

        for completion in completions {
            let acknowledgement = match &result {
                Ok(()) => Ok(()),
                Err(message) => Err(message.clone()),
            };
            let _ = completion.send(acknowledgement);
        }
    }

    async fn insert_events(&self, rows: &[EventRow]) -> anyhow::Result<()> {
        let body = build_insert_body(rows)?;

        let text = authenticated_post(&self.client, &self.url, self.auth.as_ref())
            .query(&[("wait_end_of_query", "1")])
            .body(body)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        if !text.trim().is_empty() {
            anyhow::bail!(
                "ClickHouse command returned an unexpected response body: {}",
                text.chars().take(256).collect::<String>()
            );
        }
        Ok(())
    }
}

fn build_insert_body(rows: &[EventRow]) -> anyhow::Result<String> {
    let mut body = format!(
        "INSERT INTO {EVENTS_TABLE} ({}) SETTINGS async_insert=0 FORMAT JSONEachRow\n",
        EVENT_COLUMNS.join(", ")
    );

    for row in rows {
        body.push_str(&serde_json::to_string(row)?);
        body.push('\n');
    }

    Ok(body)
}

fn site_deletion_query(site_id: &str) -> String {
    format!(
        "ALTER TABLE {EVENTS_TABLE} DELETE WHERE site_id = {} SETTINGS mutations_sync = 2",
        clickhouse_string(site_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ingestion_fences_are_shared_per_site_and_isolated_between_sites() {
        let clickhouse = ClickHouse::new("http://127.0.0.1:8123".to_owned()).unwrap();
        let first = clickhouse.ingestion_fence("site-a");
        let same = clickhouse.ingestion_fence("site-a");
        let other = clickhouse.ingestion_fence("site-b");

        assert!(Arc::ptr_eq(&first, &same));
        assert!(!Arc::ptr_eq(&first, &other));
    }

    #[ignore = "requires OWLEYE_TEST_CLICKHOUSE_URL; run with --ignored"]
    #[tokio::test]
    async fn initializes_schema_against_configured_clickhouse() {
        let Ok(url) = std::env::var("OWLEYE_TEST_CLICKHOUSE_URL") else {
            eprintln!(
                "skipping ClickHouse integration test; OWLEYE_TEST_CLICKHOUSE_URL is not set"
            );
            return;
        };

        let clickhouse = ClickHouse::new(url).unwrap();
        clickhouse.init().await.unwrap();
        let rows = clickhouse
            .query_json_each_row::<PingRow>("SELECT 1 AS value")
            .await
            .unwrap();
        let privacy_columns = clickhouse
            .query_json_each_row::<ColumnCountRow>(
                "SELECT toUInt64(countIf(name IN ('visitor_id', 'utm_source', 'utm_medium', 'utm_campaign', 'retention_days', 'retention_edition', 'tier_at_ingestion', 'retention_tier', 'retention_policy_version', 'retention_active_until', 'retention_purge_after'))) AS value FROM system.columns WHERE database = currentDatabase() AND table = 'owleye_events'",
            )
            .await
            .unwrap();
        let retention_ttl = clickhouse
            .query_json_each_row::<ColumnCountRow>(
                "SELECT toUInt64(positionCaseInsensitive(create_table_query, 'TTL occurred_at') > 0) AS value FROM system.tables WHERE database = currentDatabase() AND name = 'owleye_events'",
            )
            .await
            .unwrap();

        assert_eq!(rows.first().map(|row| row.value), Some(1));
        assert_eq!(privacy_columns.first().map(|row| row.value), Some(11));
        assert_eq!(retention_ttl.first().map(|row| row.value), Some(0));
    }

    #[test]
    fn build_insert_body_uses_async_insert_and_multiple_json_rows() {
        let rows = vec![event_row("one"), event_row("two")];
        let body = build_insert_body(&rows).unwrap();
        let first_row: serde_json::Value =
            serde_json::from_str(body.lines().nth(1).unwrap()).unwrap();
        let serialized_columns = first_row
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let insert_columns = EVENT_COLUMNS
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();

        assert!(body.contains("SETTINGS async_insert=0"));
        assert!(body.starts_with("INSERT INTO owleye_events (event_id, site_id"));
        assert!(body.contains("visitor_id"));
        assert!(body.contains("utm_source, utm_medium, utm_campaign"));
        assert_eq!(body.matches("\"event_id\"").count(), 2);
        assert!(body.lines().count() >= 3);
        assert_eq!(serialized_columns, insert_columns);
    }

    #[test]
    fn fresh_schema_only_declares_the_implemented_event_fact_surface() {
        assert_eq!(
            FRESH_FACT_TABLES,
            &["owleye_events", "owleye_uptime_checks"]
        );
    }

    #[test]
    fn site_deletion_query_waits_for_replicas_and_is_strictly_site_scoped() {
        let query = site_deletion_query("owl_site' OR 1 = 1");
        assert!(query.starts_with("ALTER TABLE owleye_events DELETE WHERE site_id = "));
        assert!(query.contains("owl_site\\' OR 1 = 1"));
        assert!(query.ends_with("SETTINGS mutations_sync = 2"));
        assert!(!query.contains("owleye_performance"));
    }

    #[tokio::test]
    async fn credentials_are_stripped_from_request_and_error_url() {
        let clickhouse = ClickHouse::new(
            "http://alice:super-secret@127.0.0.1:8123/default?password=query-secret&x=1".to_owned(),
        )
        .unwrap();
        assert!(!clickhouse.url.contains("alice"));
        assert!(!clickhouse.url.contains("super-secret"));
        assert!(!clickhouse.url.contains("query-secret"));
        assert!(clickhouse.url.contains("x=1"));
        let auth = clickhouse.auth.as_ref().unwrap();
        assert_eq!(auth.username, "alice");
        assert_eq!(auth.password.as_deref(), Some("query-secret"));
    }

    #[tokio::test]
    async fn insert_completion_wait_is_bounded() {
        let (_completion, completed) = oneshot::channel::<Result<(), String>>();
        let result = await_insert_completion(completed, Duration::from_millis(1)).await;
        assert!(matches!(result, Err(ApiError::ServiceUnavailable(_))));
    }

    #[derive(serde::Deserialize)]
    struct PingRow {
        value: u8,
    }

    #[derive(serde::Deserialize)]
    struct ColumnCountRow {
        value: u64,
    }

    #[tokio::test]
    #[ignore = "requires OWLEYE_TEST_CLICKHOUSE_URL; run with --ignored"]
    async fn cleanup_preserves_daily_totals_once_across_partial_deletion_and_retries() {
        let url =
            std::env::var("OWLEYE_TEST_CLICKHOUSE_URL").expect("set an isolated ClickHouse URL");
        let clickhouse = ClickHouse::new(url).unwrap();
        clickhouse.init().await.unwrap();
        let site = format!("deletion-test-{}", uuid::Uuid::new_v4());
        let batch = uuid::Uuid::new_v4().to_string();
        let mut rows = Vec::new();
        for (kind, name, tier) in [
            ("pageview", "page_view", "free"),
            ("external", "error", "free"),
            ("performance", "duration", "free"),
        ] {
            let mut row = event_row(&uuid::Uuid::new_v4().to_string());
            row.site_id = site.clone();
            row.event_type = kind.into();
            row.event_name = name.into();
            row.retention_tier = tier.into();
            rows.push(row);
        }
        clickhouse
            .insert_events_after(&site, rows, || async { Ok(()) })
            .await
            .unwrap();
        let predicate = format!("site_id = {}", clickhouse_string(&site));
        // Simulate a snapshot succeeding and deletion only partly completing.
        clickhouse
            .snapshot_deleted_event_totals(&batch, &predicate)
            .await
            .unwrap();
        let before = clickhouse
            .query_json_each_row::<serde_json::Value>(&format!(
                "SELECT batch_id FROM owleye_event_deletion_completions WHERE batch_id = {}",
                clickhouse_string(&batch)
            ))
            .await
            .unwrap();
        assert!(
            before.is_empty(),
            "prepared snapshots are not completed deletions"
        );
        clickhouse.execute(&format!("ALTER TABLE owleye_events DELETE WHERE {predicate} AND event_type = 'external' SETTINGS mutations_sync = 2")).await.unwrap();
        clickhouse.delete_site_events(&site, &batch).await.unwrap();
        clickhouse.delete_site_events(&site, &batch).await.unwrap();
        let totals = clickhouse.query_json_each_row::<serde_json::Value>(&format!("SELECT toUInt32(events) AS events,toUInt32(pageviews) AS pageviews,toUInt32(errors) AS errors,toUInt32(performance) AS performance FROM owleye_deleted_event_daily_totals FINAL WHERE batch_id = {}", clickhouse_string(&batch))).await.unwrap();
        assert_eq!(
            totals,
            vec![serde_json::json!({"events":3,"pageviews":1,"errors":1,"performance":1})]
        );
        let remaining = clickhouse
            .query_json_each_row::<serde_json::Value>(&format!(
                "SELECT toUInt32(count()) AS count FROM owleye_events WHERE site_id = {}",
                clickhouse_string(&site)
            ))
            .await
            .unwrap();
        assert_eq!(remaining[0]["count"], 0);
        let erasure_batch = uuid::Uuid::new_v4().to_string();
        clickhouse
            .delete_site_events(&site, &erasure_batch)
            .await
            .unwrap();
        clickhouse
            .delete_site_events(&site, &erasure_batch)
            .await
            .unwrap();
        let totals = clickhouse.query_json_each_row::<serde_json::Value>(&format!("SELECT toUInt32(sum(events)) AS events,toUInt32(sum(pageviews)) AS pageviews FROM owleye_deleted_event_daily_totals FINAL WHERE batch_id IN ({},{})",clickhouse_string(&batch),clickhouse_string(&erasure_batch))).await.unwrap();
        assert_eq!(totals[0], serde_json::json!({"events":3,"pageviews":1}));
        let remaining = clickhouse
            .query_json_each_row::<serde_json::Value>(&format!(
                "SELECT toUInt32(count()) AS count FROM owleye_events WHERE site_id = {}",
                clickhouse_string(&site)
            ))
            .await
            .unwrap();
        assert_eq!(remaining[0]["count"], 0);
        let completed = clickhouse.query_json_each_row::<serde_json::Value>(&format!("SELECT batch_id FROM owleye_event_deletion_completions FINAL WHERE batch_id IN ({},{})", clickhouse_string(&batch),clickhouse_string(&erasure_batch))).await.unwrap();
        assert_eq!(completed.len(), 2);
        let columns = clickhouse.query_json_each_row::<serde_json::Value>("SELECT name FROM system.columns WHERE database = currentDatabase() AND table = 'owleye_deleted_event_daily_totals' ORDER BY name").await.unwrap();
        assert_eq!(
            columns,
            [
                "batch_id",
                "errors",
                "event_day",
                "events",
                "pageviews",
                "performance"
            ]
            .map(|name| serde_json::json!({"name":name}))
            .to_vec()
        );
    }

    fn event_row(event_id: &str) -> EventRow {
        EventRow {
            anon_session_id: "session".to_owned(),
            anon_user_id: "user".to_owned(),
            browser_name: "Chrome".to_owned(),
            browser_version: "124".to_owned(),
            city: "Chicago".to_owned(),
            color_scheme: "dark".to_owned(),
            continent: "NA".to_owned(),
            country: "US".to_owned(),
            device_pixel_ratio: Some(2.0),
            device_type: "desktop".to_owned(),
            duration_ms: None,
            event_id: event_id.to_owned(),
            event_name: "page_view".to_owned(),
            event_type: "pageview".to_owned(),
            latitude: Some(41.8781),
            locale: "en-US".to_owned(),
            longitude: Some(-87.6298),
            occurred_at: "2026-01-01 00:00:00.000".to_owned(),
            os_name: "macOS".to_owned(),
            os_version: "15".to_owned(),
            page_title: "Home".to_owned(),
            payload_json: "null".to_owned(),
            received_at: "2026-01-01 00:00:00.000".to_owned(),
            referrer: String::new(),
            referrer_host: String::new(),
            region: "Illinois".to_owned(),
            retention_active_until: Some("2026-04-01 00:00:00.000".to_owned()),
            retention_days: 90,
            retention_policy_version: 1,
            retention_purge_after: Some("2026-05-01 00:00:00.000".to_owned()),
            retention_tier: "free".to_owned(),
            rule_id: String::new(),
            rule_type: String::new(),
            screen_height: Some(1080),
            screen_width: Some(1920),
            sdk_name: "owleye-js".to_owned(),
            sdk_version: "0.1.0".to_owned(),
            site_id: "site_test".to_owned(),
            timezone: "America/Chicago".to_owned(),
            tier_at_ingestion: "free".to_owned(),
            url: "https://example.com/".to_owned(),
            url_hash: String::new(),
            url_host: "example.com".to_owned(),
            url_path: "/".to_owned(),
            url_search: String::new(),
            utm_campaign: String::new(),
            utm_medium: String::new(),
            utm_source: String::new(),
            visitor_id: "visitor".to_owned(),
            viewport_height: Some(900),
            viewport_width: Some(1440),
        }
    }
}
