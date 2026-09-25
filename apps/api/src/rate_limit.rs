use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::Response,
};
use ipnet::IpNet;
use tokio::sync::Mutex;

use crate::{
    errors::ApiError,
    privacy::{identity::anonymous_rate_limit_key, request_ip::client_ip_from_request},
    security::ConsoleRateLimitIdentity,
    AppState,
};

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<[RateLimiterInner; 5]>,
    max_entries: [usize; 5],
}

struct RateLimiterInner {
    entry_count: AtomicUsize,
    shards: Box<[Mutex<RateLimiterState>]>,
}

// Reserve capacity for control-plane traffic so rotating public visitors cannot
// prevent new sessions  from obtaining a bucket.
#[derive(Clone, Copy)]
#[repr(usize)]
enum TrafficClass {
    Console,
    Auth,
    Public,
    Developer,
}

#[derive(Clone, Copy)]
pub struct RateLimitPolicy {
    class: TrafficClass,
    name: &'static str,
    max_requests: u32,
    window: Duration,
}

struct Bucket {
    tokens: f64,
    updated_at: Instant,
}

#[derive(Default)]
struct RateLimiterState {
    entries: HashMap<String, Bucket>,
    last_cleanup: Option<Instant>,
}

// Keep idle buckets at least as long as the longest policy window (OTP recipient).
const ENTRY_RETENTION: Duration = Duration::from_secs(30 * 60);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);
// A single-node limiter should degrade to 429s, not unbounded memory growth,
// when an attacker rotates source addresses faster than entries expire.
const DEFAULT_MAX_ENTRIES: [usize; 5] = [4_000, 4_000, 41_000, 1_000, 4_000];
const SHARD_COUNT: usize = 64;

// Limits are intentionally process-local. The deployment preflight enforces a
// single API replica until this state moves to a shared rate-limit backend.

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            inner: Arc::new(std::array::from_fn(|_| RateLimiterInner {
                entry_count: AtomicUsize::new(0),
                shards: (0..SHARD_COUNT)
                    .map(|_| Mutex::new(RateLimiterState::default()))
                    .collect(),
            })),
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }
}

impl RateLimiter {
    pub async fn check(
        &self,
        policy: RateLimitPolicy,
        key: impl AsRef<str>,
    ) -> Result<(), ApiError> {
        self.check_cost(policy, key, 1).await
    }

    async fn check_cost(
        &self,
        policy: RateLimitPolicy,
        key: impl AsRef<str>,
        cost: u32,
    ) -> Result<(), ApiError> {
        let now = Instant::now();
        let pool = &self.inner[policy.class as usize];
        let max_entries = self.max_entries[policy.class as usize];
        let full_key = format!("{}:{}", policy.name, key.as_ref());
        let shard_index = shard_index(&full_key, pool.shards.len());
        let mut state = pool.shards[shard_index].lock().await;
        if state
            .last_cleanup
            .is_none_or(|last_cleanup| now.duration_since(last_cleanup) >= CLEANUP_INTERVAL)
        {
            let before = state.entries.len();
            state
                .entries
                .retain(|_, bucket| now.duration_since(bucket.updated_at) <= ENTRY_RETENTION);
            pool.entry_count
                .fetch_sub(before - state.entries.len(), Ordering::AcqRel);
            state.last_cleanup = Some(now);
        }

        if !state.entries.contains_key(&full_key)
            && pool
                .entry_count
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                    (count < max_entries).then_some(count + 1)
                })
                .is_err()
        {
            return Err(capacity_error());
        }

        let bucket = state.entries.entry(full_key).or_insert_with(|| Bucket {
            tokens: f64::from(policy.max_requests),
            updated_at: now,
        });
        let refill_per_second = f64::from(policy.max_requests) / policy.window.as_secs_f64();
        bucket.tokens = (bucket.tokens
            + now.duration_since(bucket.updated_at).as_secs_f64() * refill_per_second)
            .min(f64::from(policy.max_requests));
        bucket.updated_at = now;

        let cost = f64::from(cost);
        if bucket.tokens < cost {
            return Err(ApiError::TooManyRequests {
                retry_after_seconds: ((cost - bucket.tokens) / refill_per_second).ceil() as u64,
            });
        }

        bucket.tokens -= cost;
        Ok(())
    }
}

fn shard_index(key: &str, shard_count: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish() as usize % shard_count
}

fn capacity_error() -> ApiError {
    ApiError::TooManyRequests {
        retry_after_seconds: ENTRY_RETENTION.as_secs(),
    }
}

impl RateLimitPolicy {
    pub fn strict() -> Self {
        Self {
            class: TrafficClass::Auth,
            name: "strict",
            max_requests: 10,
            window: Duration::from_secs(60),
        }
    }

    pub fn normal() -> Self {
        Self {
            class: TrafficClass::Console,
            name: "normal",
            max_requests: 60,
            window: Duration::from_secs(60),
        }
    }

    pub fn developer_organization(max_requests: u32) -> Self {
        Self {
            class: TrafficClass::Developer,
            name: "developer_org",
            max_requests,
            window: Duration::from_secs(60),
        }
    }

    pub fn developer() -> Self {
        Self {
            class: TrafficClass::Developer,
            name: "developer",
            max_requests: 600,
            window: Duration::from_secs(60),
        }
    }

    pub fn auth() -> Self {
        Self::strict()
    }

    pub fn console() -> Self {
        Self::normal()
    }

    pub fn sdk_events() -> Self {
        Self {
            class: TrafficClass::Public,
            name: "sdk-events",
            max_requests: 6_000,
            window: Duration::from_secs(60),
        }
    }

    pub fn sdk_client() -> Self {
        Self {
            class: TrafficClass::Public,
            name: "sdk-client",
            max_requests: 6_000,
            window: Duration::from_secs(60),
        }
    }

    pub fn sdk_site() -> Self {
        Self {
            class: TrafficClass::Public,
            name: "sdk-site",
            max_requests: 100_000,
            window: Duration::from_secs(60),
        }
    }

    pub fn public_overview_site() -> Self {
        Self {
            class: TrafficClass::Public,
            name: "public-overview-site",
            max_requests: 60,
            window: Duration::from_secs(60),
        }
    }

    pub fn sdk_rules() -> Self {
        Self {
            class: TrafficClass::Public,
            name: "sdk-rules",
            max_requests: 1_200,
            window: Duration::from_secs(60),
        }
    }
}

pub async fn auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    limit_request(state, request, next, RateLimitPolicy::auth()).await
}

pub async fn strict(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    limit_request(state, request, next, RateLimitPolicy::strict()).await
}

pub async fn developer(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    limit_request(state, request, next, RateLimitPolicy::developer()).await
}

pub async fn normal(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    limit_request(state, request, next, RateLimitPolicy::normal()).await
}

pub async fn console(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let request_key = request_key(&state, &request);
    let session_id = request
        .extensions()
        .get::<ConsoleRateLimitIdentity>()
        .map(|identity| identity.0.as_str())
        .unwrap_or("missing-session");
    state
        .rate_limiter
        .check(
            RateLimitPolicy::console(),
            format!("{session_id}:{request_key}"),
        )
        .await?;
    Ok(next.run(request).await)
}

pub async fn public_overview(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    limit_request(
        state,
        request,
        next,
        RateLimitPolicy {
            class: TrafficClass::Public,
            name: "public-overview-client",
            max_requests: 30,
            window: Duration::from_secs(60),
        },
    )
    .await
}

pub async fn sdk_ingest(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // Run before JSON extraction, key lookup and site resolution. The key never
    // contains an unvalidated site identifier.
    limit_request(state, request, next, RateLimitPolicy::sdk_client()).await
}

pub async fn sdk_rules(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    limit_request(state, request, next, RateLimitPolicy::sdk_rules()).await
}

pub fn request_ip(request: &Request, trusted_proxies: &[IpNet]) -> IpAddr {
    let peer_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0.ip())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));

    client_ip_from_request(request.headers(), peer_ip, trusted_proxies)
}

pub(crate) async fn check_sdk_events_for_site(
    state: &AppState,
    client_ip: IpAddr,
    site_id: &str,
    events: usize,
    site_ceiling: u32,
) -> Result<(), ApiError> {
    let ip_key = anonymous_rate_limit_key(client_ip, &state.settings.hash_salt);
    state
        .rate_limiter
        .check_cost(
            RateLimitPolicy::sdk_events(),
            format!("{site_id}:{ip_key}"),
            events as u32,
        )
        .await?;
    state
        .rate_limiter
        .check_cost(
            RateLimitPolicy {
                max_requests: site_ceiling,
                ..RateLimitPolicy::sdk_site()
            },
            site_id,
            events as u32,
        )
        .await
}

async fn limit_request(
    state: AppState,
    request: Request,
    next: Next,
    policy: RateLimitPolicy,
) -> Result<Response, ApiError> {
    let key = request_key(&state, &request);
    state.rate_limiter.check(policy, key).await?;
    Ok(next.run(request).await)
}

fn request_key(state: &AppState, request: &Request) -> String {
    anonymous_rate_limit_key(
        request_ip(request, &state.settings.trusted_proxies),
        &state.settings.hash_salt,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn public_capacity_cannot_starve_auth_console() {
        let limiter = RateLimiter {
            max_entries: [1; 5],
            ..RateLimiter::default()
        };
        limiter
            .check(RateLimitPolicy::sdk_client(), "attacker")
            .await
            .unwrap();
        assert!(limiter
            .check(RateLimitPolicy::sdk_rules(), "new")
            .await
            .is_err());
        limiter.check(RateLimitPolicy::auth(), "new").await.unwrap();
        limiter
            .check(RateLimitPolicy::console(), "new")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn batches_consume_the_number_of_events_not_one_request() {
        let limiter = RateLimiter::default();
        let policy = RateLimitPolicy::sdk_events();
        limiter
            .check_cost(policy, "site:visitor", 5_999)
            .await
            .unwrap();
        assert!(limiter.check_cost(policy, "site:visitor", 2).await.is_err());
        limiter.check(policy, "site:visitor").await.unwrap();
    }

    #[tokio::test]
    async fn rate_limiter_rejects_over_limit_requests() {
        let limiter = RateLimiter::default();
        let policy = RateLimitPolicy {
            class: TrafficClass::Console,
            name: "test",
            max_requests: 2,
            window: Duration::from_secs(60),
        };

        limiter.check(policy, "client").await.unwrap();
        limiter.check(policy, "client").await.unwrap();

        assert!(matches!(
            limiter.check(policy, "client").await,
            Err(ApiError::TooManyRequests { .. })
        ));
    }

    #[tokio::test]
    async fn cleanup_removes_expired_windows_without_scanning_on_every_request() {
        let limiter = RateLimiter::default();
        let stale_time = Instant::now() - ENTRY_RETENTION - Duration::from_secs(1);
        let shard = shard_index(
            "test:current",
            limiter.inner[TrafficClass::Console as usize].shards.len(),
        );
        {
            let mut state = limiter.inner[TrafficClass::Console as usize].shards[shard]
                .lock()
                .await;
            state.entries.insert(
                "test:stale".to_owned(),
                Bucket {
                    tokens: 1.0,
                    updated_at: stale_time,
                },
            );
            state.last_cleanup = Some(stale_time);
        }
        limiter.inner[TrafficClass::Console as usize]
            .entry_count
            .store(1, Ordering::Release);

        limiter
            .check(
                RateLimitPolicy {
                    class: TrafficClass::Console,
                    name: "test",
                    max_requests: 2,
                    window: Duration::from_secs(60),
                },
                "current",
            )
            .await
            .unwrap();

        let state = limiter.inner[TrafficClass::Console as usize].shards[shard]
            .lock()
            .await;
        assert!(!state.entries.contains_key("test:stale"));
        assert!(state.entries.contains_key("test:current"));
        assert_eq!(
            limiter.inner[TrafficClass::Console as usize]
                .entry_count
                .load(Ordering::Acquire),
            1
        );
    }

    #[tokio::test]
    async fn new_clients_fail_closed_at_capacity_without_blocking_known_clients() {
        let limiter = RateLimiter {
            max_entries: [2; 5],
            ..RateLimiter::default()
        };
        let policy = RateLimitPolicy {
            class: TrafficClass::Console,
            name: "capacity-test",
            max_requests: 2,
            window: Duration::from_secs(60),
        };

        limiter.check(policy, "known-a").await.unwrap();
        limiter.check(policy, "known-b").await.unwrap();
        assert!(matches!(
            limiter.check(policy, "new-client").await,
            Err(ApiError::TooManyRequests { .. })
        ));
        limiter.check(policy, "known-a").await.unwrap();
    }
}
