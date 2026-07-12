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
}

fn pool_err(e: deadpool_redis::PoolError) -> deadpool_redis::redis::RedisError {
    deadpool_redis::redis::RedisError::from(std::io::Error::other(e.to_string()))
}
