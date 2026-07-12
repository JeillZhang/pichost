use deadpool_redis::redis::AsyncCommands;
use deadpool_redis::{Config, Pool, Runtime};

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

    pub async fn get(&self, key: &str) -> Result<Option<String>, deadpool_redis::redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?;
        c.get(key).await
    }

    pub async fn set(&self, key: &str, val: &str) -> Result<(), deadpool_redis::redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?;
        c.set(key, val).await
    }

    pub async fn set_ex(&self, key: &str, val: &str, ttl: u64) -> Result<(), deadpool_redis::redis::RedisError> {
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
    pub async fn incr(&self, key: &str, ttl: u64) -> Result<u64, deadpool_redis::redis::RedisError> {
        let mut conn = self.pool.get().await.map_err(pool_err)?;
        let mut pipe = deadpool_redis::redis::pipe();
        pipe.cmd("INCR").arg(key).ignore()
            .cmd("EXPIRE").arg(key).arg(ttl as usize).ignore();
        pipe.query_async::<_, ()>(&mut *conn).await?;
        let count: u64 = deadpool_redis::redis::cmd("GET").arg(key)
            .query_async(&mut *conn).await?;
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
            .cmd("HINCRBY").arg(&key).arg(field).arg(delta).ignore()
            .cmd("EXPIRE").arg(&key).arg(300usize).ignore()
            .query_async::<_, ()>(&mut *conn).await?;

        Ok(())
    }

    /// Get all stats for a user as a map of field → value strings.
    pub async fn get_user_stats(
        &self,
        user_id: &uuid::Uuid,
    ) -> Result<Option<std::collections::HashMap<String, String>>, deadpool_redis::redis::RedisError> {
        let key = format!("pichost:stats:{}", user_id);
        let mut conn = self.pool.get().await.map_err(pool_err)?;
        conn.hgetall(&key).await
    }
}

fn pool_err(e: deadpool_redis::PoolError) -> deadpool_redis::redis::RedisError {
    deadpool_redis::redis::RedisError::from(std::io::Error::other(e.to_string()))
}
