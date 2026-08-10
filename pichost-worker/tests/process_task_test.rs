//! Smoke test for the pichost-worker library facade (T25).
//!
//! Verifies the public `pichost_worker::process_task` API compiles and is
//! callable in-process with a sqlite pool + an empty-ish `StorageRouter`.
//! The pipeline will fail to read `source_key` from storage (file missing) —
//! that is fine: this test only proves the public API is wired up. Real
//! behaviour is covered by the existing pipeline tests.
use pichost_core::config::AppConfig;
use pichost_core::db::create_sqlite_pool;
use pichost_core::models::TaskPayload;
use pichost_core::storage::local::LocalStorage;
use pichost_core::StorageRouter;
use pichost_worker::process_task;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

fn sample_payload() -> TaskPayload {
    TaskPayload {
        task_id: Uuid::new_v4(),
        image_id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        storage_backend: "local".into(),
        storage_config_id: None,
        storage_backend_name: "local".into(),
        source_key: "k".into(),
        source_mime: "image/png".into(),
        retry_count: 0,
        max_retries: 3,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires storage setup; smoke only"]
async fn process_task_public_api_compiles_and_runs() {
    let pool = create_sqlite_pool("sqlite::memory:", 1).await.unwrap();
    // StorageRouter has no `Default`; register one backend so the router does
    // not panic on `default_backend()` (the read below fails on the missing
    // source key and the error is tolerated).
    let backend = LocalStorage::new(
        PathBuf::from("/tmp/pichost-worker-smoke"),
        "http://localhost".into(),
    );
    let router = StorageRouter::new(
        HashMap::from([(
            "local".to_string(),
            Arc::new(backend) as Arc<dyn pichost_core::storage::StorageBackend>,
        )]),
        "local".to_string(),
    );
    let _ = process_task(&pool, &router, &AppConfig::default(), &sample_payload()).await;
}
