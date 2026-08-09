//! DB pool helpers — shared implementation lives in `pichost-core::db`.
#[allow(unused_imports)] // bin crate: keep surface uniform with pichost-api for T9a
pub use pichost_core::db::{
    create_pg_pool, create_sqlite_pool, run_pg_migrations, run_sqlite_migrations, PG_MIGRATOR,
    SQLITE_MIGRATOR,
};

#[allow(dead_code)] // bin crate: alias kept until T9a genericizes WorkerState<DB>
pub type DbPool = sqlx::PgPool;

