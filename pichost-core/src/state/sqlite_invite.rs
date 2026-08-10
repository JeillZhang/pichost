use crate::state::{InviteCodeInfo, InviteError, InviteStore, InviteVerifyStatus};
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct SqliteInviteStore {
    pool: SqlitePool,
}

impl SqliteInviteStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl InviteStore for SqliteInviteStore {
    async fn create(&self, code: &str, created_by: Uuid, ttl_secs: u64) -> Result<(), InviteError> {
        let expires = (chrono::Utc::now() + chrono::Duration::seconds(ttl_secs as i64))
            .format("%Y-%m-%dT%H:%M:%fZ")
            .to_string();
        sqlx::query("INSERT INTO invite_codes (code, created_by, expires_at) VALUES (?, ?, ?)")
            .bind(code)
            .bind(created_by.to_string())
            .bind(expires)
            .execute(&self.pool)
            .await
            .map_err(|e| InviteError::Other(e.to_string()))?;
        Ok(())
    }

    async fn verify(&self, code: &str) -> Result<bool, InviteError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM invite_codes WHERE code = ? AND used_by IS NULL \
             AND (expires_at IS NULL OR expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        )
        .bind(code)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| InviteError::Other(e.to_string()))?;
        Ok(n > 0)
    }

    async fn consume(&self, code: &str, used_by: Uuid) -> Result<(), InviteError> {
        sqlx::query("UPDATE invite_codes SET used_by = ? WHERE code = ? AND used_by IS NULL")
            .bind(used_by.to_string())
            .bind(code)
            .execute(&self.pool)
            .await
            .map_err(|e| InviteError::Other(e.to_string()))?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<InviteCodeInfo>, InviteError> {
        type Row = (
            String,
            Option<String>,
            chrono::DateTime<chrono::Utc>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<String>,
        );
        sqlx::query_as::<_, Row>(
            "SELECT code, created_by, created_at, expires_at, used_by \
             FROM invite_codes ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(
                    |(code, created_by, created_at, expires_at, used_by)| InviteCodeInfo {
                        code,
                        created_by: created_by
                            .and_then(|s| Uuid::parse_str(&s).ok())
                            .filter(|u| !u.is_nil()),
                        created_at,
                        expires_at,
                        used_by: used_by.and_then(|s| Uuid::parse_str(&s).ok()),
                    },
                )
                .collect()
        })
        .map_err(|e| InviteError::Other(e.to_string()))
    }

    async fn verify_detail(&self, code: &str) -> Result<InviteVerifyStatus, InviteError> {
        let row: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT used_by, expires_at FROM invite_codes WHERE code = ?")
                .bind(code)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| InviteError::Other(e.to_string()))?;
        let (used_by, expires_at) = match row {
            None => return Ok(InviteVerifyStatus::NotFound),
            Some(r) => r,
        };
        if used_by.is_some() {
            return Ok(InviteVerifyStatus::Used);
        }
        if let Some(expires) = expires_at {
            let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%fZ").to_string();
            if expires <= now {
                return Ok(InviteVerifyStatus::Expired);
            }
        }
        Ok(InviteVerifyStatus::Valid)
    }
}
