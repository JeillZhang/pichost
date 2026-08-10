use chrono::Utc;
use deadpool_redis::{redis::AsyncCommands, Pool};
use pichost_core::models::TaskPayload;
use pichost_core::state::{NackAction as StateNackAction, QueueError as StateQueueError};
use std::time::Duration;
use uuid::Uuid;

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

/// Redis-backed implementation of `pichost_core::state::Queue`.
///
/// Thin wrapper over the free queue functions in this module — the Redis key
/// layout (HSET task data + LPUSH/BRPOPLPUSH pending/processing lists) and
/// the retry/dead-letter semantics are unchanged.
#[derive(Clone)]
pub struct RedisQueue {
    pool: Pool,
}

/// Generic message stored in the task HSET `error` field when a task is nacked
/// through the `Queue` trait (the trait carries no error detail).
const NACK_ERR_MSG: &str = "nack: task failed";

/// Map any queue-layer error into the coarse `Queue` trait error type.
fn to_state_error(e: impl std::fmt::Display) -> StateQueueError {
    StateQueueError::Other(e.to_string())
}

impl RedisQueue {
    /// Create a queue backed by the given deadpool Redis pool.
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl pichost_core::state::Queue for RedisQueue {
    async fn enqueue(&self, payload: &TaskPayload) -> Result<(), StateQueueError> {
        enqueue_task(&self.pool, payload)
            .await
            .map_err(to_state_error)
    }

    async fn dequeue(&self, timeout: Duration) -> Result<Option<TaskPayload>, StateQueueError> {
        dequeue_task(&self.pool, timeout.as_secs())
            .await
            .map_err(to_state_error)
    }

    async fn ack(&self, task_id: Uuid) -> Result<(), StateQueueError> {
        ack_task(&self.pool, task_id).await.map_err(to_state_error)
    }

    async fn nack(
        &self,
        task_id: Uuid,
        retry_count: i32,
        max_retries: i32,
    ) -> Result<StateNackAction, StateQueueError> {
        // Reload the stored payload so a retry round-trips the full task data.
        let mut conn = self.pool.get().await.map_err(to_state_error)?;
        let key = task_key(task_id);
        let json: Option<String> = conn.hget(&key, "data").await.map_err(to_state_error)?;
        let mut task: TaskPayload = match json {
            Some(j) => serde_json::from_str(&j).map_err(to_state_error)?,
            None => {
                return Err(StateQueueError::Other(format!(
                    "missing task data for id {task_id}"
                )))
            }
        };
        task.retry_count = retry_count;
        task.max_retries = max_retries;
        let action = nack_task(&self.pool, &task, NACK_ERR_MSG)
            .await
            .map_err(to_state_error)?;
        Ok(match action {
            NackAction::Retry => StateNackAction::Retry,
            NackAction::DeadLetter => StateNackAction::DeadLetter,
        })
    }
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
                .await
                .map_err(QueueError::Redis)?;
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
fn parse_task_updated_at(updated_at_str: &str, task_id: Uuid) -> Option<chrono::DateTime<Utc>> {
    match chrono::DateTime::parse_from_rfc3339(updated_at_str) {
        Ok(dt) => Some(dt.with_timezone(&Utc)),
        Err(_) => {
            tracing::warn!("invalid timestamp for task {}: {}", task_id, updated_at_str);
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadpool_redis::Runtime;

    static REDIS_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

    fn test_pool() -> Pool {
        deadpool_redis::Config::from_url("redis://localhost:6379/1")
            .create_pool(Some(Runtime::Tokio1))
            .unwrap()
    }

    fn sample_task() -> TaskPayload {
        TaskPayload {
            task_id: Uuid::new_v4(),
            image_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            storage_backend: "local".into(),
            storage_config_id: None,
            storage_backend_name: "Local".into(),
            source_key: "source.png".into(),
            source_mime: "image/png".into(),
            retry_count: 0,
            max_retries: 3,
        }
    }

    async fn enqueue_and_dequeue_own(pool: &Pool, task: &TaskPayload) -> TaskPayload {
        loop {
            enqueue_task(pool, task).await.unwrap();
            match dequeue_task(pool, 1).await.unwrap() {
                Some(got) if got.task_id == task.task_id => return got,
                Some(got) => {
                    ack_task(pool, got.task_id).await.unwrap();
                }
                None => {}
            }
        }
    }

    async fn drain_own(pool: &Pool, task_id: Uuid) {
        loop {
            match dequeue_task(pool, 0).await.unwrap() {
                Some(got) if got.task_id == task_id => {
                    ack_task(pool, task_id).await.unwrap();
                    return;
                }
                Some(got) => {
                    ack_task(pool, got.task_id).await.unwrap();
                }
                None => return,
            }
        }
    }

    #[test]
    fn test_parse_task_updated_at_valid() {
        let dt = parse_task_updated_at("2026-01-01T00:00:00Z", Uuid::new_v4()).unwrap();
        assert_eq!(
            dt,
            chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn test_parse_task_updated_at_invalid() {
        assert!(parse_task_updated_at("garbage", Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_parse_task_updated_at_utc_normalized() {
        let dt = parse_task_updated_at("2026-01-01T08:00:00+08:00", Uuid::new_v4()).unwrap();
        assert_eq!(
            dt,
            chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn test_task_key_format() {
        let id = Uuid::new_v4();
        assert_eq!(task_key(id), format!("pichost:task:{id}"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_enqueue_dequeue_ack() {
        let _guard = REDIS_LOCK.lock().await;
        let pool = test_pool();
        let task = sample_task();
        let got = enqueue_and_dequeue_own(&pool, &task).await;
        assert_eq!(got.task_id, task.task_id);
        assert_eq!(got.retry_count, 0);
        ack_task(&pool, task.task_id).await.unwrap();
        let mut conn = pool.get().await.unwrap();
        let status: Option<String> = conn.hget(task_key(task.task_id), "status").await.unwrap();
        assert_eq!(status.as_deref(), Some("done"));
        let ids: Vec<String> = conn.lrange(KEY_PROCESSING, 0, -1).await.unwrap();
        assert!(!ids.contains(&task.task_id.to_string()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_dequeue_empty_returns_none() {
        let _guard = REDIS_LOCK.lock().await;
        let pool = test_pool();
        assert!(dequeue_task(&pool, 1).await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_dequeue_invalid_uuid() {
        let _guard = REDIS_LOCK.lock().await;
        let pool = test_pool();
        let mut conn = pool.get().await.unwrap();
        conn.lpush::<_, _, ()>(KEY_PENDING, "not-a-uuid")
            .await
            .unwrap();
        drop(conn);
        assert!(matches!(
            dequeue_task(&pool, 0).await,
            Err(QueueError::InvalidUuid(_))
        ));
        let mut conn = pool.get().await.unwrap();
        conn.lrem::<_, _, ()>(KEY_PROCESSING, 1, "not-a-uuid")
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_dequeue_orphaned_task() {
        let _guard = REDIS_LOCK.lock().await;
        let pool = test_pool();
        let orphan_id = Uuid::new_v4();
        let mut conn = pool.get().await.unwrap();
        conn.lpush::<_, _, ()>(KEY_PENDING, orphan_id.to_string())
            .await
            .unwrap();
        drop(conn);
        assert!(matches!(
            dequeue_task(&pool, 0).await,
            Err(QueueError::MissingData(id)) if id == orphan_id
        ));
        let mut conn = pool.get().await.unwrap();
        let ids: Vec<String> = conn.lrange(KEY_PROCESSING, 0, -1).await.unwrap();
        assert!(!ids.contains(&orphan_id.to_string()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_nack_retry() {
        let _guard = REDIS_LOCK.lock().await;
        let pool = test_pool();
        let task = sample_task();
        let got = enqueue_and_dequeue_own(&pool, &task).await;
        assert_eq!(
            nack_task(&pool, &got, "boom").await.unwrap(),
            NackAction::Retry
        );
        let mut conn = pool.get().await.unwrap();
        let key = task_key(task.task_id);
        let status: Option<String> = conn.hget(&key, "status").await.unwrap();
        assert_eq!(status.as_deref(), Some("pending"));
        let data: Option<String> = conn.hget(&key, "data").await.unwrap();
        let stored: TaskPayload = serde_json::from_str(&data.unwrap()).unwrap();
        assert_eq!(stored.retry_count, 1);
        let err: Option<String> = conn.hget(&key, "error").await.unwrap();
        assert_eq!(err.as_deref(), Some("boom"));
        let pending: Vec<String> = conn.lrange(KEY_PENDING, 0, -1).await.unwrap();
        assert!(pending.contains(&task.task_id.to_string()));
        drop(conn);
        drain_own(&pool, task.task_id).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_nack_dead_letter() {
        let _guard = REDIS_LOCK.lock().await;
        let pool = test_pool();
        let mut task = sample_task();
        task.retry_count = 3;
        task.max_retries = 3;
        let got = enqueue_and_dequeue_own(&pool, &task).await;
        assert_eq!(
            nack_task(&pool, &got, "fatal").await.unwrap(),
            NackAction::DeadLetter
        );
        let mut conn = pool.get().await.unwrap();
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
    async fn test_recover_stale_tasks() {
        let _guard = REDIS_LOCK.lock().await;
        let pool = test_pool();
        let task = sample_task();
        let got = enqueue_and_dequeue_own(&pool, &task).await;
        assert_eq!(got.task_id, task.task_id);
        let mut conn = pool.get().await.unwrap();
        conn.hset::<_, _, _, ()>(
            &task_key(task.task_id),
            "updated_at",
            "2020-01-01T00:00:00Z",
        )
        .await
        .unwrap();
        conn.lpush::<_, _, ()>(KEY_PROCESSING, Uuid::new_v4().to_string())
            .await
            .unwrap();
        conn.lpush::<_, _, ()>(KEY_PROCESSING, "garbage")
            .await
            .unwrap();
        drop(conn);
        let recovered = recover_stale_tasks(&pool, 10).await.unwrap();
        assert!(recovered.iter().any(|t| t.task_id == task.task_id));
        drain_own(&pool, task.task_id).await;
        let mut conn = pool.get().await.unwrap();
        conn.lrem::<_, _, ()>(KEY_PROCESSING, 1, "garbage")
            .await
            .unwrap();
        let orphan: Vec<String> = conn.lrange(KEY_PROCESSING, 0, -1).await.unwrap();
        for id in orphan {
            if Uuid::parse_str(&id).is_ok() {
                conn.lrem::<_, _, ()>(KEY_PROCESSING, 1, id).await.unwrap();
            }
        }
    }
}
