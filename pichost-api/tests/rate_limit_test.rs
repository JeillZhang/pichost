use pichost_api::cache::{self, Cache};
use pichost_api::middleware::rate_limit::RedisRateLimiter;
use pichost_core::state::RateLimiter;
use std::time::Duration;
use uuid::Uuid;

fn test_cache() -> Cache {
    let url =
        std::env::var("PICHOST_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    Cache::new(cache::create_pool(&url, 5))
}

/// Same key within one window: first `limit` checks are allowed, the next is
/// denied with a positive retry-after.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn redis_rate_limiter_window() {
    let rl = RedisRateLimiter::new(test_cache());
    let key = format!("rate-test-{}", Uuid::new_v4().simple());
    let r1 = rl
        .check("auth", &key, 2, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(r1.allowed);
    let r2 = rl
        .check("auth", &key, 2, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(r2.allowed);
    let r3 = rl
        .check("auth", &key, 2, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(!r3.allowed);
    assert!(r3.retry_after > 0);
}
