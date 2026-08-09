use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::PgPool;
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

pub static PG_MIGRATOR: Migrator = sqlx::migrate!("../migrations");
pub static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("../migrations-sqlite");

pub async fn create_pg_pool(url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await
}

// Options carry create_if_missing / foreign_keys / WAL / busy_timeout, so no
// post-connect pragma is needed. `sqlite::memory:` shares ONE in-memory DB
// across pooled connections (sqlx special-cases it with shared_cache=true),
// so the AnyPool-era URL rewrite hack must not be reintroduced.
pub async fn create_sqlite_pool(url: &str, max_connections: u32) -> Result<SqlitePool, sqlx::Error> {
    let connect = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(connect)
        .await
}

pub async fn run_pg_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    PG_MIGRATOR.run(pool).await
}

pub async fn run_sqlite_migrations(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    SQLITE_MIGRATOR.run(pool).await
}


