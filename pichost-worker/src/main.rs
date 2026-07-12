use deadpool_redis::{Config as RedisConfig, Pool as RedisPool, Runtime};
use std::sync::Arc;

mod config;
mod db;
mod pipeline;
mod processor;
mod queue;

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

    // 4. Startup recovery: re-enqueue stale tasks from processing queue
    let recovered = queue::recover_stale_tasks(&redis_pool, app_config.worker.task_timeout).await?;
    if !recovered.is_empty() {
        tracing::info!(count = recovered.len(), "recovered stale tasks");
    }

    // 5. Wrap config in Arc for sharing across tasks
    let config = Arc::new(app_config);
    let concurrency = config.worker.concurrency;

    // 6. Spawn worker loop for each concurrency slot
    let mut handles = Vec::with_capacity(concurrency);
    for i in 0..concurrency {
        let pool = pool.clone();
        let redis = redis_pool.clone();
        let config = config.clone();

        let handle = tokio::spawn(async move {
            tracing::info!(worker_id = i, "worker started");
            worker_loop(i, pool, redis, config).await;
        });
        handles.push(handle);
    }

    // 7. Wait for all workers (they run forever unless shutdown signal)
    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn worker_loop(
    worker_id: usize,
    pool: sqlx::PgPool,
    redis: RedisPool,
    config: Arc<pichost_core::config::AppConfig>,
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
            pipeline::process_task(&pool, &config, &task),
        )
        .await;

        match process_result {
            Ok(Ok(())) => {
                // Success — acknowledge
                if let Err(e) = queue::ack_task(&redis, task_id).await {
                    tracing::error!(worker_id, %task_id, error = %e, "ack failed");
                }
                tracing::info!(worker_id, %task_id, "task completed");
            }
            Ok(Err(e)) => {
                // Processing failed — nack (retry or dead-letter)
                tracing::warn!(worker_id, %task_id, error = %e, "task processing failed");
                match queue::nack_task(&redis, &task, &e.to_string()).await {
                    Ok(action) => match action {
                        queue::NackAction::Retry => {
                            tracing::info!(
                                worker_id,
                                %task_id,
                                retry = task.retry_count + 1,
                                "task retrying"
                            );
                        }
                        queue::NackAction::DeadLetter => {
                            tracing::error!(worker_id, %task_id, "task dead-lettered");

                            // Update upload_tasks table with failure
                            let now = chrono::Utc::now();
                            let _ = sqlx::query(
                                r#"INSERT INTO upload_tasks
                                   (image_id, task_type, status, error, retry_count, max_retries, completed_at)
                                   VALUES ($1, 'all', 'failed', $2, $3, $4, $5)"#,
                            )
                            .bind(task.image_id)
                            .bind(e.to_string())
                            .bind(task.retry_count + 1)
                            .bind(task.max_retries)
                            .bind(now)
                            .execute(&pool)
                            .await;

                            // Mark image as failed
                            let _ =
                                sqlx::query("UPDATE images SET status = 'failed' WHERE id = $1")
                                    .bind(task.image_id)
                                    .execute(&pool)
                                    .await;
                        }
                    },
                    Err(e) => {
                        tracing::error!(worker_id, %task_id, error = %e, "nack failed");
                    }
                }
            }
            Err(_elapsed) => {
                // Timeout — nack as retry
                tracing::warn!(worker_id, %task_id, "task timed out");
                let timeout_err = format!("timed out after {}s", config.worker.task_timeout);
                let _ = queue::nack_task(&redis, &task, &timeout_err).await;
            }
        }
    }
}
