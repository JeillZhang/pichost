use std::sync::Arc;
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

async fn check_rate_limit(
    cache: &crate::cache::Cache,
    policy: &str,
    key: &str,
    max_requests: u32,
    window_secs: u64,
) -> Result<u32, u64> {
    let redis_key = rl_key(policy, key);
    match cache.incr(&redis_key, window_secs).await {
        Ok(count) => {
            if count as u32 > max_requests {
                let mut conn = match cache.get_pool().get().await {
                    Ok(c) => c,
                    Err(_) => return Err(window_secs),
                };
                let ttl: u64 = deadpool_redis::redis::cmd("TTL")
                    .arg(&redis_key)
                    .query_async(&mut *conn)
                    .await
                    .unwrap_or(window_secs);
                Err(ttl)
            } else {
                Ok(max_requests - count as u32)
            }
        }
        Err(e) => {
            tracing::warn!("Rate limit Redis error: {e}");
            Ok(max_requests)
        }
    }
}

pub async fn rate_limit_auth<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let ip = extract_client_ip(&req);
    let max = state.config.rate_limit.auth_max;
    match check_rate_limit(&state.cache, "auth", &ip, max, RATE_LIMIT_WINDOW_SECS).await {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => {
            tracing::warn!(ip = %ip, "auth rate limited");
            Err(too_many_response(&req, retry_after))
        }
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
    match check_rate_limit(&state.cache, "upload", &key, max, RATE_LIMIT_WINDOW_SECS).await {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => {
            tracing::warn!(key = %key, "upload rate limited");
            Err(too_many_response(&req, retry_after))
        }
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
    match check_rate_limit(&state.cache, "general", &key, max, RATE_LIMIT_WINDOW_SECS).await {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => {
            tracing::warn!(key = %key, "general rate limited");
            Err(too_many_response(&req, retry_after))
        }
    }
}

pub async fn rate_limit_public<DB: DbType>(
    State(state): State<Arc<AppState<DB>>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let ip = extract_client_ip(&req);
    let max = state.config.rate_limit.public_max;
    match check_rate_limit(&state.cache, "public", &ip, max, RATE_LIMIT_WINDOW_SECS).await {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => {
            tracing::warn!(ip = %ip, "public rate limited");
            Err(too_many_response(&req, retry_after))
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
    async fn test_check_rate_limit_against_redis() {
        let pool = crate::cache::create_pool("redis://localhost:6379", 2);
        let cache = crate::cache::Cache::new(pool);
        let key = format!("unit-test-{}", Uuid::new_v4());
        for _ in 0..3 {
            assert!(check_rate_limit(&cache, "test", &key, 3, 60).await.is_ok());
        }
        let err = check_rate_limit(&cache, "test", &key, 3, 60).await.unwrap_err();
        assert!(err > 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_check_rate_limit_redis_down_opens() {
        let cache = crate::cache::Cache::new(crate::cache::create_pool(
            "redis://127.0.0.1:1",
            1,
        ));
        let res = check_rate_limit(&cache, "auth", "1.2.3.4", 10, 60).await;
        assert_eq!(res, Ok(10));
    }

    async fn rate_state(max: u32) -> Arc<AppState<sqlx::Sqlite>> {
        use pichost_core::StorageRouter;
        let mut cfg = pichost_core::config::AppConfig::default();
        cfg.rate_limit.auth_max = max;
        cfg.rate_limit.upload_max = max;
        cfg.rate_limit.general_max = max;
        cfg.rate_limit.public_max = max;
        Arc::new(AppState {
            pool: crate::db::create_sqlite_pool("sqlite::memory:", 1)
                .await
                .unwrap(),
            cache: Arc::new(crate::cache::Cache::new(crate::cache::create_pool(
                "redis://localhost:6379",
                2,
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
