use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Database, FromRow, PgPool, Postgres, Sqlite, SqlitePool};
use std::str::FromStr;
use std::time::Duration;

pub static PG_MIGRATOR: Migrator = sqlx::migrate!("../migrations");
pub static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("../migrations-sqlite");

/// Marker for the two concrete drivers PicHost runs on (Postgres + Sqlite).
pub trait DbType: Database {}
impl DbType for Postgres {}
impl DbType for Sqlite {}

/// Compile-time evidence that `T` decodes from a full row on driver `DB`.
///
/// sqlx 0.8 implements `Type`/`Encode`/`Decode` per driver for primitives
/// (Uuid, i64, String, chrono, …) with no blanket impls over `Database`, so
/// generic fns must carry explicit bounds; `DbRow` names the row evidence
/// and raw `for<'q> X: sqlx::Encode<'q, DB> + …` clauses name the scalars.
pub trait DbRow<DB: DbType>: for<'r> FromRow<'r, DB::Row> {}
impl<DB: DbType, T> DbRow<DB> for T where T: for<'r> FromRow<'r, DB::Row> {}

/// Generic affected-row accessor. sqlx 0.8 removed the `QueryResult` trait,
/// leaving only per-driver inherent `rows_affected()` methods.
pub trait DbQueryResult {
    fn affected(&self) -> u64;
}
impl DbQueryResult for sqlx::postgres::PgQueryResult {
    fn affected(&self) -> u64 {
        self.rows_affected()
    }
}
impl DbQueryResult for sqlx::sqlite::SqliteQueryResult {
    fn affected(&self) -> u64 {
        self.rows_affected()
    }
}

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
pub async fn create_sqlite_pool(
    url: &str,
    max_connections: u32,
) -> Result<SqlitePool, sqlx::Error> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbErrorKind {
    UniqueViolation,
    Other,
}

/// Maps driver error codes onto dialect-neutral kinds:
/// PG SQLSTATE 23505 (unique_violation) and SQLite extended codes 2067
/// (SQLITE_CONSTRAINT_UNIQUE), 1555 (SQLITE_CONSTRAINT_PRIMARYKEY) plus
/// base 19 (SQLITE_CONSTRAINT) → UniqueViolation.
pub fn db_error_kind(err: &sqlx::Error) -> DbErrorKind {
    match err {
        sqlx::Error::Database(db) => {
            let code = db.code().map(|c| c.to_string()).unwrap_or_default();
            if code == "23505" || code == "2067" || code == "19" || code == "1555" {
                DbErrorKind::UniqueViolation
            } else {
                DbErrorKind::Other
            }
        }
        _ => DbErrorKind::Other,
    }
}
