//! State-layer traits shared by Redis (T17-T20) and SQLite (T22-T24) backends.
//!
//! Each trait is object-safe (`Send + Sync`) so it can be held behind
//! `Arc<dyn Trait>` by the API / worker layers. Error types are coarse
//! (`Other(String)`) — backend-specific detail is preserved in the message.

use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NackAction {
    Retry,
    DeadLetter,
}

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("queue error: {0}")]
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub retry_after: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum RateLimiterError {
    #[error("rate limiter error: {0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum BlacklistError {
    #[error("blacklist error: {0}")]
    Other(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InviteCodeInfo {
    pub code: String,
    pub created_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub used_by: Option<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum InviteError {
    #[error("invite error: {0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache error: {0}")]
    Other(String),
}

/// Detailed result of an invite-code verification, preserving the
/// Used/Expired/NotFound distinction (the coarse `verify` collapses these).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteVerifyStatus {
    Valid,
    Used,
    Expired,
    NotFound,
}

#[async_trait::async_trait]
pub trait Queue: Send + Sync {
    async fn enqueue(&self, payload: &crate::models::TaskPayload) -> Result<(), QueueError>;
    async fn dequeue(
        &self,
        timeout: Duration,
    ) -> Result<Option<crate::models::TaskPayload>, QueueError>;
    async fn ack(&self, task_id: Uuid) -> Result<(), QueueError>;
    async fn nack(
        &self,
        task_id: Uuid,
        retry_count: i32,
        max_retries: i32,
    ) -> Result<NackAction, QueueError>;
}

#[async_trait::async_trait]
pub trait Blacklist: Send + Sync {
    async fn check(&self, jti: &str) -> Result<bool, BlacklistError>;
    async fn revoke(&self, jti: &str, ttl: Duration) -> Result<(), BlacklistError>;
}

#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    async fn check(
        &self,
        policy: &str,
        key: &str,
        limit: u32,
        window: Duration,
    ) -> Result<RateLimitResult, RateLimiterError>;
}

#[async_trait::async_trait]
pub trait InviteStore: Send + Sync {
    async fn create(&self, code: &str, created_by: Uuid, ttl_secs: u64) -> Result<(), InviteError>;
    async fn verify(&self, code: &str) -> Result<bool, InviteError>;
    async fn consume(&self, code: &str, used_by: Uuid) -> Result<(), InviteError>;
    async fn list(&self) -> Result<Vec<InviteCodeInfo>, InviteError>;
    /// Detailed verification preserving the Used/Expired/NotFound distinction
    /// so routes can emit distinct user-facing error codes.
    async fn verify_detail(&self, code: &str) -> Result<InviteVerifyStatus, InviteError>;
}

#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<String>, CacheError>;
    async fn set_ex(&self, key: &str, val: &str, ttl: u64) -> Result<(), CacheError>;
    async fn del(&self, key: &str) -> Result<(), CacheError>;
    async fn exists(&self, key: &str) -> Result<bool, CacheError>;
    async fn incr(&self, key: &str, ttl: u64) -> Result<u64, CacheError>;
    /// Raw byte fetch (binary-safe; `get` is String-based and lossy for blobs).
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError>;
    /// Raw byte store with TTL (binary-safe counterpart of `get_bytes`).
    async fn set_ex_bytes(&self, key: &str, val: &[u8], ttl: u64) -> Result<(), CacheError>;
    /// Hash-based user-stat counter (HINCRBY) with TTL on first creation.
    async fn incr_user_stat(
        &self,
        user_id: &Uuid,
        field: &str,
        delta: i64,
    ) -> Result<(), CacheError>;
    /// Overwrite user-stat hash fields and refresh TTL (HSET-based, no double count).
    async fn set_user_stats(
        &self,
        user_id: &Uuid,
        fields: &[(&str, Option<i64>)],
    ) -> Result<(), CacheError>;
    /// Read all user-stat fields as a map (empty map for a missing key).
    async fn get_user_stats(
        &self,
        user_id: &Uuid,
    ) -> Result<Option<std::collections::HashMap<String, String>>, CacheError>;
}

pub mod sqlite_queue;

// 测试用 Mock 实现（state_test.rs 及后续任务测试使用）
pub struct MockQueue;
#[async_trait::async_trait]
impl Queue for MockQueue {
    async fn enqueue(&self, _p: &crate::models::TaskPayload) -> Result<(), QueueError> {
        Ok(())
    }
    async fn dequeue(
        &self,
        _t: Duration,
    ) -> Result<Option<crate::models::TaskPayload>, QueueError> {
        Ok(None)
    }
    async fn ack(&self, _id: Uuid) -> Result<(), QueueError> {
        Ok(())
    }
    async fn nack(&self, _id: Uuid, _r: i32, _m: i32) -> Result<NackAction, QueueError> {
        Ok(NackAction::Retry)
    }
}

pub struct MockBlacklist;
#[async_trait::async_trait]
impl Blacklist for MockBlacklist {
    async fn check(&self, _jti: &str) -> Result<bool, BlacklistError> {
        Ok(false)
    }
    async fn revoke(&self, _jti: &str, _ttl: Duration) -> Result<(), BlacklistError> {
        Ok(())
    }
}

pub struct MockRateLimiter;
#[async_trait::async_trait]
impl RateLimiter for MockRateLimiter {
    async fn check(
        &self,
        _policy: &str,
        _key: &str,
        _limit: u32,
        _window: Duration,
    ) -> Result<RateLimitResult, RateLimiterError> {
        Ok(RateLimitResult {
            allowed: true,
            retry_after: 0,
        })
    }
}

pub struct MockInviteStore;
#[async_trait::async_trait]
impl InviteStore for MockInviteStore {
    async fn create(
        &self,
        _code: &str,
        _created_by: Uuid,
        _ttl_secs: u64,
    ) -> Result<(), InviteError> {
        Ok(())
    }
    async fn verify(&self, _code: &str) -> Result<bool, InviteError> {
        Ok(true)
    }
    async fn consume(&self, _code: &str, _used_by: Uuid) -> Result<(), InviteError> {
        Ok(())
    }
    async fn list(&self) -> Result<Vec<InviteCodeInfo>, InviteError> {
        Ok(Vec::new())
    }
    async fn verify_detail(&self, _code: &str) -> Result<InviteVerifyStatus, InviteError> {
        Ok(InviteVerifyStatus::Valid)
    }
}

pub struct MockCache;
#[async_trait::async_trait]
impl Cache for MockCache {
    async fn get(&self, _key: &str) -> Result<Option<String>, CacheError> {
        Ok(None)
    }
    async fn set_ex(&self, _key: &str, _val: &str, _ttl: u64) -> Result<(), CacheError> {
        Ok(())
    }
    async fn del(&self, _key: &str) -> Result<(), CacheError> {
        Ok(())
    }
    async fn exists(&self, _key: &str) -> Result<bool, CacheError> {
        Ok(false)
    }
    async fn incr(&self, _key: &str, _ttl: u64) -> Result<u64, CacheError> {
        Ok(0)
    }
    async fn get_bytes(&self, _key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        Ok(None)
    }
    async fn set_ex_bytes(&self, _key: &str, _val: &[u8], _ttl: u64) -> Result<(), CacheError> {
        Ok(())
    }
    async fn incr_user_stat(
        &self,
        _user_id: &Uuid,
        _field: &str,
        _delta: i64,
    ) -> Result<(), CacheError> {
        Ok(())
    }
    async fn set_user_stats(
        &self,
        _user_id: &Uuid,
        _fields: &[(&str, Option<i64>)],
    ) -> Result<(), CacheError> {
        Ok(())
    }
    async fn get_user_stats(
        &self,
        _user_id: &Uuid,
    ) -> Result<Option<std::collections::HashMap<String, String>>, CacheError> {
        Ok(None)
    }
}

#[cfg(test)]
mod state_test;
