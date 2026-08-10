use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

static MIGRATOR: Migrator = sqlx::migrate!("../migrations-sqlite");

async fn sqlite_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_apply_users_images() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.expect("migrations apply");
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('users','images')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 2);
    let cols: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('users') WHERE name IN ('id','username','password_hash')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(cols, 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_processing_and_tasks() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('images') WHERE name IN ('thumbnail_key','webp_url')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 2);
    let t: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='upload_tasks'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(t, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_quota_and_index() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('users') WHERE name='storage_quota'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
    let idx: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='idx_images_user_filename'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(idx, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_storage_configs() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let t: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('oauth_accounts','user_storage_configs')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(t, 2);
    let idx: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='idx_default_per_user'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(idx, 1);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('user_storage_configs') WHERE name IN ('config','is_default')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_all_ten() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let ver: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ver, 10);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('users') WHERE name='watermark_config'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
    let cat: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='categories'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cat, 1);
}
