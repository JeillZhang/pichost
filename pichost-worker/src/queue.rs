use chrono::Utc;
use deadpool_redis::{redis::AsyncCommands, Pool};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Task payload for async image processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPayload {
    pub task_id: Uuid,
    pub image_id: Uuid,
    pub user_id: Uuid,
    pub storage_backend: String,
    pub source_key: String,
    pub source_mime: String,
    pub retry_count: i32,
    pub max_retries: i32,
}

/// Errors from queue operations.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("redis error: {0}")]
    Redis(#[from] deadpool_redis::redis::RedisError),
    #[error("pool error: {0}")]
    Pool(#[from] deadpool_redis::PoolError),
    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("missing task data for id {0}")]
    MissingData(Uuid),
    #[error("invalid uuid: {0}")]
    InvalidUuid(String),
}

/// Action to take after a NACK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NackAction {
    Retry,
    DeadLetter,
}

// Redis key constants
const KEY_PENDING: &str = "pichost:tasks:pending";
const KEY_PROCESSING: &str = "pichost:tasks:processing";
const KEY_DEAD: &str = "pichost:tasks:dead";
const KEY_PREFIX: &str = "pichost:task:";

fn task_key(task_id: Uuid) -> String {
    format!("{}{}", KEY_PREFIX, task_id)
}

/// Enqueue a task to the pending queue.
///
/// Serializes `task` to JSON, stores it in an HSET under `pichost:task:{task_id}`,
/// and pushes the task ID to `pichost:tasks:pending`.
#[allow(dead_code)]
pub async fn enqueue_task(redis: &Pool, task: &TaskPayload) -> Result<(), QueueError> {
    let mut conn = redis.get().await?;
    let key = task_key(task.task_id);
    let now = Utc::now().to_rfc3339();
    let json = serde_json::to_string(task)?;

    // Store task data and metadata in HSET
    conn.hset::<_, _, _, ()>(&key, "data", &json).await?;
    conn.hset::<_, _, _, ()>(&key, "status", "pending").await?;
    conn.hset::<_, _, _, ()>(&key, "created_at", &now).await?;
    conn.hset::<_, _, _, ()>(&key, "updated_at", &now).await?;

    // Push to pending queue (list)
    conn.lpush::<_, _, ()>(KEY_PENDING, task.task_id.to_string())
        .await?;

    Ok(())
}

/// Dequeue a task from the pending queue.
///
/// Uses `BRPOPLPUSH` to atomically move a task ID from `pichost:tasks:pending`
/// to `pichost:tasks:processing`. Then reads the full task payload from the HSET.
/// Returns `None` if the queue is empty after `timeout` seconds.
pub async fn dequeue_task(redis: &Pool, timeout: u64) -> Result<Option<TaskPayload>, QueueError> {
    let mut conn = redis.get().await?;

    // Atomically move from pending to processing.
    // BLOCKING — waits up to `timeout` seconds for an element.
    let task_id_str: Option<String> = conn
        .brpoplpush(KEY_PENDING, KEY_PROCESSING, timeout as f64)
        .await?;

    let task_id_str = match task_id_str {
        Some(s) => s,
        None => return Ok(None),
    };

    let task_id: Uuid = task_id_str
        .parse()
        .map_err(|e| QueueError::InvalidUuid(format!("invalid task id in queue: {}", e)))?;

    let key = task_key(task_id);

    // Read the full task data from the HSET
    let json: Option<String> = conn.hget(&key, "data").await?;
    let json = match json {
        Some(j) => j,
        None => {
            // Orphaned task — data hash was never written. Clean up and skip.
            conn.lrem::<_, _, ()>(KEY_PROCESSING, 1, task_id.to_string())
                .await.map_err(QueueError::Redis)?;
            tracing::warn!(%task_id, "cleaned up orphaned task (no data hash)");
            return Err(QueueError::MissingData(task_id));
        }
    };

    let task: TaskPayload = serde_json::from_str(&json)?;

    // Mark as processing
    let now = Utc::now().to_rfc3339();
    conn.hset::<_, _, _, ()>(&key, "status", "processing")
        .await?;
    conn.hset::<_, _, _, ()>(&key, "updated_at", &now).await?;

    Ok(Some(task))
}

/// Acknowledge a task as completed.
///
/// Removes the task ID from `pichost:tasks:processing` and sets `status = done`
/// in the HSET.
pub async fn ack_task(redis: &Pool, task_id: Uuid) -> Result<(), QueueError> {
    let mut conn = redis.get().await?;
    let key = task_key(task_id);
    let now = Utc::now().to_rfc3339();

    // Remove one occurrence from the processing queue
    conn.lrem::<_, _, ()>(KEY_PROCESSING, 1, task_id.to_string())
        .await?;

    // Mark as done
    conn.hset::<_, _, _, ()>(&key, "status", "done").await?;
    conn.hset::<_, _, _, ()>(&key, "updated_at", &now).await?;

    Ok(())
}

/// Negative acknowledgment — retry or send to dead-letter queue.
///
/// If `retry_count < max_retries`, increments the retry count and re-enqueues
/// the task to `pichost:tasks:pending`. Otherwise moves the task ID to
/// `pichost:tasks:dead` and sets `status = dead`.
///
/// Returns `NackAction::Retry` or `NackAction::DeadLetter` accordingly.
pub async fn nack_task(
    redis: &Pool,
    task: &TaskPayload,
    err: &str,
) -> Result<NackAction, QueueError> {
    let mut conn = redis.get().await?;
    let key = task_key(task.task_id);
    let now = Utc::now().to_rfc3339();

    // Remove from processing queue
    conn.lrem::<_, _, ()>(KEY_PROCESSING, 1, task.task_id.to_string())
        .await?;

    // Store the error message
    conn.hset::<_, _, _, ()>(&key, "error", err).await?;
    conn.hset::<_, _, _, ()>(&key, "updated_at", &now).await?;

    if task.retry_count < task.max_retries {
        // Re-enqueue with incremented retry count
        let mut updated = task.clone();
        updated.retry_count += 1;

        let json = serde_json::to_string(&updated)?;
        conn.hset::<_, _, _, ()>(&key, "data", &json).await?;
        conn.hset::<_, _, _, ()>(&key, "status", "pending").await?;
        conn.lpush::<_, _, ()>(KEY_PENDING, task.task_id.to_string())
            .await?;

        Ok(NackAction::Retry)
    } else {
        // Max retries exhausted — move to dead-letter queue
        conn.lpush::<_, _, ()>(KEY_DEAD, task.task_id.to_string())
            .await?;
        conn.hset::<_, _, _, ()>(&key, "status", "dead").await?;

        tracing::warn!(
            task_id = %task.task_id,
            image_id = %task.image_id,
            retries = task.retry_count,
            max_retries = task.max_retries,
            error = err,
            "task moved to dead-letter queue after exhausting retries"
        );

        Ok(NackAction::DeadLetter)
    }
}

/// Recover tasks that have been stuck in the processing queue beyond the timeout.
///
/// Scans all task IDs in `pichost:tasks:processing`, checks `updated_at`, and
/// re-enqueues any that are older than `task_timeout_secs`. Returns the list of
/// recovered task payloads.
pub async fn recover_stale_tasks(
    redis: &Pool,
    task_timeout_secs: u64,
) -> Result<Vec<TaskPayload>, QueueError> {
    let mut conn = redis.get().await?;

    // Get all task IDs currently in the processing queue
    let task_ids: Vec<String> = conn.lrange(KEY_PROCESSING, 0, -1).await?;
    let mut recovered = Vec::new();
    let cutoff = Utc::now() - chrono::Duration::seconds(task_timeout_secs as i64);

    for id_str in &task_ids {
        let task_id: Uuid = match id_str.parse() {
            Ok(id) => id,
            Err(_) => {
                tracing::warn!("invalid uuid in processing queue: {}", id_str);
                continue;
            }
        };

        let key = task_key(task_id);

        // Check updated_at timestamp
        let updated_at_str: Option<String> = conn.hget(&key, "updated_at").await?;
        let updated_at_str = match updated_at_str {
            Some(s) => s,
            None => {
                tracing::warn!("no updated_at for task {}", task_id);
                continue;
            }
        };
        let updated_at = match parse_task_updated_at(&updated_at_str, task_id) {
            Some(ts) => ts,
            None => continue,
        };

        if updated_at >= cutoff {
            // Not stale yet
            continue;
        }

        // Recover the stale task
        if let Some(task) = recover_single_task(&mut *conn, task_id, id_str).await? {
            recovered.push(task);
        }
    }

    Ok(recovered)
}

/// Parse a RFC 3339 timestamp string from Redis into a UTC DateTime.
///
/// Returns `None` (and logs a warning) if the string is not a valid timestamp.
fn parse_task_updated_at(
    updated_at_str: &str,
    task_id: Uuid,
) -> Option<chrono::DateTime<Utc>> {
    match chrono::DateTime::parse_from_rfc3339(updated_at_str) {
        Ok(dt) => Some(dt.with_timezone(&Utc)),
        Err(_) => {
            tracing::warn!(
                "invalid timestamp for task {}: {}",
                task_id,
                updated_at_str
            );
            None
        }
    }
}

/// Read, remove, and re-enqueue a single stale task.
///
/// Reads the task data hash, removes the task ID from the processing queue,
/// resets its status to `pending`, and pushes it back onto the pending queue.
///
/// Returns `Some(task)` on success, `None` if the task data is missing or corrupt,
/// or an error if a Redis operation fails.
async fn recover_single_task(
    conn: &mut impl AsyncCommands,
    task_id: Uuid,
    id_str: &str,
) -> Result<Option<TaskPayload>, QueueError> {
    let key = task_key(task_id);

    // Read the task data
    let json: Option<String> = conn.hget(&key, "data").await?;
    let task: TaskPayload = match json {
        Some(j) => match serde_json::from_str(&j) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("invalid task data for {}: {}", task_id, e);
                return Ok(None);
            }
        },
        None => {
            tracing::warn!("no task data for {}", task_id);
            return Ok(None);
        }
    };

    // Remove from processing and re-enqueue
    conn.lrem::<_, _, ()>(KEY_PROCESSING, 1, id_str).await?;

    let now = Utc::now().to_rfc3339();
    conn.hset::<_, _, _, ()>(&key, "status", "pending").await?;
    conn.hset::<_, _, _, ()>(&key, "updated_at", &now).await?;
    conn.hset::<_, _, _, ()>(&key, "error", "recovered: stale")
        .await?;
    conn.lpush::<_, _, ()>(KEY_PENDING, id_str).await?;

    tracing::info!(
        task_id = %task.task_id,
        image_id = %task.image_id,
        "recovered stale task from processing queue"
    );

    Ok(Some(task))
}
