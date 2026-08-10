//! Integration tests for the `Queue` trait implementation on top of Redis.
//!
//! Requires a running Redis at `redis://localhost:6379` (database 3 is used
//! to keep these tests isolated from the inline queue unit tests on DB 1 and
//! the worker main.rs tests on DB 2).

use pichost_core::models::TaskPayload;
use pichost_core::state::Queue;
use pichost_worker::queue::RedisQueue;
use uuid::Uuid;

fn test_pool() -> deadpool_redis::Pool {
    deadpool_redis::Config::from_url("redis://localhost:6379/3")
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running Redis"]
async fn redis_queue_trait_enqueue_dequeue() {
    let q = RedisQueue::new(test_pool());
    let p = TaskPayload {
        task_id: Uuid::new_v4(),
        image_id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        storage_backend: "local".into(),
        storage_config_id: None,
        storage_backend_name: "local".into(),
        source_key: "k".into(),
        source_mime: "image/png".into(),
        retry_count: 0,
        max_retries: 3,
    };
    q.enqueue(&p).await.unwrap();
    let got = q
        .dequeue(std::time::Duration::from_millis(100))
        .await
        .unwrap();
    assert_eq!(got.unwrap().task_id, p.task_id);
    q.ack(p.task_id).await.unwrap();
}
