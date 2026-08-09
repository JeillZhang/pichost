use crate::db::{create_pg_pool, create_sqlite_pool, db_error_kind, run_pg_migrations, run_sqlite_migrations, DbErrorKind};
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicU64, Ordering};

static WAL_TEST_SEQ: AtomicU64 = AtomicU64::new(0);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_sqlite_pool_in_memory_runs_migrations() {
    let pool = create_sqlite_pool("sqlite::memory:", 5).await.unwrap();
    run_sqlite_migrations(&pool).await.unwrap();
    let v: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(v, 10);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL"]
async fn create_pg_pool_url_runs_migrations() {
    let pool = create_pg_pool("postgres://pichost:pichost@localhost:5432/pichost", 5)
        .await
        .unwrap();
    run_pg_migrations(&pool).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_sqlite_pool_file_enables_wal() {
    let seq = WAL_TEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let db_path = std::env::temp_dir().join(format!(
        "pichost-wal-test-{}-{seq}.db",
        std::process::id()
    ));
    let url = format!("sqlite://{}", db_path.display());
    {
        let pool = create_sqlite_pool(&url, 1).await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mode, "wal");
    }
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", db_path.display()));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_unique_violation_code_maps() {
    // 真实 sqlite 约束错误驱动（sqlx 无 new_for_test 公共 API）:
    // 在 sqlite::memory: 上建带 UNIQUE 约束的表，插入重复值捕获错误
    let pool = create_sqlite_pool("sqlite::memory:", 1).await.unwrap();
    let pk_err = duplicate_insert(&pool, "t_pk", "id TEXT PRIMARY KEY").await;
    let unique_err = duplicate_insert(&pool, "t_unique", "id TEXT UNIQUE").await;
    if let sqlx::Error::Database(db) = &pk_err {
        eprintln!(
            "sqlite PK constraint error: code={:?} message={}",
            db.code(),
            db.message()
        );
    }
    if let sqlx::Error::Database(db) = &unique_err {
        eprintln!(
            "sqlite UNIQUE constraint error: code={:?} message={}",
            db.code(),
            db.message()
        );
    }
    assert_eq!(db_error_kind(&pk_err), DbErrorKind::UniqueViolation);
    assert_eq!(db_error_kind(&unique_err), DbErrorKind::UniqueViolation);
}

async fn duplicate_insert(pool: &SqlitePool, table: &str, column_def: &str) -> sqlx::Error {
    sqlx::query(&format!("CREATE TABLE {table} ({column_def})"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(&format!("INSERT INTO {table} (id) VALUES ('a')"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(&format!("INSERT INTO {table} (id) VALUES ('a')"))
        .execute(pool)
        .await
        .unwrap_err()
}
