use crate::state::{RateLimitResult, RateLimiter, RateLimiterError};
use sqlx::SqlitePool;
use std::time::Duration;

pub struct SqliteRateLimiter {
    pool: SqlitePool,
}

impl SqliteRateLimiter {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl RateLimiter for SqliteRateLimiter {
    async fn check(
        &self,
        policy: &str,
        key: &str,
        limit: u32,
        window: Duration,
    ) -> Result<RateLimitResult, RateLimiterError> {
        let window_start = chrono::Utc::now().timestamp() / window.as_secs() as i64;
        let count: i64 = sqlx::query_scalar(
            "INSERT INTO rate_limits (policy, key, window_start, count) VALUES (?, ?, ?, 1) \
             ON CONFLICT(policy, key, window_start) DO UPDATE SET count = count + 1 \
             RETURNING count",
        )
        .bind(policy)
        .bind(key)
        .bind(window_start)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RateLimiterError::Other(e.to_string()))?;
        if count <= limit as i64 {
            Ok(RateLimitResult {
                allowed: true,
                retry_after: 0,
            })
        } else {
            Ok(RateLimitResult {
                allowed: false,
                retry_after: window.as_secs(),
            })
        }
    }
}
