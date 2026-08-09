use crate::config::DatabaseMode;
use crate::db::{create_pool, run_migrations};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_pool_sqlite_in_memory_runs_migrations() {
    let pool = create_pool("sqlite::memory:", 5, DatabaseMode::Sqlite).await.unwrap();
    run_migrations(&pool, DatabaseMode::Sqlite).await.unwrap();
    let v: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(v, 10);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL"]
async fn create_pool_postgres_url() {
    let pool = create_pool(
        "postgres://pichost:pichost@localhost:5432/pichost", 5, DatabaseMode::Postgres)
        .await.unwrap();
    run_migrations(&pool, DatabaseMode::Postgres).await.unwrap();
}
