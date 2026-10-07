//! Only public overview aggregates belong here. Authorization is checked by the
//! handler before and after every lookup, including cache hits.
use crate::ApiError;
use chrono::NaiveDate;
use serde_json::Value;
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

const TTL: Duration = Duration::from_secs(60);
const FAILURE_BACKOFF: Duration = Duration::from_secs(1);
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_ENTRIES: usize = 128;
const MAX_FETCHES: usize = 2;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) struct Key {
    pub site_id: String,
    pub tracking_id: String,
    pub revision: String,
    pub days: u16,
    pub date: NaiveDate,
}

struct Entry {
    stored: Instant,
    // A brief failed-fetch backoff prevents queued callers hammering a failing DB.
    data: Option<Value>,
}
type Slot = Arc<AsyncMutex<Option<Entry>>>;
type Entries = HashMap<Key, (Instant, Slot)>;

#[derive(Clone)]
pub(crate) struct PublicOverviewCache {
    entries: Arc<Mutex<Entries>>,
    fetches: Arc<Semaphore>,
}

impl Default for PublicOverviewCache {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            fetches: Arc::new(Semaphore::new(MAX_FETCHES)),
        }
    }
}

fn busy() -> ApiError {
    ApiError::TooManyRequests {
        retry_after_seconds: 1,
    }
}
fn unavailable() -> ApiError {
    ApiError::ServiceUnavailable(
        "Public dashboard temporarily unavailable. Try again shortly.".into(),
    )
}

impl PublicOverviewCache {
    fn slot(&self, key: Key) -> Result<Slot, ApiError> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some((used, slot)) = entries.get_mut(&key) {
            *used = Instant::now();
            return Ok(slot.clone());
        }
        if entries.len() >= MAX_ENTRIES {
            // Never evict an in-flight slot: that would allow duplicate fetches.
            let oldest = entries
                .iter()
                .filter(|(_, (_, slot))| Arc::strong_count(slot) == 1)
                .min_by_key(|(_, (used, _))| *used)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                entries.remove(&oldest);
            } else {
                return Err(busy());
            }
        }
        let slot = Arc::new(AsyncMutex::new(None));
        entries.insert(key, (Instant::now(), slot.clone()));
        Ok(slot)
    }

    pub(super) async fn get_or_load<F, Fut>(&self, key: Key, load: F) -> Result<Value, ApiError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value, ApiError>>,
    {
        let slot = self.slot(key)?;
        let mut cached = tokio::time::timeout(FETCH_TIMEOUT, slot.lock())
            .await
            .map_err(|_| busy())?;
        if let Some(entry) = cached.as_ref() {
            if let Some(data) = &entry.data {
                if entry.stored.elapsed() < TTL {
                    return Ok(data.clone());
                }
            } else if entry.stored.elapsed() < FAILURE_BACKOFF {
                return Err(unavailable());
            }
        }
        // Cache hits and callers waiting for the same key consume no extra permit.
        // Different cold keys fail fast instead of building an unbounded DB queue.
        let _permit = self.fetches.try_acquire().map_err(|_| busy())?;
        let result = tokio::time::timeout(FETCH_TIMEOUT, load())
            .await
            .unwrap_or_else(|_| Err(unavailable()));
        *cached = Some(Entry {
            stored: Instant::now(),
            data: result.as_ref().ok().cloned(),
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn key() -> Key {
        Key {
            site_id: "site".into(),
            tracking_id: "owl_site".into(),
            revision: "rev1".into(),
            days: 7,
            date: NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
        }
    }

    #[tokio::test]
    async fn simultaneous_reads_share_one_fetch_and_expired_entries_refresh() {
        let cache = PublicOverviewCache::default();
        let calls = AtomicUsize::new(0);
        let load = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(json!({"pageviews": 23}))
        };
        let (a, b, c) = tokio::join!(
            cache.get_or_load(key(), load),
            cache.get_or_load(key(), load),
            cache.get_or_load(key(), load)
        );
        assert_eq!(a.unwrap(), b.unwrap());
        assert_eq!(c.unwrap()["pageviews"], 23);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let slot = cache.slot(key()).unwrap();
        slot.lock().await.as_mut().unwrap().stored = Instant::now() - TTL;
        cache.get_or_load(key(), load).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cache_keys_isolate_site_tracking_id_revision_range_and_utc_day() {
        let cache = PublicOverviewCache::default();
        cache
            .get_or_load(key(), || async { Ok(json!("original")) })
            .await
            .unwrap();
        let mut keys = vec![key(); 5];
        keys[0].site_id = "other-site".into();
        keys[1].tracking_id = "owl_other".into();
        keys[2].revision = "rev2".into();
        keys[3].days = 30;
        keys[4].date = key().date.succ_opt().unwrap();
        for key in keys {
            assert_eq!(
                cache
                    .get_or_load(key, || async { Ok(json!("fresh")) })
                    .await
                    .unwrap(),
                "fresh"
            );
        }
        assert_eq!(
            cache
                .get_or_load(key(), || async { panic!("must hit") })
                .await
                .unwrap(),
            "original"
        );
    }

    #[tokio::test]
    async fn cold_fetch_capacity_fails_fast_but_cached_reads_still_work() {
        let cache = PublicOverviewCache::default();
        cache
            .get_or_load(key(), || async { Ok(json!(23)) })
            .await
            .unwrap();
        let permits = cache
            .fetches
            .acquire_many(MAX_FETCHES as u32)
            .await
            .unwrap();
        let mut cold = key();
        cold.days = 30;
        assert!(matches!(
            cache
                .get_or_load(cold.clone(), || async { panic!("no capacity") })
                .await,
            Err(ApiError::TooManyRequests { .. })
        ));
        assert_eq!(
            cache
                .get_or_load(key(), || async { panic!("cache hit") })
                .await
                .unwrap(),
            23
        );
        drop(permits);
        assert_eq!(
            cache
                .get_or_load(cold, || async { Ok(json!(42)) })
                .await
                .unwrap(),
            42
        );
    }

    #[tokio::test]
    async fn failed_fetches_back_off_then_retry_without_serving_stale_data() {
        let cache = PublicOverviewCache::default();
        cache
            .get_or_load(key(), || async { Ok(json!(23)) })
            .await
            .unwrap();
        let slot = cache.slot(key()).unwrap();
        slot.lock().await.as_mut().unwrap().stored = Instant::now() - TTL;
        assert!(cache
            .get_or_load(key(), || async { Err(unavailable()) })
            .await
            .is_err());
        assert!(cache
            .get_or_load(key(), || async { panic!("backoff") })
            .await
            .is_err());
        slot.lock().await.as_mut().unwrap().stored = Instant::now() - FAILURE_BACKOFF;
        assert_eq!(
            cache
                .get_or_load(key(), || async { Ok(json!(42)) })
                .await
                .unwrap(),
            42
        );
    }

    #[tokio::test]
    async fn cancelling_a_fetch_releases_slot_and_global_capacity() {
        let cache = PublicOverviewCache::default();
        let (started, ready) = tokio::sync::oneshot::channel();
        let cloned = cache.clone();
        let task = tokio::spawn(async move {
            cloned
                .get_or_load(key(), || async {
                    started.send(()).unwrap();
                    std::future::pending::<Result<Value, ApiError>>().await
                })
                .await
        });
        ready.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(cache.fetches.available_permits(), MAX_FETCHES);
        assert_eq!(
            cache
                .get_or_load(key(), || async { Ok(json!(42)) })
                .await
                .unwrap(),
            42
        );
    }

    #[test]
    fn cache_is_bounded_and_never_evicts_inflight_slots() {
        let cache = PublicOverviewCache::default();
        let mut held = Vec::new();
        for n in 0..MAX_ENTRIES {
            let mut key = key();
            key.site_id = n.to_string();
            held.push(cache.slot(key).unwrap());
        }
        assert!(matches!(
            cache.slot(key()),
            Err(ApiError::TooManyRequests { .. })
        ));
        drop(held);
        assert!(cache.slot(key()).is_ok());
        assert_eq!(cache.entries.lock().unwrap().len(), MAX_ENTRIES);
    }
}
