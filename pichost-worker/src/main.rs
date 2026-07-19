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
    let pool = db::create_pool(
        &app_config.database.url,
        app_config.database.max_connections,
    )
    .await?;
    db::run_migrations(&pool).await?;
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
