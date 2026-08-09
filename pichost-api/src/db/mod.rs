//! DB pool helpers — shared implementation lives in `pichost-core::db`.
pub use pichost_core::db::{
    create_pg_pool, create_sqlite_pool, run_pg_migrations, run_sqlite_migrations, DbRow,
    DbQueryResult, DbType, PG_MIGRATOR, SQLITE_MIGRATOR,
};


