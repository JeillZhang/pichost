use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use pichost_api::cache::{self, Cache, InviteVerifyResult};
use uuid::Uuid;

fn test_cache() -> Cache {
    let url = std::env::var("PICHOST_REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string());
    Cache::new(cache::create_pool(&url, 5))
}

fn unique_key(prefix: &str) -> String {
    format!("pichost:test:{}:{}", prefix, Uuid::new_v4().simple())
}

async fn ttl_of(cache: &Cache, key: &str) -> i64 {
    let mut conn = cache.get_pool().get().await.expect("redis conn");
    deadpool_redis::redis::cmd("TTL")
        .arg(key)
        .query_async::<_, i64>(&mut conn)
        .await
        .expect("ttl query")
}

// ── Basic string ops ──────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_set_del_exists_roundtrip() {
    let cache = test_cache();
    let key = unique_key("basic");

    assert_eq!(cache.get(&key).await.unwrap(), None);
    assert!(!cache.exists(&key).await.unwrap());

    cache.set(&key, "hello").await.unwrap();
    assert!(cache.exists(&key).await.unwrap());
    assert_eq!(cache.get(&key).await.unwrap().as_deref(), Some("hello"));

    cache.del(&key).await.unwrap();
    assert!(!cache.exists(&key).await.unwrap());
    assert_eq!(cache.get(&key).await.unwrap(), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn set_ex_applies_ttl_and_expires() {
    let cache = test_cache();
    let key = unique_key("ttl");

    cache.set_ex(&key, "v", 60).await.unwrap();
    assert_eq!(cache.get(&key).await.unwrap().as_deref(), Some("v"));
    let ttl = ttl_of(&cache, &key).await;
    assert!(ttl > 0 && ttl <= 60, "ttl was {ttl}");

    cache.set_ex(&key, "short", 1).await.unwrap();
    assert!(cache.exists(&key).await.unwrap());
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(!cache.exists(&key).await.unwrap());
    assert_eq!(cache.get(&key).await.unwrap(), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn incr_increments_and_sets_ttl_on_first_creation() {
    let cache = test_cache();
    let key = unique_key("incr");

    assert_eq!(cache.incr(&key, 60).await.unwrap(), 1);
    assert_eq!(cache.incr(&key, 60).await.unwrap(), 2);
    assert_eq!(cache.incr(&key, 60).await.unwrap(), 3);

    let ttl = ttl_of(&cache, &key).await;
    assert!(ttl > 0 && ttl <= 60, "ttl was {ttl}");
    cache.del(&key).await.unwrap();
}

// ── Metadata cache ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn cached_meta_miss_then_hit() {
    let cache = test_cache();
    let image_id = Uuid::new_v4();
    let calls = Arc::new(AtomicUsize::new(0));

    let c1 = calls.clone();
    let val: Result<(i64, String), std::io::Error> = cache
        .cached_meta(&image_id, 60, async move {
            c1.fetch_add(1, Ordering::SeqCst);
            Ok((42, "from_db".to_string()))
        })
        .await;
    assert_eq!(val.unwrap(), (42, "from_db".to_string()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let c2 = calls.clone();
    let val: Result<(i64, String), std::io::Error> = cache
        .cached_meta(&image_id, 60, async move {
            c2.fetch_add(1, Ordering::SeqCst);
            Ok((99, "should_not_be_used".to_string()))
        })
        .await;
    assert_eq!(val.unwrap(), (42, "from_db".to_string()));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "fetch_fn called on cache hit");
}

// ── Thumbnail cache ───────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn cached_thumb_miss_then_hit() {
    let cache = test_cache();
    let cache_key = unique_key("thumb");
    let calls = Arc::new(AtomicUsize::new(0));

    let c1 = calls.clone();
    let val: Result<Vec<u8>, std::io::Error> = cache
        .cached_thumb(&cache_key, 60, async move {
            c1.fetch_add(1, Ordering::SeqCst);
            Ok(vec![1u8, 2, 3, 4, 5])
        })
        .await;
    assert_eq!(val.unwrap(), vec![1u8, 2, 3, 4, 5]);

    let c2 = calls.clone();
    let val: Result<Vec<u8>, std::io::Error> = cache
        .cached_thumb(&cache_key, 60, async move {
            c2.fetch_add(1, Ordering::SeqCst);
            Ok(vec![9u8])
        })
        .await;
    assert_eq!(val.unwrap(), vec![1u8, 2, 3, 4, 5]);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "fetch_fn called on cache hit");
    cache.del(&format!("pichost:thumb:{cache_key}")).await.unwrap();
}

// ── User stats cache ──────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn user_stats_incr_accumulates() {
    let cache = test_cache();
    let user_id = Uuid::new_v4();

    cache.incr_user_stat(&user_id, "uploads", 5).await.unwrap();
    cache.incr_user_stat(&user_id, "uploads", 7).await.unwrap();

    let stats = cache.get_user_stats(&user_id).await.unwrap().unwrap();
    assert_eq!(stats.get("uploads").map(String::as_str), Some("12"));
    cache.del(&format!("pichost:stats:{user_id}")).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn user_stats_set_overwrites_and_stores_none_as_empty() {
    let cache = test_cache();
    let user_id = Uuid::new_v4();

    cache
        .set_user_stats(&user_id, &[("images", Some(3)), ("size", None)])
        .await
        .unwrap();

    let stats = cache.get_user_stats(&user_id).await.unwrap().unwrap();
    assert_eq!(stats.get("images").map(String::as_str), Some("3"));
    assert_eq!(stats.get("size").map(String::as_str), Some(""));

    cache
        .set_user_stats(&user_id, &[("images", Some(9))])
        .await
        .unwrap();
    let stats = cache.get_user_stats(&user_id).await.unwrap().unwrap();
    assert_eq!(stats.get("images").map(String::as_str), Some("9"));
    cache.del(&format!("pichost:stats:{user_id}")).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn user_stats_unknown_user_returns_empty_map() {
    let cache = test_cache();
    let stats: HashMap<String, String> =
        cache.get_user_stats(&Uuid::new_v4()).await.unwrap().unwrap();
    assert!(stats.is_empty());
}

// ── Trait-object usage (pichost_core::state::Cache) ───────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running Redis"]
async fn redis_cache_trait_roundtrip() {
    let cache = test_cache();
    let c: &dyn pichost_core::state::Cache = &cache;
    c.set_ex("t:1", "v", 60).await.unwrap();
    assert_eq!(c.get("t:1").await.unwrap(), Some("v".into()));
    assert!(c.exists("t:1").await.unwrap());
    c.del("t:1").await.unwrap();
    assert_eq!(c.get("t:1").await.unwrap(), None);
}

// ── Invite codes ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn invite_code_valid_then_consumed() {
    let cache = test_cache();
    let admin_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    let code = cache.create_invite_code(&admin_id, 3600).await.unwrap();
    assert_eq!(code.len(), 32);
    assert_eq!(
        cache.verify_invite_code(&code).await.unwrap(),
        InviteVerifyResult::Valid
    );

    assert!(cache.consume_invite_code(&code, &user_id).await.unwrap());
    assert_eq!(
        cache.verify_invite_code(&code).await.unwrap(),
        InviteVerifyResult::Used
    );

    assert!(!cache.consume_invite_code(&format!("no_such_{code}"), &user_id).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn invite_code_not_found() {
    let cache = test_cache();
    let code = format!("missing{}", Uuid::new_v4().simple());
    assert_eq!(
        cache.verify_invite_code(&code).await.unwrap(),
        InviteVerifyResult::NotFound
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn invite_code_expired_when_expires_at_past() {
    let cache = test_cache();
    let code = cache.create_invite_code(&Uuid::new_v4(), 60).await.unwrap();
    let key = format!("pichost:invite:{code}");
    let past = chrono::Utc::now().timestamp() - 100;

    let mut conn = cache.get_pool().get().await.unwrap();
    deadpool_redis::redis::cmd("HSET")
        .arg(&key)
        .arg("expires_at")
        .arg(past.to_string())
        .query_async::<_, ()>(&mut conn)
        .await
        .unwrap();
    drop(conn);

    assert_eq!(
        cache.verify_invite_code(&code).await.unwrap(),
        InviteVerifyResult::Expired
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn list_invite_codes_sorted_desc_and_consumed_removed() {
    let cache = test_cache();
    let admin_id = Uuid::new_v4();

    let older = cache.create_invite_code(&admin_id, 3600).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let newer = cache.create_invite_code(&admin_id, 3600).await.unwrap();

    let codes = cache.list_invite_codes().await.unwrap();
    let pos_older = codes.iter().position(|c| c.code == older).expect("older present");
    let pos_newer = codes.iter().position(|c| c.code == newer).expect("newer present");
    assert!(pos_newer < pos_older, "not sorted desc: {codes:?}");
    assert_eq!(codes[pos_newer].created_by, admin_id);

    cache.consume_invite_code(&older, &Uuid::new_v4()).await.unwrap();
    let codes = cache.list_invite_codes().await.unwrap();
    assert!(
        !codes.iter().any(|c| c.code == older),
        "consumed code still listed: {codes:?}"
    );
    assert!(codes.iter().any(|c| c.code == newer));
}
