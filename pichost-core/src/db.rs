use crate::config::DatabaseMode;
use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::migrate::Migrator;
use sqlx::AnyPool;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub type DbPool = AnyPool;

pub static PG_MIGRATOR: Migrator = sqlx::migrate!("../migrations");
pub static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("../migrations-sqlite");

static MEM_DB_SEQ: AtomicU64 = AtomicU64::new(0);

pub async fn create_pool(
    url: &str,
    max_connections: u32,
    mode: DatabaseMode,
) -> Result<DbPool, sqlx::Error> {
    sqlx::any::install_default_drivers(); // 必须：AnyPool 无驱动时 connect 会 panic
    let opts = AnyPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5));
    let connect = match mode {
        DatabaseMode::Postgres => AnyConnectOptions::from_str(url)?,
        DatabaseMode::Sqlite => AnyConnectOptions::from_str(&sqlite_url(url))?,
    };
    let pool = opts.connect_with(connect).await?;
    if mode == DatabaseMode::Sqlite {
        enable_wal(&pool, url).await?;
    }
    Ok(pool)
}

/// WAL is a persistent property of the sqlite database file, so it can be
/// restored with a single post-connect statement (AnyConnectOptions cannot
/// carry it). Skipped for in-memory databases (`mode=memory`), where WAL is
/// meaningless. Foreign keys are already enforced by sqlx by default.
async fn enable_wal(pool: &DbPool, url: &str) -> Result<(), sqlx::Error> {
    if sqlite_url(url).contains("mode=memory") {
        return Ok(());
    }
    sqlx::query("PRAGMA journal_mode=WAL").execute(pool).await?;
    Ok(())
}

pub async fn run_migrations(
    pool: &DbPool,
    mode: DatabaseMode,
) -> Result<(), sqlx::migrate::MigrateError> {
    match mode {
        DatabaseMode::Postgres => PG_MIGRATOR.run(pool).await,
        DatabaseMode::Sqlite => SQLITE_MIGRATOR.run(pool).await,
    }
}

/// Rewrites a sqlite URL into one `AnyConnectOptions` can carry, since it is
/// opaque to driver-specific flags:
/// - `sqlite::memory:` becomes a uniquely named shared-cache in-memory DB; a
///   plain `:memory:` would give every pooled connection its own empty DB.
/// - otherwise `mode=rwc` is appended to replicate `create_if_missing(true)`
///   (the sqlx default would reject a not-yet-existing file database).
///   Foreign keys are enforced by sqlx by default; WAL is not expressible here.
fn sqlite_url(url: &str) -> String {
    let rest = url.trim_start_matches("sqlite://").trim_start_matches("sqlite:");
    if rest.starts_with(":memory:") {
        let seq = MEM_DB_SEQ.fetch_add(1, Ordering::Relaxed);
        return format!("sqlite:file:pichost-mem-{seq}?mode=memory&cache=shared");
    }
    if url.contains("mode=") {
        url.to_string()
    } else if url.contains('?') {
        format!("{url}&mode=rwc")
    } else {
        format!("{url}?mode=rwc")
    }
}
