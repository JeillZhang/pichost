use std::sync::Arc;
use std::time::Duration;

use pichost_core::state::{RateLimitResult, RateLimiter, RateLimiterError};
use pichost_core::DbType;

use axum::{
    extract::{Request, State},
    http::header::ACCEPT_LANGUAGE,
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};

use pichost_core::i18n::I18n;

use crate::app::AppState;
use crate::i18n_ext::{error_json_args, locale_from_header};
use crate::middleware::auth::AuthUser;

/// Rate limits are read from config (defaults match these constants).
/// Window is fixed at 60s; per-policy maxima are configurable so deployments
/// and E2E test environments can raise them (PICHOST_RATE_LIMIT_*_MAX).
const RATE_LIMIT_WINDOW_SECS: u64 = 60;

fn too_many_response(req: &Request, retry_after: u64) -> (StatusCode, Json<serde_json::Value>) {
    let locale = locale_from_header(req.headers().get(ACCEPT_LANGUAGE), I18n::global().language());
    error_json_args(
        locale,
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        &[retry_after.to_string()],
    )
}

fn rl_key(policy: &str, suffix: &str) -> String {
    format!("rl:{policy}:{suffix}")
}

/// Redis-backed `RateLimiter` implementation. Keys are `rl:{policy}:{key}`;
/// counters are INCR'd with the window TTL. On deny the remaining key TTL is
/// returned as `retry_after` (falling back to the full window) — same
/// semantics the middleware previously got from its own TTL query.
pub struct RedisRateLimiter {
    cache: crate::cache::Cache,
}

impl RedisRateLimiter {
    pub fn new(cache: crate::cache::Cache) -> Self {
        Self { cache }
    }

    async fn ttl_or_window(&self, redis_key: &str, window_secs: u64) -> u64 {
        let mut conn = match self.cache.get_pool().get().await {
            Ok(c) => c,
            Err(_) => return window_secs,
        };
        let ttl: u64 = deadpool_redis::redis::cmd("TTL")
            .arg(redis_key)
            .query_async(&mut *conn)
            .await
            .unwrap_or(window_secs);
        ttl
    }
}

#[async_trait::async_trait]
impl RateLimiter for RedisRateLimiter {
    async fn check(
        &self,
        policy: &str,
        key: &str,
        limit: u32,
        window: Duration,
    ) -> Result<RateLimitResult, RateLimiterError> {
        let redis_key = rl_key(policy, key);
        let count = self
            .cache
            .incr(&redis_key, window.as_secs())
            .await
            .map_err(|e| RateLimiterError::Other(e.to_string()))?;
        if count <= limit as u64 {
            Ok(RateLimitResult {
                allowed: true,
                retry_after: 0,
            })
        } else {
            let retry_after = self.ttl_or_window(&redis_key, window.as_secs()).await;
            Ok(RateLimitResult {
                allowed: false,
                retry_after,
            })
        }
    }
}

fn extract_client_ip(req: &Request) -> String {
    if let Some(xff) = req.headers().get("x-forwarded-for") {
        if let Ok(val) = xff.to_str() {
            if let Some(ip) = val.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }
    "unknown".to_string()
}

pub async fn rate_limit_auth<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let ip = extract_client_ip(&req);
    let max = state.config.rate_limit.auth_max;
    let result = limiter_check(&state, "auth", &ip, max).await;
    if result.allowed {
        Ok(next.run(req).await)
    } else {
        tracing::warn!(ip = %ip, "auth rate limited");
        Err(too_many_response(&req, result.retry_after))
    }
}

pub async fn rate_limit_upload<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let key = req
        .extensions()
        .get::<AuthUser>()
        .map(|u| u.id.to_string())
        .unwrap_or_else(|| extract_client_ip(&req));
    let max = state.config.rate_limit.upload_max;
    let result = limiter_check(&state, "upload", &key, max).await;
    if result.allowed {
        Ok(next.run(req).await)
    } else {
        tracing::warn!(key = %key, "upload rate limited");
        Err(too_many_response(&req, result.retry_after))
    }
}

pub async fn rate_limit_general<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let key = req
        .extensions()
        .get::<AuthUser>()
        .map(|u| u.id.to_string())
        .unwrap_or_else(|| extract_client_ip(&req));
    let max = state.config.rate_limit.general_max;
    let result = limiter_check(&state, "general", &key, max).await;
    if result.allowed {
        Ok(next.run(req).await)
    } else {
        tracing::warn!(key = %key, "general rate limited");
        Err(too_many_response(&req, result.retry_after))
    }
}

pub async fn rate_limit_public<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let ip = extract_client_ip(&req);
    let max = state.config.rate_limit.public_max;
    let result = limiter_check(&state, "public", &ip, max).await;
    if result.allowed {
        Ok(next.run(req).await)
    } else {
        tracing::warn!(ip = %ip, "public rate limited");
        Err(too_many_response(&req, result.retry_after))
    }
}

/// Runs the rate-limiter check for the fixed 60s window. On limiter errors
/// the request is allowed through (fail-open, matching the pre-trait
/// behavior where a Redis error returned the full allowance).
async fn limiter_check<DB: DbType>(
    state: &Arc<AppState<DB>>,
    policy: &str,
    key: &str,
    max_requests: u32,
) -> RateLimitResult {
    match state
        .rate_limiter
        .check(policy, key, max_requests, Duration::from_secs(RATE_LIMIT_WINDOW_SECS))
        .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("Rate limit Redis error: {e}");
            RateLimitResult {
                allowed: true,
                retry_after: 0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::middleware;
    use axum::Router;
    use tower::ServiceExt;
    use uuid::Uuid;

    #[test]
    fn test_rl_key() {
        assert_eq!(rl_key("auth", "1.2.3.4"), "rl:auth:1.2.3.4");
    }

    #[test]
    fn test_extract_client_ip_no_xff() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert_eq!(extract_client_ip(&req), "unknown");
    }

    #[test]
    fn test_extract_client_ip_single() {
        let req = Request::builder()
            .header("x-forwarded-for", "1.2.3.4")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_client_ip(&req), "1.2.3.4");
    }

    #[test]
    fn test_extract_client_ip_first_of_many() {
        let req = Request::builder()
            .header("x-forwarded-for", "1.2.3.4, 5.6.7.8")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_client_ip(&req), "1.2.3.4");
    }

    #[test]
    fn test_too_many_response() {
        let req = Request::builder()
            .header(ACCEPT_LANGUAGE, "en")
            .body(Body::empty())
            .unwrap();
        let (status, json) = too_many_response(&req, 42);
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(json.0["error"], "rate limit exceeded, retry after 42s");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_redis_rate_limiter_against_redis() {
        let cache = crate::cache::Cache::new(crate::cache::create_pool(
            "redis://localhost:6379",
            2,
        ));
        let rl = RedisRateLimiter::new(cache);
        let key = format!("unit-test-{}", Uuid::new_v4());
        for _ in 0..3 {
            let r = rl
                .check("test", &key, 3, Duration::from_secs(60))
                .await
                .unwrap();
            assert!(r.allowed);
        }
        let r = rl
            .check("test", &key, 3, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!r.allowed);
        assert!(r.retry_after > 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_rate_limit_redis_down_fails_open() {
        let state = rate_state_dead_redis(1).await;
        let app = Router::new()
            .route("/", axum::routing::get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), rate_limit_auth))
            .with_state(state);
        let (first, second) = hit_twice(app, &unique_xff()).await;
        assert_eq!(first, StatusCode::OK);
        assert_eq!(second, StatusCode::OK);
    }

    async fn rate_state(max: u32) -> Arc<AppState<sqlx::Sqlite>> {
        use pichost_core::StorageRouter;
        let mut cfg = pichost_core::config::AppConfig::default();
        cfg.rate_limit.auth_max = max;
        cfg.rate_limit.upload_max = max;
        cfg.rate_limit.general_max = max;
        cfg.rate_limit.public_max = max;
        let cache_pool = crate::cache::create_pool("redis://localhost:6379", 2);
        Arc::new(AppState {
            pool: crate::db::create_sqlite_pool("sqlite::memory:", 1)
                .await
                .unwrap(),
            cache: Arc::new(crate::cache::Cache::new(cache_pool.clone())),
            blacklist: Arc::new(crate::middleware::auth::RedisBlacklist::new(
                crate::cache::Cache::new(cache_pool.clone()),
            )),
            rate_limiter: Arc::new(RedisRateLimiter::new(crate::cache::Cache::new(
                cache_pool,
            ))),
            config: Arc::new(cfg),
            router: Arc::new(StorageRouter::new(
                std::collections::HashMap::new(),
                "local".into(),
            )),
        })
    }

    async fn rate_state_dead_redis(max: u32) -> Arc<AppState<sqlx::Sqlite>> {
        use pichost_core::StorageRouter;
        let mut cfg = pichost_core::config::AppConfig::default();
        cfg.rate_limit.auth_max = max;
        let cache_pool = crate::cache::create_pool("redis://127.0.0.1:1", 1);
        Arc::new(AppState {
            pool: crate::db::create_sqlite_pool("sqlite::memory:", 1)
                .await
                .unwrap(),
            cache: Arc::new(crate::cache::Cache::new(cache_pool.clone())),
            blacklist: Arc::new(crate::middleware::auth::RedisBlacklist::new(
                crate::cache::Cache::new(cache_pool.clone()),
            )),
            rate_limiter: Arc::new(RedisRateLimiter::new(crate::cache::Cache::new(
                cache_pool,
            ))),
            config: Arc::new(cfg),
            router: Arc::new(StorageRouter::new(
                std::collections::HashMap::new(),
                "local".into(),
            )),
        })
    }

    fn unique_xff() -> String {
        let uuid = Uuid::new_v4();
        let b = uuid.as_bytes();
        format!("10.{}.{}.{}", b[0], b[1], b[2])
    }

    async fn hit_twice(app: Router, xff: &str) -> (StatusCode, StatusCode) {
        let req = || {
            Request::builder()
                .uri("/")
                .header("x-forwarded-for", xff)
                .body(Body::empty())
                .unwrap()
        };
        let first = app.clone().oneshot(req()).await.unwrap().status();
        let second = app.clone().oneshot(req()).await.unwrap().status();
        (first, second)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_rate_limit_auth_middleware_429() {
        let state = rate_state(1).await;
        let app = Router::new()
            .route("/", axum::routing::get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), rate_limit_auth))
            .with_state(state);
        let (first, second) = hit_twice(app, &unique_xff()).await;
        assert_eq!(first, StatusCode::OK);
        assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_rate_limit_upload_middleware_429() {
        let state = rate_state(1).await;
        let app = Router::new()
            .route("/", axum::routing::get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), rate_limit_upload))
            .with_state(state);
        let (first, second) = hit_twice(app, &unique_xff()).await;
        assert_eq!(first, StatusCode::OK);
        assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_rate_limit_general_middleware_429() {
        let state = rate_state(1).await;
        let app = Router::new()
            .route("/", axum::routing::get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), rate_limit_general))
            .with_state(state);
        let (first, second) = hit_twice(app, &unique_xff()).await;
        assert_eq!(first, StatusCode::OK);
        assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_rate_limit_public_middleware_429() {
        let state = rate_state(1).await;
        let app = Router::new()
            .route("/", axum::routing::get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), rate_limit_public))
            .with_state(state);
        let (first, second) = hit_twice(app, &unique_xff()).await;
        assert_eq!(first, StatusCode::OK);
        assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
    }
}
