use crate::state::{Cache, CacheError};
use std::collections::HashMap;
use uuid::Uuid;

/// No-op cache: every read misses, every write is a no-op, and counters
/// always report 1 (rate-limiter fail-open semantics).
pub struct NoopCache;

#[async_trait::async_trait]
impl Cache for NoopCache {
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
        Ok(1)
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
    ) -> Result<Option<HashMap<String, String>>, CacheError> {
        Ok(None)
    }
}
