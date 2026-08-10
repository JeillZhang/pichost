use crate::db::{create_sqlite_pool, run_sqlite_migrations};
use crate::state::noop_cache::NoopCache;
use crate::state::sqlite_blacklist::SqliteBlacklist;
use crate::state::sqlite_invite::SqliteInviteStore;
use crate::state::sqlite_queue::SqliteQueue;
use crate::state::sqlite_rate_limiter::SqliteRateLimiter;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_queue_reclaim_stale_processing() {
    let pool = create_sqlite_pool("sqlite::memory:", 5).await.unwrap();
    run_sqlite_migrations(&pool).await.unwrap();
    let q = SqliteQueue::new(pool.clone());
    let p = sample_task();
    q.enqueue(&p).await.unwrap();
    // Claim the task (status -> processing), simulating a crash before ack.
    let got = q.dequeue(Duration::from_millis(50)).await.unwrap().unwrap();
    assert_eq!(got.task_id, p.task_id);
    // Fresh handle (as after a restart) reclaims the stuck task.
    SqliteQueue::new(pool.clone())
        .reclaim_stale()
        .await
        .unwrap();
    let requeued = SqliteQueue::new(pool)
        .dequeue(Duration::from_millis(50))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(requeued.task_id, p.task_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_blacklist_and_rate_limiter() {
    let pool = create_sqlite_pool("sqlite::memory:", 5).await.unwrap();
    run_sqlite_migrations(&pool).await.unwrap();

    let bl = SqliteBlacklist::new(pool.clone());
    assert!(!bl.check("jti-1").await.unwrap());
    bl.revoke("jti-1", Duration::from_secs(60)).await.unwrap();
    assert!(bl.check("jti-1").await.unwrap());

    let rl = SqliteRateLimiter::new(pool);
    let r1 = rl
        .check("auth", "1.2.3.4", 2, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(r1.allowed);
    let _r2 = rl
        .check("auth", "1.2.3.4", 2, Duration::from_secs(60))
        .await
        .unwrap();
    let r3 = rl
        .check("auth", "1.2.3.4", 2, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(!r3.allowed);
    assert!(r3.retry_after > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_invite_store_and_noop_cache() {
    let pool = create_sqlite_pool("sqlite::memory:", 5).await.unwrap();
    run_sqlite_migrations(&pool).await.unwrap();
    let inv = SqliteInviteStore::new(pool);
    let admin = Uuid::new_v4();
    inv.create("CODE1", admin, 3600).await.unwrap();
    assert!(inv.verify("CODE1").await.unwrap());
    assert_eq!(
        inv.verify_detail("CODE1").await.unwrap(),
        InviteVerifyStatus::Valid
    );
    inv.consume("CODE1", Uuid::new_v4()).await.unwrap();
    assert!(!inv.verify("CODE1").await.unwrap());
    // Parity with RedisInviteStore: consuming a missing/used code errors.
    assert!(inv.consume("NOPE", Uuid::new_v4()).await.is_err());
    assert!(inv.consume("CODE1", Uuid::new_v4()).await.is_err());
    assert_eq!(
        inv.verify_detail("CODE1").await.unwrap(),
        InviteVerifyStatus::Used
    );
    assert_eq!(
        inv.verify_detail("NOPE").await.unwrap(),
        InviteVerifyStatus::NotFound
    );

    let noop = NoopCache;
    assert_eq!(noop.get("x").await.unwrap(), None);
    noop.set_ex("x", "v", 60).await.unwrap();
    assert_eq!(noop.get("x").await.unwrap(), None); // noop 恒 miss
    assert_eq!(noop.incr("c", 60).await.unwrap(), 1); // 恒返回 1（brief 语义）
}
