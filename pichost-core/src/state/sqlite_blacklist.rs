use crate::state::{Blacklist, BlacklistError};
use sqlx::SqlitePool;
use std::time::Duration;

pub struct SqliteBlacklist {
    pool: SqlitePool,
}

impl SqliteBlacklist {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl Blacklist for SqliteBlacklist {
    async fn check(&self, jti: &str) -> Result<bool, BlacklistError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM token_blacklist \
             WHERE jti = ? AND expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(jti)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| BlacklistError::Other(e.to_string()))?;
        Ok(n > 0)
    }

    async fn revoke(&self, jti: &str, ttl: Duration) -> Result<(), BlacklistError> {
        let expires = (chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default())
            .format("%Y-%m-%dT%H:%M:%fZ")
            .to_string();
        sqlx::query(
            "INSERT INTO token_blacklist (jti, expires_at) VALUES (?, ?) \
             ON CONFLICT(jti) DO UPDATE SET expires_at = excluded.expires_at",
        )
        .bind(jti)
        .bind(expires)
        .execute(&self.pool)
        .await
        .map_err(|e| BlacklistError::Other(e.to_string()))?;
        Ok(())
    }
}
