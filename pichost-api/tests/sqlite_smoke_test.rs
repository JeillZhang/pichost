use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

static MIGRATOR: Migrator = sqlx::migrate!("../migrations-sqlite");

async fn sqlite_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    SqlitePoolOptions::new().max_connections(5).connect_with(opts).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_quota_and_config_queries() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let uid: String = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash) VALUES ('u','h') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    // Dialect-neutral quota query (portable CAST, no PG-only ::BIGINT)
    let q: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(file_size), 0) AS BIGINT) FROM images WHERE user_id = ?")
        .bind(&uid).fetch_one(&pool).await.unwrap();
    assert_eq!(q, 0);
    // Dialect-neutral IN-list config lookup (ANY($1) replacement pattern)
    sqlx::query(
        "INSERT INTO user_storage_configs (user_id, name, provider, config) \
         VALUES (?, 'g', 'github', '{}')")
        .bind(&uid).execute(&pool).await.unwrap();
    let cfg_id: String = sqlx::query_scalar(
        "SELECT id FROM user_storage_configs WHERE user_id = ? LIMIT 1")
        .bind(&uid).fetch_one(&pool).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_storage_configs WHERE id IN (?, ?) AND user_id = ?")
        .bind(&cfg_id).bind(&cfg_id).bind(&uid).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_stats_and_update_queries() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let uid: String = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash) VALUES ('stats','h') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    // Dialect-neutral stats query: COUNT(*) is bigint on both drivers;
    // PG SUM() is NUMERIC, so COALESCE(SUM(...)) needs the CAST AS BIGINT
    let (count, total): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), CAST(COALESCE(SUM(file_size), 0) AS BIGINT) \
         FROM images WHERE user_id = ?")
        .bind(&uid).fetch_one(&pool).await.unwrap();
    assert_eq!((count, total), (0, 0));
    // Dialect-neutral update shape: bool/JSON binds via plain CASE WHEN,
    // CURRENT_TIMESTAMP instead of now()
    let res = sqlx::query(
        "UPDATE users SET email = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind("stats@example.com").bind(&uid).execute(&pool).await.unwrap();
    assert_eq!(res.rows_affected(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_admin_stats_queries() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let uid: String = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash) VALUES ('admin-stats','h') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    // Admin system stats (dialect-neutral: no ::BIGINT; PG SUM() is NUMERIC,
    // so COALESCE(SUM(...)) must be CAST AS BIGINT to decode i64)
    let q: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), CAST(COALESCE(SUM(file_size), 0) AS BIGINT) FROM images")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(q, (0, 0));
    // Admin active-users-24h (Rust-computed cutoff instead of NOW() - INTERVAL)
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT user_id) FROM images WHERE created_at >= ?")
        .bind(cutoff).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0);
    // Admin storage-quota SUM over nullable column (CAST keeps NULL semantics)
    let quota: Option<i64> = sqlx::query_scalar(
        "SELECT CAST(SUM(storage_quota) AS BIGINT) FROM users WHERE storage_quota IS NOT NULL")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(quota, None);
    let _ = &uid;
}
