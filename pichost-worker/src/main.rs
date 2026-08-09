use std::collections::HashMap;
use std::sync::Arc;

use deadpool_redis::{Config as RedisConfig, Pool as RedisPool, Runtime};
use pichost_core::storage::local::LocalStorage;
use pichost_core::storage::s3::RustfsStorage;
use pichost_core::storage::StorageBackend;
use pichost_core::StorageRouter;
use tokio::task::JoinHandle;

mod config;
mod db;
mod fonts;
mod pipeline;
mod processor;
mod queue;
mod watermark;

/// Bundled state shared across all worker tasks.
struct WorkerState {
    pool: sqlx::PgPool,
    redis: RedisPool,
    config: Arc<pichost_core::config::AppConfig>,
    router: Arc<StorageRouter>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .json()
        .init();

    // 1. Load config
    let app_config = config::load_worker_config()?;
    tracing::info!(
        concurrency = app_config.worker.concurrency,
        "worker starting"
    );

    // 2. Init DB pool
    let pool = db::create_pg_pool(
        &app_config.database.url,
        app_config.database.max_connections,
    )
    .await?;
    db::run_pg_migrations(&pool).await?;
    tracing::info!("database connected, migrations applied");

    // 3. Init Redis pool
    let mut redis_cfg = RedisConfig::from_url(&app_config.redis.url);
    redis_cfg.pool = Some(deadpool_redis::PoolConfig::new(
        app_config.redis.pool_size as usize,
    ));
    let redis_pool = redis_cfg
        .create_pool(Some(Runtime::Tokio1))
        .expect("failed to create Redis pool");

    // 4. Init full worker state (recovery + storage router)
    let state = init_worker_state(pool, redis_pool, Arc::new(app_config)).await?;

    // 5. Spawn workers and wait forever
    let handles = spawn_workers(state);
    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Recover stale tasks and initialise the StorageRouter for all configured backends.
async fn init_worker_state(
    pool: sqlx::PgPool,
    redis: RedisPool,
    config: Arc<pichost_core::config::AppConfig>,
) -> Result<WorkerState, Box<dyn std::error::Error>> {
    // Startup recovery: re-enqueue stale tasks from processing queue
    let recovered =
        queue::recover_stale_tasks(&redis, config.worker.task_timeout).await?;
    if !recovered.is_empty() {
        tracing::info!(count = recovered.len(), "recovered stale tasks");
    }

    // Init StorageRouter
    let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();

    let local = LocalStorage::new(
        config.storage.local_base_path.clone(),
        config.server.public_url.clone(),
    );
    backends.insert("local".into(), Arc::new(local));

    if let Some(rustfs_config) = &config.storage.rustfs {
        let rustfs = RustfsStorage::new(rustfs_config).await;
        tracing::info!(endpoint = %rustfs_config.endpoint, "Rustfs storage initialized");
        backends.insert("rustfs".into(), Arc::new(rustfs));
    }

    let router = Arc::new(StorageRouter::new(
        backends,
        config.storage.default_backend.clone(),
    ));

    Ok(WorkerState {
        pool,
        redis,
        config,
        router,
    })
}

/// Spawn one `worker_loop` task per configured concurrency slot.
fn spawn_workers(state: WorkerState) -> Vec<JoinHandle<()>> {
    let concurrency = state.config.worker.concurrency;
    let mut handles = Vec::with_capacity(concurrency);
    for i in 0..concurrency {
        let pool = state.pool.clone();
        let redis = state.redis.clone();
        let config = state.config.clone();
        let router = state.router.clone();

        let handle = tokio::spawn(async move {
            tracing::info!(worker_id = i, "worker started");
            worker_loop(i, pool, redis, config, router).await;
        });
        handles.push(handle);
    }
    handles
}

async fn worker_loop(
    worker_id: usize,
    pool: sqlx::PgPool,
    redis: RedisPool,
    config: Arc<pichost_core::config::AppConfig>,
    router: Arc<StorageRouter>,
) {
    let timeout = config.worker.queue_poll_timeout;

    loop {
        // Dequeue: block up to `timeout` seconds for a task
        let task = match queue::dequeue_task(&redis, timeout).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                // No task available within timeout — loop again
                continue;
            }
            Err(e) => {
                tracing::error!(worker_id, error = %e, "dequeue failed");
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        let task_id = task.task_id;
        let image_id = task.image_id;
        tracing::info!(worker_id, %task_id, %image_id, "processing task");

        // Process with timeout
        let process_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(config.worker.task_timeout),
            pipeline::process_task(&pool, &router, &config, &task),
        )
        .await;

        handle_task_result(worker_id, &pool, &redis, task, process_result, &config).await;
    }
}

/// Handle the result of a single task processing attempt.
async fn handle_task_result(
    worker_id: usize, pool: &sqlx::PgPool, redis: &RedisPool,
    task: queue::TaskPayload,
    result: Result<Result<(), pipeline::PipelineError>, tokio::time::error::Elapsed>,
    config: &pichost_core::config::AppConfig,
) {
    let task_id = task.task_id;
    match result {
        Ok(Ok(())) => {
            if let Err(e) = queue::ack_task(redis, task_id).await {
                tracing::error!(worker_id, %task_id, error = %e, "ack failed");
            }
            tracing::info!(worker_id, %task_id, "task completed");
        }
        Ok(Err(e)) => {
            tracing::warn!(worker_id, %task_id, error = %e, "task processing failed");
            match queue::nack_task(redis, &task, &e.to_string()).await {
                Ok(queue::NackAction::Retry) => tracing::info!(
                    worker_id, %task_id, retry = task.retry_count + 1, "task retrying"
                ),
                Ok(queue::NackAction::DeadLetter) => {
                    tracing::error!(worker_id, %task_id, "task dead-lettered");
                    handle_dead_letter(
                        pool, task.image_id, task.retry_count + 1,
                        task.max_retries, &e.to_string(),
                    )
                    .await;
                }
                Err(e) => tracing::error!(worker_id, %task_id, error = %e, "nack failed"),
            }
        }
        Err(_elapsed) => {
            tracing::warn!(worker_id, %task_id, "task timed out");
            let timeout_err = format!("timed out after {}s", config.worker.task_timeout);
            let _ = queue::nack_task(redis, &task, &timeout_err).await;
        }
    }
}

/// Persist dead-letter task failure in the database.
async fn handle_dead_letter(
    pool: &sqlx::PgPool,
    image_id: uuid::Uuid,
    retry_count: i32,
    max_retries: i32,
    error: &str,
) {
    let now = chrono::Utc::now();
    let _ = sqlx::query(
        r#"INSERT INTO upload_tasks
           (image_id, task_type, status, error, retry_count, max_retries, completed_at)
           VALUES ($1, 'all', 'failed', $2, $3, $4, $5)"#,
    )
    .bind(image_id)
    .bind(error)
    .bind(retry_count)
    .bind(max_retries)
    .bind(now)
    .execute(pool)
    .await;

    // Mark image as failed
    let _ = sqlx::query("UPDATE images SET status = 'failed' WHERE id = $1")
        .bind(image_id)
        .execute(pool)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpool_redis::redis::AsyncCommands;
    use pichost_core::config::AppConfig;

    static REDIS_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

    const TEST_DB_URL: &str = "postgres://pichost:pichost@localhost:5432/pichost";
    const KEY_DEAD: &str = "pichost:tasks:dead";

    fn test_redis_pool() -> RedisPool {
        RedisConfig::from_url("redis://localhost:6379/2")
            .create_pool(Some(Runtime::Tokio1))
            .unwrap()
    }

    async fn test_pg_pool() -> sqlx::PgPool {
        let pool = db::create_pg_pool(TEST_DB_URL, 4).await.unwrap();
        db::run_pg_migrations(&pool).await.unwrap();
        pool
    }

    fn sample_task() -> queue::TaskPayload {
        queue::TaskPayload {
            task_id: uuid::Uuid::new_v4(),
            image_id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            storage_backend: "local".into(),
            storage_config_id: None,
            storage_backend_name: "Local".into(),
            source_key: "source.png".into(),
            source_mime: "image/png".into(),
            retry_count: 0,
            max_retries: 3,
        }
    }

    fn task_key(task_id: uuid::Uuid) -> String {
        format!("pichost:task:{task_id}")
    }

    async fn enqueue_and_dequeue_own(
        redis: &RedisPool,
        task: &queue::TaskPayload,
    ) -> queue::TaskPayload {
        loop {
            queue::enqueue_task(redis, task).await.unwrap();
            match queue::dequeue_task(redis, 1).await.unwrap() {
                Some(got) if got.task_id == task.task_id => return got,
                Some(got) => {
                    queue::ack_task(redis, got.task_id).await.unwrap();
                }
                None => {}
            }
        }
    }

    async fn drain_own(redis: &RedisPool, task_id: uuid::Uuid) {
        loop {
            match queue::dequeue_task(redis, 0).await.unwrap() {
                Some(t) if t.task_id == task_id => {
                    queue::ack_task(redis, task_id).await.unwrap();
                    return;
                }
                Some(t) => {
                    queue::ack_task(redis, t.task_id).await.unwrap();
                }
                None => return,
            }
        }
    }

    async fn insert_user_image(pool: &sqlx::PgPool) -> (uuid::Uuid, uuid::Uuid) {
        let user_id = uuid::Uuid::new_v4();
        let image_id = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES ($1, $2, 'x')")
            .bind(user_id)
            .bind(format!("test_{user_id}"))
            .execute(pool)
            .await
            .unwrap();
        let pk = format!("{:x}", image_id)[..16].to_string();
        sqlx::query(
            "INSERT INTO images (id, user_id, public_key, original_name, storage_key, \
             storage_backend, mime_type, file_size, sha256, url, status) \
             VALUES ($1, $2, $3, 'n', 'k', 'local', 'image/png', 1, $4, 'u', 'active')",
        )
        .bind(image_id)
        .bind(user_id)
        .bind(pk)
        .bind("a".repeat(64))
        .execute(pool)
        .await
        .unwrap();
        (user_id, image_id)
    }

    async fn cleanup_rows(pool: &sqlx::PgPool, image_id: uuid::Uuid, user_id: uuid::Uuid) {
        sqlx::query("DELETE FROM images WHERE id = $1")
            .bind(image_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_handle_task_result_ack() {
        let _guard = REDIS_LOCK.lock().await;
        let redis = test_redis_pool();
        let pool = test_pg_pool().await;
        let cfg = AppConfig::default();
        let task = sample_task();
        let got = enqueue_and_dequeue_own(&redis, &task).await;
        handle_task_result(0, &pool, &redis, got, Ok(Ok(())), &cfg).await;
        let mut conn = redis.get().await.unwrap();
        let status: Option<String> = conn.hget(task_key(task.task_id), "status").await.unwrap();
        assert_eq!(status.as_deref(), Some("done"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_handle_task_result_nack_retry() {
        let _guard = REDIS_LOCK.lock().await;
        let redis = test_redis_pool();
        let pool = test_pg_pool().await;
        let cfg = AppConfig::default();
        let task = sample_task();
        let got = enqueue_and_dequeue_own(&redis, &task).await;
        handle_task_result(
            0, &pool, &redis, got,
            Ok(Err(pipeline::PipelineError::Decode("boom".into()))),
            &cfg,
        )
        .await;
        let mut conn = redis.get().await.unwrap();
        let status: Option<String> = conn.hget(task_key(task.task_id), "status").await.unwrap();
        assert_eq!(status.as_deref(), Some("pending"));
        let data: Option<String> = conn.hget(task_key(task.task_id), "data").await.unwrap();
        let stored: queue::TaskPayload = serde_json::from_str(&data.unwrap()).unwrap();
        assert_eq!(stored.retry_count, 1);
        drop(conn);
        drain_own(&redis, task.task_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_handle_task_result_dead_letter() {
        let _guard = REDIS_LOCK.lock().await;
        let redis = test_redis_pool();
        let pool = test_pg_pool().await;
        let cfg = AppConfig::default();
        let (user_id, image_id) = insert_user_image(&pool).await;
        let mut task = sample_task();
        task.image_id = image_id;
        task.user_id = user_id;
        task.retry_count = 3;
        task.max_retries = 3;
        let got = enqueue_and_dequeue_own(&redis, &task).await;
        handle_task_result(
            0, &pool, &redis, got,
            Ok(Err(pipeline::PipelineError::Decode("fatal".into()))),
            &cfg,
        )
        .await;
        let mut conn = redis.get().await.unwrap();
        let status: Option<String> = conn.hget(task_key(task.task_id), "status").await.unwrap();
        assert_eq!(status.as_deref(), Some("dead"));
        let dead: Vec<String> = conn.lrange(KEY_DEAD, 0, -1).await.unwrap();
        assert!(dead.contains(&task.task_id.to_string()));
        conn.lrem::<_, _, ()>(KEY_DEAD, 1, task.task_id.to_string()).await.unwrap();
        drop(conn);
        let (ut_status,): (String,) =
            sqlx::query_as("SELECT status FROM upload_tasks WHERE image_id = $1 ORDER BY created_at DESC LIMIT 1")
                .bind(image_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(ut_status, "failed");
        let (img_status,): (String,) =
            sqlx::query_as("SELECT status FROM images WHERE id = $1")
                .bind(image_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(img_status, "failed");
        cleanup_rows(&pool, image_id, user_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_handle_dead_letter_writes_db() {
        let pool = test_pg_pool().await;
        let (user_id, image_id) = insert_user_image(&pool).await;
        handle_dead_letter(&pool, image_id, 4, 3, "boom").await;
        let (ut_status,): (String,) =
            sqlx::query_as("SELECT status FROM upload_tasks WHERE image_id = $1 ORDER BY created_at DESC LIMIT 1")
                .bind(image_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(ut_status, "failed");
        let (img_status,): (String,) =
            sqlx::query_as("SELECT status FROM images WHERE id = $1")
                .bind(image_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(img_status, "failed");
        cleanup_rows(&pool, image_id, user_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_init_worker_state() {
        let redis = test_redis_pool();
        let pool = test_pg_pool().await;
        let cfg = Arc::new(AppConfig::default());
        let state = init_worker_state(pool, redis, cfg).await.expect("state init");
        assert_eq!(state.config.worker.concurrency, 4);
        assert!(state.router.get("local").is_some());
        assert!(state.router.backend_count() >= 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_spawn_workers() {
        let _guard = REDIS_LOCK.lock().await;
        let redis = test_redis_pool();
        let pool = test_pg_pool().await;
        let cfg = Arc::new(AppConfig::default());
        let state = init_worker_state(pool, redis, cfg).await.unwrap();
        let handles = spawn_workers(state);
        assert_eq!(handles.len(), 4);
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        for h in handles {
            h.abort();
        }
    }

    fn exhausted_redis_pool() -> RedisPool {
        let mut rc = RedisConfig::from_url("redis://localhost:6379/2");
        let mut pc = deadpool_redis::PoolConfig::new(1);
        pc.timeouts.wait = Some(std::time::Duration::from_millis(50));
        rc.pool = Some(pc);
        rc.create_pool(Some(Runtime::Tokio1)).unwrap()
    }

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("phw-main-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_handle_task_result_timeout_nacks() {
        let _guard = REDIS_LOCK.lock().await;
        let redis = test_redis_pool();
        let pool = test_pg_pool().await;
        let cfg = AppConfig::default();
        let task = sample_task();
        let got = enqueue_and_dequeue_own(&redis, &task).await;
        let elapsed = tokio::time::timeout(
            tokio::time::Duration::from_millis(1),
            std::future::pending::<()>(),
        )
        .await
        .unwrap_err();
        handle_task_result(0, &pool, &redis, got, Err(elapsed), &cfg).await;
        let mut conn = redis.get().await.unwrap();
        let status: Option<String> = conn.hget(task_key(task.task_id), "status").await.unwrap();
        assert_eq!(status.as_deref(), Some("pending"));
        let data: Option<String> = conn.hget(task_key(task.task_id), "data").await.unwrap();
        let stored: queue::TaskPayload = serde_json::from_str(&data.unwrap()).unwrap();
        assert_eq!(stored.retry_count, 1);
        drop(conn);
        drain_own(&redis, task.task_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_handle_task_result_redis_errors() {
        let _guard = REDIS_LOCK.lock().await;
        let normal = test_redis_pool();
        let pool = test_pg_pool().await;
        let cfg = AppConfig::default();

        let task = sample_task();
        let got = enqueue_and_dequeue_own(&normal, &task).await;
        let exhausted = exhausted_redis_pool();
        let held = exhausted.get().await.unwrap();
        handle_task_result(0, &pool, &exhausted, got, Ok(Ok(())), &cfg).await;
        let mut conn = normal.get().await.unwrap();
        let status: Option<String> = conn.hget(task_key(task.task_id), "status").await.unwrap();
        assert_eq!(status.as_deref(), Some("processing"));
        drop(held);

        let task2 = sample_task();
        let got2 = enqueue_and_dequeue_own(&normal, &task2).await;
        let exhausted2 = exhausted_redis_pool();
        let held2 = exhausted2.get().await.unwrap();
        handle_task_result(
            0, &pool, &exhausted2, got2,
            Ok(Err(pipeline::PipelineError::Decode("boom".into()))),
            &cfg,
        )
        .await;
        let mut conn = normal.get().await.unwrap();
        let status: Option<String> = conn.hget(task_key(task2.task_id), "status").await.unwrap();
        assert_eq!(status.as_deref(), Some("processing"));
        drop(held2);

        queue::ack_task(&normal, task.task_id).await.unwrap();
        queue::ack_task(&normal, task2.task_id).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_worker_loop_deadletters_failed_task() {
        let _guard = REDIS_LOCK.lock().await;
        let redis = test_redis_pool();
        let pool = test_pg_pool().await;
        let mut cfg = AppConfig::default();
        cfg.worker.queue_poll_timeout = 1;
        let cfg = Arc::new(cfg);

        let orphan = uuid::Uuid::new_v4();
        let mut conn = redis.get().await.unwrap();
        conn.lpush::<_, _, ()>("pichost:tasks:pending", orphan.to_string())
            .await
            .unwrap();
        drop(conn);

        let mut task = sample_task();
        task.retry_count = 3;
        task.max_retries = 3;
        queue::enqueue_task(&redis, &task).await.unwrap();

        let dir = TempDir::new();
        let local: Arc<dyn StorageBackend> = Arc::new(LocalStorage::new(
            dir.path().to_path_buf(),
            "http://localhost:3000".into(),
        ));
        let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
        backends.insert("local".into(), local);
        let router = Arc::new(StorageRouter::new(backends, "local".into()));

        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(4),
            worker_loop(0, pool.clone(), redis.clone(), cfg, router),
        )
        .await;
        assert!(result.is_err());

        let mut conn = redis.get().await.unwrap();
        let status: Option<String> = conn.hget(task_key(task.task_id), "status").await.unwrap();
        assert_eq!(status.as_deref(), Some("dead"));
        let dead: Vec<String> = conn.lrange(KEY_DEAD, 0, -1).await.unwrap();
        assert!(dead.contains(&task.task_id.to_string()));
        conn.lrem::<_, _, ()>(KEY_DEAD, 1, task.task_id.to_string())
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_init_worker_state_recover_stale() {
        let _guard = REDIS_LOCK.lock().await;
        let redis = test_redis_pool();
        let pool = test_pg_pool().await;
        let cfg = Arc::new(AppConfig::default());
        let task = sample_task();
        let got = enqueue_and_dequeue_own(&redis, &task).await;
        let mut conn = redis.get().await.unwrap();
        conn.hset::<_, _, _, ()>(
            &task_key(got.task_id),
            "updated_at",
            "2020-01-01T00:00:00Z",
        )
        .await
        .unwrap();
        drop(conn);
        let state = init_worker_state(pool, redis, cfg).await.expect("state init");
        assert!(state.router.get("local").is_some());
        drain_own(&state.redis, got.task_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_init_worker_state_with_rustfs() {
        let _guard = REDIS_LOCK.lock().await;
        let redis = test_redis_pool();
        let pool = test_pg_pool().await;
        let mut cfg = AppConfig::default();
        cfg.storage.rustfs = Some(pichost_core::config::RustfsStorageConfig {
            endpoint: "http://localhost:9000".into(),
            bucket: "test".into(),
            access_key: "minioadmin".into(),
            secret_key: "minioadmin".into(),
            region: "us-east-1".into(),
            use_ssl: false,
            public_endpoint: None,
        });
        let state = init_worker_state(pool, redis, Arc::new(cfg)).await.expect("state init");
        assert!(state.router.get("rustfs").is_some());
        assert!(state.router.get("local").is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_queue_helper_loop_paths() {
        let _guard = REDIS_LOCK.lock().await;
        let redis = test_redis_pool();
        let foreign = sample_task();
        let own = sample_task();
        queue::enqueue_task(&redis, &foreign).await.unwrap();
        queue::enqueue_task(&redis, &own).await.unwrap();
        let got = enqueue_and_dequeue_own(&redis, &own).await;
        assert_eq!(got.task_id, own.task_id);
        queue::ack_task(&redis, own.task_id).await.unwrap();

        let foreign2 = sample_task();
        let own2 = sample_task();
        queue::enqueue_task(&redis, &foreign2).await.unwrap();
        queue::enqueue_task(&redis, &own2).await.unwrap();
        drain_own(&redis, own2.task_id).await;
    }
}
