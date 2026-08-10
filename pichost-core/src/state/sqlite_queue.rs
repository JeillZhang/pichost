use crate::models::TaskPayload;
use crate::state::{NackAction, Queue, QueueError};
use sqlx::SqlitePool;
use std::time::Duration;
use uuid::Uuid;

pub struct SqliteQueue {
    pool: SqlitePool,
}

impl SqliteQueue {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl Queue for SqliteQueue {
    async fn enqueue(&self, p: &TaskPayload) -> Result<(), QueueError> {
        let json = serde_json::to_string(p).map_err(|e| QueueError::Other(e.to_string()))?;
        sqlx::query(
            "INSERT INTO pending_tasks (task_id, payload_json, status) VALUES (?, ?, 'pending')",
        )
        .bind(p.task_id.to_string())
        .bind(json)
        .execute(&self.pool)
        .await
        .map_err(|e| QueueError::Other(e.to_string()))?;
        Ok(())
    }

    async fn dequeue(&self, _timeout: Duration) -> Result<Option<TaskPayload>, QueueError> {
        let row = sqlx::query_as::<_, (String, String)>(
            "UPDATE pending_tasks SET status='processing', claimed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'), updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') \
             WHERE task_id = (SELECT task_id FROM pending_tasks WHERE status='pending' AND claimed_at IS NULL ORDER BY created_at LIMIT 1) \
             RETURNING task_id, payload_json",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| QueueError::Other(e.to_string()))?;
        match row {
            Some((_id, json)) => serde_json::from_str(&json)
                .map(Some)
                .map_err(|e| QueueError::Other(e.to_string())),
            None => Ok(None),
        }
    }

    async fn ack(&self, task_id: Uuid) -> Result<(), QueueError> {
        sqlx::query("DELETE FROM pending_tasks WHERE task_id = ?")
            .bind(task_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| QueueError::Other(e.to_string()))?;
        Ok(())
    }

    async fn nack(
        &self,
        task_id: Uuid,
        retry_count: i32,
        max_retries: i32,
    ) -> Result<NackAction, QueueError> {
        if retry_count < max_retries {
            sqlx::query(
                "UPDATE pending_tasks SET status='pending', claimed_at=NULL, retry_count=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE task_id = ?",
            )
            .bind(retry_count + 1)
            .bind(task_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| QueueError::Other(e.to_string()))?;
            Ok(NackAction::Retry)
        } else {
            sqlx::query(
                "UPDATE pending_tasks SET status='dead', claimed_at=NULL, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE task_id = ?",
            )
            .bind(task_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| QueueError::Other(e.to_string()))?;
            Ok(NackAction::DeadLetter)
        }
    }
}
