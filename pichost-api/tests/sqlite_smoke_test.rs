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
