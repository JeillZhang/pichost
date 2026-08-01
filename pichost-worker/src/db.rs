use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

pub type DbPool = PgPool;

pub async fn create_pool(url: &str, max_connections: u32) -> Result<DbPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await
}

pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../migrations").run(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DB_URL: &str = "postgres://pichost:pichost@localhost:5432/pichost";

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "requires running PostgreSQL and Redis"]
    async fn test_create_pool_and_run_migrations() {
        let pool = create_pool(TEST_DB_URL, 2).await.expect("pg pool");
        run_migrations(&pool).await.expect("migrations apply");
        sqlx::query("SELECT 1").execute(&pool).await.expect("query works");
    }
}
