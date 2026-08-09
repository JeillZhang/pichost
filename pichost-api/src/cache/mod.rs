use std::collections::HashMap;

use deadpool_redis::redis::AsyncCommands;
use deadpool_redis::{Config, Pool, Runtime};
use pichost_core::state::{CacheError, InviteError};
use serde::Serialize;
use uuid::Uuid;

pub type CachePool = Pool;

pub fn create_pool(url: &str, pool_size: usize) -> CachePool {
    let mut cfg = Config::from_url(url);
    cfg.pool = Some(deadpool_redis::PoolConfig::new(pool_size));
    cfg.create_pool(Some(Runtime::Tokio1)).unwrap()
}

pub struct Cache {
    pool: CachePool,
}

impl Cache {
    pub fn new(pool: CachePool) -> Self {
        Self { pool }
    }

    pub fn get_pool(&self) -> CachePool {
        self.pool.clone()
    }

    pub async fn get(
        &self,
        key: &str,
    ) -> Result<Option<String>, deadpool_redis::redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?;
        c.get(key).await
    }

    pub async fn set(&self, key: &str, val: &str) -> Result<(), deadpool_redis::redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?;
        c.set(key, val).await
    }

    pub async fn set_ex(
        &self,
        key: &str,
        val: &str,
        ttl: u64,
    ) -> Result<(), deadpool_redis::redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?;
        c.set_ex(key, val, ttl).await
    }

    pub async fn del(&self, key: &str) -> Result<(), deadpool_redis::redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?;
        c.del(key).await
    }

    pub async fn exists(&self, key: &str) -> Result<bool, deadpool_redis::redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?;
        c.exists(key).await
    }

    /// Atomically increment a counter and set TTL on first creation.
    /// Returns the new count after increment.
    pub async fn incr(
        &self,
        key: &str,
        ttl: u64,
    ) -> Result<u64, deadpool_redis::redis::RedisError> {
        let mut conn = self.pool.get().await.map_err(pool_err)?;
        let mut pipe = deadpool_redis::redis::pipe();
        pipe.cmd("INCR")
            .arg(key)
            .ignore()
            .cmd("EXPIRE")
            .arg(key)
            .arg(ttl as usize)
            .ignore();
        pipe.query_async::<_, ()>(&mut *conn).await?;
        let count: u64 = deadpool_redis::redis::cmd("GET")
            .arg(key)
            .query_async(&mut *conn)
            .await?;
        Ok(count)
    }

    // ── Metadata Cache (cache-aside, JSON, TTL 600s) ──

    /// Fetch from metadata cache, or populate via `fetch_fn` on miss.
    /// Generic over any Serde-compatible type.
    pub async fn cached_meta<T, F, E>(
        &self,
        image_id: &uuid::Uuid,
        ttl: u64,
        fetch_fn: F,
    ) -> Result<T, E>
    where
        T: serde::de::DeserializeOwned + serde::Serialize,
        F: std::future::Future<Output = Result<T, E>>,
    {
        let key = format!("pichost:meta:{}", image_id);

        // Try cache hit
        if let Ok(Some(json)) = self.get(&key).await {
            if let Ok(val) = serde_json::from_str::<T>(&json) {
                return Ok(val);
            }
        }

        // Cache miss — fetch from source
        let val = fetch_fn.await?;

        // Store in cache (best-effort)
        if let Ok(json) = serde_json::to_string(&val) {
            let _ = self.set_ex(&key, &json, ttl).await;
        }

        Ok(val)
    }

    // ── Thumbnail/Blob Cache (raw bytes, TTL 3600s) ──

    /// Fetch from thumbnail/blob cache, or populate via `fetch_fn` on miss.
    /// Returns raw bytes. Uses Redis String storage (safe for < 512MB values).
    pub async fn cached_thumb<F, E>(
        &self,
        cache_key: &str,
        ttl: u64,
        fetch_fn: F,
    ) -> Result<Vec<u8>, E>
    where
        F: std::future::Future<Output = Result<Vec<u8>, E>>,
    {
        let redis_key = format!("pichost:thumb:{}", cache_key);

        // Try cache hit — read raw bytes directly (not via self.get which does String)
        let mut conn = match self.pool.get().await {
            Ok(c) => c,
            Err(_) => return fetch_fn.await,
        };

        let cached: Option<Vec<u8>> = deadpool_redis::redis::cmd("GET")
            .arg(&redis_key)
            .query_async(&mut *conn)
            .await
            .unwrap_or(None);

        if let Some(bytes) = cached {
            return Ok(bytes);
        }

        // Cache miss — fetch from source
        let bytes = fetch_fn.await?;

        // Store (best-effort)
        let _: Result<(), _> = deadpool_redis::redis::cmd("SETEX")
            .arg(&redis_key)
            .arg(ttl as usize)
            .arg(&bytes)
            .query_async(&mut *conn)
            .await;

        Ok(bytes)
    }

    // ── User Stats Cache (Hash counters, TTL 300s) ──

    /// Increment a user stat field and set TTL on first creation.
    pub async fn incr_user_stat(
        &self,
        user_id: &uuid::Uuid,
        field: &str,
        delta: i64,
    ) -> Result<(), deadpool_redis::redis::RedisError> {
        let key = format!("pichost:stats:{}", user_id);
        let mut conn = self.pool.get().await.map_err(pool_err)?;

        deadpool_redis::redis::pipe()
            .cmd("HINCRBY")
            .arg(&key)
            .arg(field)
            .arg(delta)
            .ignore()
            .cmd("EXPIRE")
            .arg(&key)
            .arg(300usize)
            .ignore()
            .query_async::<_, ()>(&mut *conn)
            .await?;

        Ok(())
    }

    /// Overwrite user stat fields and refresh the TTL (HSET-based).
    /// Safe for re-populating the cache from DB — unlike `incr_user_stat`,
    /// repeated calls cannot double-count values.
    /// `None` values are stored as an empty string (and read back as None).
    pub async fn set_user_stats(
        &self,
        user_id: &uuid::Uuid,
        fields: &[(&str, Option<i64>)],
    ) -> Result<(), deadpool_redis::redis::RedisError> {
        let key = format!("pichost:stats:{}", user_id);
        let mut conn = self.pool.get().await.map_err(pool_err)?;

        let mut pipe = deadpool_redis::redis::pipe();
        for (field, value) in fields {
            match value {
                Some(v) => pipe.cmd("HSET").arg(&key).arg(field).arg(v).ignore(),
                None => pipe.cmd("HSET").arg(&key).arg(field).arg("").ignore(),
            };
        }
        pipe.cmd("EXPIRE").arg(&key).arg(300usize).ignore();
        pipe.query_async::<_, ()>(&mut *conn).await?;

        Ok(())
    }

    /// Get all stats for a user as a map of field → value strings.
    pub async fn get_user_stats(
        &self,
        user_id: &uuid::Uuid,
    ) -> Result<Option<std::collections::HashMap<String, String>>, deadpool_redis::redis::RedisError>
    {
        let key = format!("pichost:stats:{}", user_id);
        let mut conn = self.pool.get().await.map_err(pool_err)?;
        conn.hgetall(&key).await
    }

    // ── Invite Code Methods ──

    pub async fn create_invite_code(
        &self,
        admin_id: &Uuid,
        ttl: u64,
    ) -> Result<String, deadpool_redis::redis::RedisError> {
        let code = Uuid::new_v4().to_string().replace('-', "");
        self.write_invite_code(&code, admin_id, ttl).await?;
        Ok(code)
    }

    async fn write_invite_code(
        &self,
        code: &str,
        admin_id: &Uuid,
        ttl: u64,
    ) -> Result<(), deadpool_redis::redis::RedisError> {
        let key = format!("pichost:invite:{}", code);
        let now = chrono::Utc::now().timestamp();
        let expires_at = now + ttl as i64;

        let mut conn = self.pool.get().await.map_err(pool_err)?;
        deadpool_redis::redis::pipe()
            .cmd("HSET")
            .arg(&key)
            .arg("created_by")
            .arg(admin_id.to_string())
            .ignore()
            .cmd("HSET")
            .arg(&key)
            .arg("created_at")
            .arg(now.to_string())
            .ignore()
            .cmd("HSET")
            .arg(&key)
            .arg("expires_at")
            .arg(expires_at.to_string())
            .ignore()
            .cmd("HSET")
            .arg(&key)
            .arg("used_by")
            .arg("")
            .ignore()
            .cmd("EXPIRE")
            .arg(&key)
            .arg(ttl as usize)
            .ignore()
            .cmd("SADD")
            .arg("pichost:invites")
            .arg(code)
            .ignore()
            .query_async::<_, ()>(&mut *conn)
            .await?;

        Ok(())
    }

    pub async fn verify_invite_code(
        &self,
        code: &str,
    ) -> Result<InviteVerifyResult, deadpool_redis::redis::RedisError> {
        let key = format!("pichost:invite:{}", code);
        let mut conn = self.pool.get().await.map_err(pool_err)?;

        let exists: bool = conn.exists(&key).await?;
        if !exists {
            return Ok(InviteVerifyResult::NotFound);
        }

        let fields: std::collections::HashMap<String, String> = conn.hgetall(&key).await?;

        if let Some(used_by) = fields.get("used_by") {
            if !used_by.is_empty() {
                return Ok(InviteVerifyResult::Used);
            }
        }

        if let Some(expires_at_str) = fields.get("expires_at") {
            if let Ok(expires_at) = expires_at_str.parse::<i64>() {
                let now = chrono::Utc::now().timestamp();
                if now > expires_at {
                    return Ok(InviteVerifyResult::Expired);
                }
            }
        }

        Ok(InviteVerifyResult::Valid)
    }

    pub async fn consume_invite_code(
        &self,
        code: &str,
        user_id: &Uuid,
    ) -> Result<bool, deadpool_redis::redis::RedisError> {
        let key = format!("pichost:invite:{}", code);
        let mut conn = self.pool.get().await.map_err(pool_err)?;

        let exists: bool = conn.exists(&key).await?;
        if !exists {
            return Ok(false);
        }

        deadpool_redis::redis::pipe()
            .cmd("HSET")
            .arg(&key)
            .arg("used_by")
            .arg(user_id.to_string())
            .ignore()
            .cmd("SREM")
            .arg("pichost:invites")
            .arg(code)
            .ignore()
            .query_async::<_, ()>(&mut *conn)
            .await?;

        Ok(true)
    }

    pub async fn list_invite_codes(
        &self,
    ) -> Result<Vec<InviteCodeInfo>, deadpool_redis::redis::RedisError> {
        let mut conn = self.pool.get().await.map_err(pool_err)?;

        let members: Vec<String> = deadpool_redis::redis::cmd("SMEMBERS")
            .arg("pichost:invites")
            .query_async(&mut *conn)
            .await?;

        let mut codes = Vec::new();
        let mut stale = Vec::new();
        let now = chrono::Utc::now().timestamp();

        for code in &members {
            let key = format!("pichost:invite:{}", code);
            let fields: HashMap<String, String> =
                deadpool_redis::redis::cmd("HGETALL")
                    .arg(&key)
                    .query_async(&mut *conn)
                    .await?;

            match parse_invite_fields(code, &fields, now) {
                Some(info) => codes.push(info),
                None => stale.push(code.clone()),
            }
        }

        codes.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        clean_stale_invites(&mut *conn, &stale).await?;

        Ok(codes)
    }
}

#[async_trait::async_trait]
impl pichost_core::state::Cache for Cache {
    async fn get(&self, key: &str) -> Result<Option<String>, CacheError> {
        self.get(key).await.map_err(|e| CacheError::Other(e.to_string()))
    }

    async fn set_ex(&self, key: &str, val: &str, ttl: u64) -> Result<(), CacheError> {
        self.set_ex(key, val, ttl)
            .await
            .map_err(|e| CacheError::Other(e.to_string()))
    }

    async fn del(&self, key: &str) -> Result<(), CacheError> {
        self.del(key).await.map_err(|e| CacheError::Other(e.to_string()))
    }

    async fn exists(&self, key: &str) -> Result<bool, CacheError> {
        self.exists(key)
            .await
            .map_err(|e| CacheError::Other(e.to_string()))
    }

    async fn incr(&self, key: &str, ttl: u64) -> Result<u64, CacheError> {
        self.incr(key, ttl)
            .await
            .map_err(|e| CacheError::Other(e.to_string()))
    }
}

pub struct RedisInviteStore {
    cache: Cache,
}

impl RedisInviteStore {
    pub fn new(cache: Cache) -> Self {
        Self { cache }
    }
}

#[async_trait::async_trait]
impl pichost_core::state::InviteStore for RedisInviteStore {
    async fn create(&self, code: &str, created_by: Uuid, ttl_secs: u64) -> Result<(), InviteError> {
        self.cache
            .write_invite_code(code, &created_by, ttl_secs)
            .await
            .map_err(|e| InviteError::Other(e.to_string()))
    }

    async fn verify(&self, code: &str) -> Result<bool, InviteError> {
        self.cache
            .verify_invite_code(code)
            .await
            .map(|r| r == InviteVerifyResult::Valid)
            .map_err(|e| InviteError::Other(e.to_string()))
    }

    async fn consume(&self, code: &str, used_by: Uuid) -> Result<(), InviteError> {
        let consumed = self
            .cache
            .consume_invite_code(code, &used_by)
            .await
            .map_err(|e| InviteError::Other(e.to_string()))?;
        if consumed {
            Ok(())
        } else {
            Err(InviteError::Other(format!("invite code not found: {code}")))
        }
    }

    async fn list(&self) -> Result<Vec<pichost_core::state::InviteCodeInfo>, InviteError> {
        let codes = self
            .cache
            .list_invite_codes()
            .await
            .map_err(|e| InviteError::Other(e.to_string()))?;
        Ok(codes.into_iter().map(to_core_invite_info).collect())
    }
}

fn to_core_invite_info(info: InviteCodeInfo) -> pichost_core::state::InviteCodeInfo {
    pichost_core::state::InviteCodeInfo {
        code: info.code,
        created_by: (info.created_by != Uuid::nil()).then_some(info.created_by),
        created_at: chrono::DateTime::from_timestamp(info.created_at, 0).unwrap_or_default(),
        expires_at: chrono::DateTime::from_timestamp(info.expires_at, 0)
            .filter(|_| info.expires_at > 0),
        used_by: info.used_by,
    }
}

fn parse_invite_fields(
    code: &str,
    fields: &HashMap<String, String>,
    now: i64,
) -> Option<InviteCodeInfo> {
    if fields.is_empty() {
        return None;
    }
    let used_by = fields
        .get("used_by")
        .and_then(|v| if v.is_empty() { None } else { Uuid::parse_str(v).ok() });
    let expires_at = fields
        .get("expires_at")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    if now > expires_at || used_by.is_some() {
        return None;
    }
    let created_at = fields
        .get("created_at")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let created_by = fields
        .get("created_by")
        .and_then(|v| Uuid::parse_str(v).ok())
        .unwrap_or(Uuid::nil());
    Some(InviteCodeInfo { code: code.to_string(), created_by, expires_at, used_by, created_at })
}

async fn clean_stale_invites<C: deadpool_redis::redis::aio::ConnectionLike>(
    conn: &mut C,
    stale: &[String],
) -> Result<(), deadpool_redis::redis::RedisError> {
    if stale.is_empty() {
        return Ok(());
    }
    let mut pipe = deadpool_redis::redis::pipe();
    for s in stale {
        pipe.cmd("SREM").arg("pichost:invites").arg(s).ignore();
    }
    pipe.query_async::<_, ()>(conn).await?;
    Ok(())
}

// ── Invite Code Types ──

#[derive(Debug, PartialEq)]
pub enum InviteVerifyResult {
    Valid,
    Used,
    Expired,
    NotFound,
}

#[derive(Debug, Serialize)]
pub struct InviteCodeInfo {
    pub code: String,
    pub created_by: Uuid,
    pub expires_at: i64,
    pub used_by: Option<Uuid>,
    pub created_at: i64,
}

fn pool_err(e: deadpool_redis::PoolError) -> deadpool_redis::redis::RedisError {
    deadpool_redis::redis::RedisError::from(std::io::Error::other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cache() -> Cache {
        let url = std::env::var("PICHOST_REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        Cache::new(create_pool(&url, 5))
    }

    fn unique_code() -> String {
        format!("t{}", Uuid::new_v4().simple())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_ops_fail_with_unreachable_redis() {
        let cache = Cache::new(create_pool("redis://127.0.0.1:1", 1));
        assert!(cache.get("k").await.is_err());
        assert!(cache.set("k", "v").await.is_err());
        assert!(cache.set_ex("k", "v", 60).await.is_err());
        assert!(cache.del("k").await.is_err());
        assert!(cache.exists("k").await.is_err());
        assert!(cache.incr("k", 60).await.is_err());
        assert!(cache.incr_user_stat(&Uuid::new_v4(), "f", 1).await.is_err());
        assert!(cache.set_user_stats(&Uuid::new_v4(), &[("f", Some(1))]).await.is_err());
        assert!(cache.get_user_stats(&Uuid::new_v4()).await.is_err());
        assert!(cache.create_invite_code(&Uuid::new_v4(), 60).await.is_err());
        assert!(cache.verify_invite_code("x").await.is_err());
        assert!(cache.consume_invite_code("x", &Uuid::new_v4()).await.is_err());
        assert!(cache.list_invite_codes().await.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_cached_fallbacks_with_unreachable_redis() {
        let cache = Cache::new(create_pool("redis://127.0.0.1:1", 1));
        let meta: Result<(i32,), std::io::Error> =
            cache.cached_meta(&Uuid::new_v4(), 60, async { Ok((7,)) }).await;
        assert_eq!(meta.unwrap().0, 7);
        let thumb: Result<Vec<u8>, std::io::Error> =
            cache.cached_thumb("k", 60, async { Ok(vec![1u8, 2]) }).await;
        assert_eq!(thumb.unwrap(), vec![1u8, 2]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_list_invite_codes_cleans_stale_expired() {
        let cache = test_cache();
        let code = cache.create_invite_code(&Uuid::new_v4(), 60).await.unwrap();
        let key = format!("pichost:invite:{code}");
        let mut conn = cache.get_pool().get().await.unwrap();
        deadpool_redis::redis::cmd("HSET")
            .arg(&key)
            .arg("expires_at")
            .arg((chrono::Utc::now().timestamp() - 100).to_string())
            .query_async::<_, ()>(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        let codes = cache.list_invite_codes().await.unwrap();
        assert!(!codes.iter().any(|c| c.code == code));
        let mut conn = cache.get_pool().get().await.unwrap();
        let members: Vec<String> = deadpool_redis::redis::cmd("SMEMBERS")
            .arg("pichost:invites")
            .query_async(&mut *conn)
            .await
            .unwrap();
        assert!(!members.contains(&code));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_list_invite_codes_cleans_used_without_srem() {
        let cache = test_cache();
        let code = cache.create_invite_code(&Uuid::new_v4(), 60).await.unwrap();
        let key = format!("pichost:invite:{code}");
        let mut conn = cache.get_pool().get().await.unwrap();
        deadpool_redis::redis::cmd("HSET")
            .arg(&key)
            .arg("used_by")
            .arg(Uuid::new_v4().to_string())
            .query_async::<_, ()>(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        let codes = cache.list_invite_codes().await.unwrap();
        assert!(!codes.iter().any(|c| c.code == code));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_parse_invite_fields_empty() {
        assert!(parse_invite_fields(&unique_code(), &HashMap::new(), 0).is_none());
        let mut fields = HashMap::new();
        fields.insert("created_at".to_string(), "garbage".to_string());
        fields.insert("created_by".to_string(), "garbage".to_string());
        fields.insert("expires_at".to_string(), "garbage".to_string());
        assert!(parse_invite_fields(&unique_code(), &fields, 1).is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_parse_invite_fields_valid_graceful_defaults() {
        let code = unique_code();
        let mut fields = HashMap::new();
        fields.insert("expires_at".to_string(), (chrono::Utc::now().timestamp() + 3600).to_string());
        fields.insert("created_at".to_string(), "garbage".to_string());
        fields.insert("created_by".to_string(), "garbage".to_string());
        let info = parse_invite_fields(&code, &fields, 0).expect("parsed");
        assert_eq!(info.code, code);
        assert_eq!(info.created_at, 0);
        assert_eq!(info.created_by, Uuid::nil());
        assert_eq!(info.used_by, None);
    }
}
