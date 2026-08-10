use crate::db::{create_sqlite_pool, run_sqlite_migrations};
use crate::state::sqlite_queue::SqliteQueue;
use crate::state::*;
use std::time::Duration;

// 编译级契约：trait 必须可作 trait object
fn assert_queue_object(_q: &dyn Queue) {}
fn assert_blacklist_object(_b: &dyn Blacklist) {}
fn assert_rate_limiter_object(_r: &dyn RateLimiter) {}
fn assert_invite_object(_i: &dyn InviteStore) {}
fn assert_cache_object(_c: &dyn Cache) {}

#[test]
fn traits_are_object_safe() {
    assert_queue_object(&MockQueue);
    assert_blacklist_object(&MockBlacklist);
    assert_rate_limiter_object(&MockRateLimiter);
    assert_invite_object(&MockInviteStore);
    assert_cache_object(&MockCache);
}

fn sample_task() -> crate::models::TaskPayload {
    use uuid::Uuid;
    crate::models::TaskPayload {
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
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_queue_claim_ack_cycle() {
    let pool = create_sqlite_pool("sqlite::memory:", 5).await.unwrap();
    run_sqlite_migrations(&pool).await.unwrap();
    let q = SqliteQueue::new(pool);
    let p = sample_task();
    q.enqueue(&p).await.unwrap();
    let got = q.dequeue(Duration::from_millis(50)).await.unwrap().unwrap();
    assert_eq!(got.task_id, p.task_id);
    // 原子 claim：已 claim 任务第二次 dequeue 拿不到
    let second = q.dequeue(Duration::from_millis(50)).await.unwrap();
    assert!(second.is_none());
    q.ack(p.task_id).await.unwrap();
    let third = q.dequeue(Duration::from_millis(50)).await.unwrap();
    assert!(third.is_none());
}
