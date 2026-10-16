//! Request-local work accounting. Scan allowances are assigned before execution,
//! rather than refunded from optional/provisional ClickHouse summary headers.
//! Failed and cancelled statements still consume their assigned allowance.

use super::*;
use tokio::sync::OwnedSemaphorePermit;

const MAX_STATEMENTS: usize = 64;
const REQUEST_SCAN_ROWS: u64 = 20_000_000;
const REQUEST_SCAN_BYTES: u64 = 1_073_741_824;
const REQUEST_RESULT_BYTES: usize = 2_097_152;
const REQUEST_RESULT_ROWS: usize = 8_000;
const SITE_ADMISSION: usize = 2;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct ReportResourceError(&'static str);

pub(super) fn resource_error(message: &'static str) -> anyhow::Error {
    anyhow::Error::new(ReportResourceError(message))
}

pub(crate) fn report_resource_message(error: &anyhow::Error) -> Option<&'static str> {
    error
        .downcast_ref::<ReportResourceError>()
        .map(|error| error.0)
}

tokio::task_local! {
    static REPORT_SCOPE: Arc<ReportScope>;
}

#[derive(Default)]
struct Usage {
    statements: usize,
    result_bytes: usize,
    result_rows: usize,
}

pub(super) struct ReportScope {
    site: String,
    client: usize,
    max_statements: usize,
    usage: Mutex<Usage>,
    capacity: Arc<Semaphore>,
}

impl ReportScope {
    pub(super) fn site(&self) -> &str {
        &self.site
    }
    fn new(site: &str, client: usize, max_statements: usize) -> anyhow::Result<Arc<Self>> {
        anyhow::ensure!(
            !site.is_empty() && site.len() <= 256,
            "Invalid report site scope"
        );
        anyhow::ensure!(
            (1..=MAX_STATEMENTS).contains(&max_statements),
            "Invalid report statement allowance"
        );
        Ok(Arc::new(Self {
            site: site.into(),
            client,
            max_statements,
            usage: Mutex::default(),
            capacity: Arc::new(Semaphore::new(REPORT_CONCURRENCY)),
        }))
    }

    pub(super) fn scan_rows(&self) -> u64 {
        (REQUEST_SCAN_ROWS / self.max_statements as u64).min(5_000_000)
    }

    pub(super) fn scan_bytes(&self) -> u64 {
        (REQUEST_SCAN_BYTES / self.max_statements as u64).min(268_435_456)
    }

    pub(super) fn reserve_statement(&self) -> anyhow::Result<()> {
        let mut usage = recover_mutex(&self.usage);
        if usage.result_bytes > REQUEST_RESULT_BYTES || usage.result_rows > REQUEST_RESULT_ROWS {
            return Err(resource_error(
                "Analytics request exceeded its shared result budget; shorten the report and retry",
            ));
        }
        if usage.statements >= self.max_statements {
            return Err(resource_error("Analytics request exceeded its shared statement budget; shorten the report and retry"));
        }
        usage.statements += 1;
        Ok(())
    }

    pub(super) fn consume_output(&self, bytes: usize, rows: usize) -> anyhow::Result<()> {
        let mut usage = recover_mutex(&self.usage);
        // Charge bytes even when decoding/further execution subsequently fails.
        usage.result_bytes = usage.result_bytes.saturating_add(bytes);
        usage.result_rows = usage.result_rows.saturating_add(rows);
        if usage.result_bytes > REQUEST_RESULT_BYTES || usage.result_rows > REQUEST_RESULT_ROWS {
            return Err(resource_error(
                "Analytics request exceeded its shared result budget; shorten the report and retry",
            ));
        }
        Ok(())
    }

    pub(super) fn fail_output(&self, message: &'static str) -> anyhow::Error {
        // Malformed/interrupted/local-limit failures have no complete observed
        // row count. Refuse all further reads even when a caller catches one
        // optional child failure; never reclaim an uncertain output allowance.
        recover_mutex(&self.usage).result_bytes = REQUEST_RESULT_BYTES + 1;
        resource_error(message)
    }
}

#[derive(Default)]
pub(super) struct SiteAdmissions {
    sites: HashMap<String, Arc<Semaphore>>,
}

impl SiteAdmissions {
    fn get(&mut self, site: &str) -> Arc<Semaphore> {
        // At most eight accepted statements hold references; discard idle site
        // keys on each access so many distinct tenants cannot grow this cache.
        self.sites.retain(|_, gate| Arc::strong_count(gate) > 1);
        self.sites
            .entry(site.into())
            .or_insert_with(|| Arc::new(Semaphore::new(SITE_ADMISSION)))
            .clone()
    }
}

pub(super) struct ReportPermits {
    _site: OwnedSemaphorePermit,
    _admission: OwnedSemaphorePermit,
    _execution: OwnedSemaphorePermit,
    _request: OwnedSemaphorePermit,
}

impl ClickHouse {
    fn report_client_id(&self) -> usize {
        Arc::as_ptr(&self.report_capacity) as usize
    }

    /// Scope an already authorized, single-site reporting future. Nested same-
    /// site work inherits the original budget; it cannot reset/increase it.
    /// Task-local context follows joined/nested futures, never spawned tasks.
    pub(crate) async fn with_report_scope<T, E, F>(
        &self,
        site: &str,
        max_statements: usize,
        future: F,
    ) -> Result<T, E>
    where
        E: From<anyhow::Error>,
        F: Future<Output = Result<T, E>>,
    {
        if let Ok(current) = REPORT_SCOPE.try_with(Arc::clone) {
            if current.site != site || current.client != self.report_client_id() {
                return Err(anyhow::anyhow!("A reporting request cannot change site scope").into());
            }
            return future.await;
        }
        let scope = ReportScope::new(site, self.report_client_id(), max_statements)?;
        REPORT_SCOPE.scope(scope, future).await
    }

    fn active_report_scope(&self) -> anyhow::Result<Arc<ReportScope>> {
        let scope = REPORT_SCOPE.try_with(Arc::clone).map_err(|_| {
            anyhow::anyhow!("Analytics reports require an authorized request scope")
        })?;
        anyhow::ensure!(
            scope.client == self.report_client_id(),
            "Report client scope changed"
        );
        Ok(scope)
    }

    pub(super) fn required_report_scope(
        &self,
        _fixture_site: Option<&str>,
    ) -> anyhow::Result<Arc<ReportScope>> {
        match self.active_report_scope() {
            Ok(scope) => Ok(scope),
            // Existing isolated SQL fixtures intentionally call one reader
            // directly. Production readers always require explicit route scope.
            #[cfg(test)]
            Err(_) if REPORT_SCOPE.try_with(|_| ()).is_err() => ReportScope::new(
                _fixture_site.unwrap_or("standalone-sql-fixture"),
                self.report_client_id(),
                1,
            ),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn report_permits(
        &self,
        scope: &ReportScope,
    ) -> anyhow::Result<ReportPermits> {
        // A report may join more than two child reads (e.g. performance's three
        // cards). Queue these inside its bounded statement allowance instead of
        // treating its third child as a competing same-site request.
        // This is finite intra-request work, not shared queue admission. Its
        // predecessors have bounded queue/HTTP times; a three-card report must
        // not time out its third read before a valid five-second read finishes.
        let request = scope.capacity.clone().acquire_owned().await?;
        let admission = self
            .report_admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| resource_error("Analytics report queue is full; retry shortly"))?;
        let site_gate = recover_mutex(&self.report_sites).get(&scope.site);
        let site = site_gate.try_acquire_owned().map_err(|_| {
            resource_error("This site's analytics report capacity is busy; retry shortly")
        })?;
        // Tokio's semaphore is FIFO. With at most two admitted statements per
        // site, one tenant cannot occupy the bounded global waiting queue.
        let execution = time::timeout(
            Duration::from_secs(3),
            self.report_capacity.clone().acquire_owned(),
        )
        .await
        .map_err(|_| resource_error("Analytics report capacity is busy; retry shortly"))??;
        Ok(ReportPermits {
            _site: site,
            _admission: admission,
            _execution: execution,
            _request: request,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, response::IntoResponse};

    #[test]
    fn report_assigned_scan_allowances_bound_all_statements_including_growth() {
        for count in 1..=MAX_STATEMENTS {
            let scope = ReportScope::new("authorized", 1, count).unwrap();
            assert!(scope.scan_rows() * count as u64 <= REQUEST_SCAN_ROWS);
            assert!(scope.scan_bytes() * count as u64 <= REQUEST_SCAN_BYTES);
            assert!(scope.scan_rows() <= 5_000_000);
            assert!(scope.scan_bytes() <= 268_435_456);
            for _ in 0..count {
                scope.reserve_statement().unwrap();
            }
            assert!(scope.reserve_statement().is_err());
        }
        assert!(ReportScope::new("authorized", 1, 0).is_err());
        assert!(ReportScope::new("authorized", 1, MAX_STATEMENTS + 1).is_err());
    }

    #[test]
    fn report_actual_output_is_shared_and_failure_does_not_refund_it() {
        let scope = ReportScope::new("authorized", 1, 14).unwrap();
        scope
            .consume_output(REQUEST_RESULT_BYTES - 10, REQUEST_RESULT_ROWS - 1)
            .unwrap();
        scope.consume_output(10, 1).unwrap();
        assert!(scope.consume_output(1, 0).is_err());
        assert!(scope.consume_output(0, 0).is_err());
        let other = ReportScope::new("authorized", 1, 14).unwrap();
        other.consume_output(0, REQUEST_RESULT_ROWS).unwrap();
        assert!(other.consume_output(0, 1).is_err());
        // A separately admitted request has its own output accounting.
        ReportScope::new("another-site", 1, 1)
            .unwrap()
            .consume_output(1, 1)
            .unwrap();
    }

    #[tokio::test]
    async fn report_nested_scope_cannot_reset_budget_change_tenant_or_escape_to_spawn() {
        let ch = ClickHouse::new("http://127.0.0.1:1".into()).unwrap();
        assert!(ch.active_report_scope().is_err());
        ch.with_report_scope("site-a", 1, async {
            ch.active_report_scope()?.reserve_statement()?;
            ch.with_report_scope("site-a", 64, async {
                assert!(ch.active_report_scope()?.reserve_statement().is_err());
                Ok::<_, anyhow::Error>(())
            })
            .await?;
            assert!(ch
                .with_report_scope("site-b", 1, async { Ok::<_, anyhow::Error>(()) })
                .await
                .is_err());
            let clone = ch.clone();
            assert!(tokio::spawn(async move { clone.active_report_scope().is_err() }).await?);
            Ok::<_, anyhow::Error>(())
        })
        .await
        .unwrap();
        // No ambient budget remains after completion, nor on another request.
        assert!(ch.active_report_scope().is_err());
        ch.with_report_scope("site-b", 1, async {
            ch.active_report_scope()?.reserve_statement()?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn report_resource_errors_are_safe_retryable_service_errors() {
        let scope = ReportScope::new("secret-site", 1, 1).unwrap();
        scope.reserve_statement().unwrap();
        let error = scope.reserve_statement().unwrap_err();
        let response = ApiError::from(error).into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()["retry-after"], "3");
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["error"]["code"], "service_unavailable");
        assert!(payload["error"]["message"]
            .as_str()
            .unwrap()
            .contains("retry"));
        assert!(!String::from_utf8(bytes.to_vec())
            .unwrap()
            .contains("secret-site"));
    }

    #[test]
    fn report_site_admission_cache_discards_idle_tenants() {
        let mut sites = SiteAdmissions::default();
        let busy = sites.get("busy").try_acquire_owned().unwrap();
        for index in 0..1000 {
            drop(sites.get(&format!("site-{index}")));
        }
        assert!(sites.sites.len() <= 2);
        drop(busy);
        drop(sites.get("last"));
        assert_eq!(sites.sites.len(), 1);
    }
}
