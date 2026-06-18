use std::{
    collections::HashMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

use crate::{
    config::{RateLimitBackend, RateLimitConfig},
    error::GatewayError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteClass {
    PublicTileMetadata,
    TileRedirect,
    PublicSiteRead,
    AdminRead,
    AdminWrite,
    AuthFailure,
}

impl RouteClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PublicTileMetadata => "public_tile_metadata",
            Self::TileRedirect => "tile_redirect",
            Self::PublicSiteRead => "public_site_read",
            Self::AdminRead => "admin_read",
            Self::AdminWrite => "admin_write",
            Self::AuthFailure => "auth_failure",
        }
    }

    pub fn policy(self) -> RateLimitPolicy {
        match self {
            Self::PublicTileMetadata => RateLimitPolicy::new(120, 60),
            Self::TileRedirect => RateLimitPolicy::new(600, 120),
            Self::PublicSiteRead => RateLimitPolicy::new(120, 60),
            Self::AdminRead => RateLimitPolicy::new(60, 20),
            Self::AdminWrite => RateLimitPolicy::new(10, 5),
            Self::AuthFailure => RateLimitPolicy::new(10, 5),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RateLimitPolicy {
    limit_per_minute: u32,
    burst: u32,
}

impl RateLimitPolicy {
    pub fn new(limit_per_minute: u32, burst: u32) -> Self {
        Self {
            limit_per_minute,
            burst,
        }
    }

    fn allowed_in_window(self) -> u32 {
        self.limit_per_minute + self.burst
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    store: Arc<dyn RateLimitStore>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let store: Arc<dyn RateLimitStore> = match config.backend {
            RateLimitBackend::Memory => Arc::new(MemoryRateLimitStore::default()),
            RateLimitBackend::Redis => match config.redis_url.as_deref() {
                Some(redis_url) => match RedisRateLimitStore::new(redis_url) {
                    Ok(store) => Arc::new(store),
                    Err(error) => {
                        tracing::error!(error = %error, "failed to initialize rate-limit store");
                        Arc::new(UnavailableRateLimitStore)
                    }
                },
                None => Arc::new(UnavailableRateLimitStore),
            },
        };

        Self { config, store }
    }

    pub async fn check(&self, key: String, route_class: RouteClass) -> Result<(), GatewayError> {
        let policy = route_class.policy();
        let window = Duration::from_secs(60);
        let map_key = format!("{}:{key}", route_class.as_str());

        let snapshot = self
            .store
            .increment_window(&map_key, window)
            .await
            .map_err(|error| {
                if self.config.backend == RateLimitBackend::Redis {
                    tracing::warn!(
                        route_class = route_class.as_str(),
                        error = %error,
                        "distributed rate-limit store unavailable"
                    );
                }
                GatewayError::service_unavailable("rate-limit store is unavailable")
            })?;

        if snapshot.count > u64::from(policy.allowed_in_window()) {
            return Err(GatewayError::rate_limited(snapshot.retry_after_seconds));
        }

        Ok(())
    }

    pub async fn check_store_available(&self) -> Result<(), RateLimitStoreError> {
        self.store.check_health().await
    }
}

#[derive(Debug)]
struct WindowCounter {
    started_at: Instant,
    count: u64,
}

#[derive(Debug, Clone, Copy)]
struct WindowSnapshot {
    count: u64,
    retry_after_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct RateLimitStoreError {
    operation: &'static str,
}

impl RateLimitStoreError {
    fn new(operation: &'static str) -> Self {
        Self { operation }
    }
}

impl fmt::Display for RateLimitStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "rate-limit store {} failed", self.operation)
    }
}

impl std::error::Error for RateLimitStoreError {}

type RateLimitStoreFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

trait RateLimitStore: Send + Sync {
    fn increment_window<'a>(
        &'a self,
        key: &'a str,
        window: Duration,
    ) -> RateLimitStoreFuture<'a, Result<WindowSnapshot, RateLimitStoreError>>;

    fn check_health(&self) -> RateLimitStoreFuture<'_, Result<(), RateLimitStoreError>>;
}

#[derive(Default)]
struct MemoryRateLimitStore {
    counters: Mutex<HashMap<String, WindowCounter>>,
}

impl RateLimitStore for MemoryRateLimitStore {
    fn increment_window<'a>(
        &'a self,
        key: &'a str,
        window: Duration,
    ) -> RateLimitStoreFuture<'a, Result<WindowSnapshot, RateLimitStoreError>> {
        Box::pin(async move {
            let now = Instant::now();
            let mut counters = self.counters.lock().await;
            let counter = counters
                .entry(key.to_owned())
                .or_insert_with(|| WindowCounter {
                    started_at: now,
                    count: 0,
                });

            if now.duration_since(counter.started_at) >= window {
                counter.started_at = now;
                counter.count = 0;
            }

            counter.count += 1;
            let retry_after = window
                .saturating_sub(now.duration_since(counter.started_at))
                .as_secs()
                .max(1);

            Ok(WindowSnapshot {
                count: counter.count,
                retry_after_seconds: retry_after,
            })
        })
    }

    fn check_health(&self) -> RateLimitStoreFuture<'_, Result<(), RateLimitStoreError>> {
        Box::pin(async { Ok(()) })
    }
}

struct RedisRateLimitStore {
    client: redis::Client,
}

impl RedisRateLimitStore {
    fn new(redis_url: &str) -> Result<Self, RateLimitStoreError> {
        let client =
            redis::Client::open(redis_url).map_err(|_| RateLimitStoreError::new("configure"))?;
        Ok(Self { client })
    }
}

impl RateLimitStore for RedisRateLimitStore {
    fn increment_window<'a>(
        &'a self,
        key: &'a str,
        window: Duration,
    ) -> RateLimitStoreFuture<'a, Result<WindowSnapshot, RateLimitStoreError>> {
        Box::pin(async move {
            let mut connection = self
                .client
                .get_multiplexed_async_connection()
                .await
                .map_err(|_| RateLimitStoreError::new("connect"))?;
            let window_seconds = window.as_secs().max(1);
            let script = redis::Script::new(
                r#"
local current = redis.call("INCR", KEYS[1])
if current == 1 then
  redis.call("EXPIRE", KEYS[1], ARGV[1])
end
local ttl = redis.call("TTL", KEYS[1])
if ttl < 1 then
  ttl = tonumber(ARGV[1])
end
return { current, ttl }
"#,
            );
            let (count, retry_after_seconds): (u64, u64) = script
                .key(key)
                .arg(window_seconds)
                .invoke_async(&mut connection)
                .await
                .map_err(|_| RateLimitStoreError::new("increment"))?;

            Ok(WindowSnapshot {
                count,
                retry_after_seconds: retry_after_seconds.max(1),
            })
        })
    }

    fn check_health(&self) -> RateLimitStoreFuture<'_, Result<(), RateLimitStoreError>> {
        Box::pin(async move {
            let mut connection = self
                .client
                .get_multiplexed_async_connection()
                .await
                .map_err(|_| RateLimitStoreError::new("connect"))?;
            let _: String = redis::cmd("PING")
                .query_async(&mut connection)
                .await
                .map_err(|_| RateLimitStoreError::new("ping"))?;
            Ok(())
        })
    }
}

struct UnavailableRateLimitStore;

impl RateLimitStore for UnavailableRateLimitStore {
    fn increment_window<'a>(
        &'a self,
        _key: &'a str,
        _window: Duration,
    ) -> RateLimitStoreFuture<'a, Result<WindowSnapshot, RateLimitStoreError>> {
        Box::pin(async { Err(RateLimitStoreError::new("connect")) })
    }

    fn check_health(&self) -> RateLimitStoreFuture<'_, Result<(), RateLimitStoreError>> {
        Box::pin(async { Err(RateLimitStoreError::new("connect")) })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::config::{RateLimitBackend, RateLimitConfig};

    use super::{
        RateLimitStore, RateLimitStoreError, RateLimitStoreFuture, RateLimiter, RouteClass,
        WindowSnapshot,
    };

    #[test]
    fn route_class_policies_match_public_api_contract() {
        let cases = [
            (RouteClass::PublicTileMetadata, 120, 60),
            (RouteClass::TileRedirect, 600, 120),
            (RouteClass::PublicSiteRead, 120, 60),
            (RouteClass::AdminRead, 60, 20),
            (RouteClass::AdminWrite, 10, 5),
            (RouteClass::AuthFailure, 10, 5),
        ];

        for (route_class, limit_per_minute, burst) in cases {
            let policy = route_class.policy();
            assert_eq!(policy.limit_per_minute, limit_per_minute, "{route_class:?}");
            assert_eq!(policy.burst, burst, "{route_class:?}");
        }
    }

    #[tokio::test]
    async fn memory_limiter_enforces_policy_window() {
        let limiter = RateLimiter::new(RateLimitConfig {
            backend: RateLimitBackend::Memory,
            redis_url: None,
            distributed_required: false,
        });

        for _ in 0..15 {
            limiter
                .check("admin-1:/trigger".to_owned(), RouteClass::AdminWrite)
                .await
                .unwrap();
        }

        let error = limiter
            .check("admin-1:/trigger".to_owned(), RouteClass::AdminWrite)
            .await
            .unwrap_err();

        assert_eq!(error.status, axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(error.code.as_str(), "rate_limited");
        assert!(error.retry_after_seconds.unwrap() >= 1);
    }

    #[tokio::test]
    async fn admin_writes_fail_closed_when_distributed_store_is_required() {
        let limiter = RateLimiter::new(RateLimitConfig {
            backend: RateLimitBackend::Redis,
            redis_url: None,
            distributed_required: true,
        });

        let error = limiter
            .check("admin-1:/trigger".to_owned(), RouteClass::AdminWrite)
            .await
            .unwrap_err();

        assert_eq!(error.status, axum::http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.code.as_str(), "service_unavailable");
    }

    #[tokio::test]
    async fn redis_limiter_enforces_policy_from_store_snapshot() {
        let limiter = RateLimiter {
            config: RateLimitConfig {
                backend: RateLimitBackend::Redis,
                redis_url: Some("redis://localhost:6379".to_owned()),
                distributed_required: true,
            },
            store: std::sync::Arc::new(FakeRateLimitStore {
                count: AtomicU64::new(15),
                retry_after_seconds: 37,
                fail: false,
            }),
        };

        let error = limiter
            .check("admin-1:/trigger".to_owned(), RouteClass::AdminWrite)
            .await
            .unwrap_err();

        assert_eq!(error.status, axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(error.code.as_str(), "rate_limited");
        assert_eq!(error.retry_after_seconds, Some(37));
    }

    #[tokio::test]
    async fn redis_store_errors_return_service_unavailable() {
        let limiter = RateLimiter {
            config: RateLimitConfig {
                backend: RateLimitBackend::Redis,
                redis_url: Some("redis://localhost:6379".to_owned()),
                distributed_required: true,
            },
            store: std::sync::Arc::new(FakeRateLimitStore {
                count: AtomicU64::new(0),
                retry_after_seconds: 60,
                fail: true,
            }),
        };

        let error = limiter
            .check("admin-1:/trigger".to_owned(), RouteClass::AdminWrite)
            .await
            .unwrap_err();

        assert_eq!(error.status, axum::http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.code.as_str(), "service_unavailable");
        assert_eq!(error.message, "rate-limit store is unavailable");
        assert!(error.retry_after_seconds.is_none());
    }

    struct FakeRateLimitStore {
        count: AtomicU64,
        retry_after_seconds: u64,
        fail: bool,
    }

    impl RateLimitStore for FakeRateLimitStore {
        fn increment_window<'a>(
            &'a self,
            _key: &'a str,
            _window: std::time::Duration,
        ) -> RateLimitStoreFuture<'a, Result<WindowSnapshot, RateLimitStoreError>> {
            Box::pin(async move {
                if self.fail {
                    return Err(RateLimitStoreError::new("increment"));
                }

                Ok(WindowSnapshot {
                    count: self.count.fetch_add(1, Ordering::SeqCst) + 1,
                    retry_after_seconds: self.retry_after_seconds,
                })
            })
        }

        fn check_health(&self) -> RateLimitStoreFuture<'_, Result<(), RateLimitStoreError>> {
            Box::pin(async move {
                if self.fail {
                    Err(RateLimitStoreError::new("ping"))
                } else {
                    Ok(())
                }
            })
        }
    }
}
