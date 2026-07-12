use pichost_core::config::RustfsStorageConfig;
use pichost_core::storage::s3::RustfsStorage;
use pichost_core::storage::StorageBackend;

/// Helper: read config from environment or skip.
fn get_config() -> Option<RustfsStorageConfig> {
    let endpoint = std::env::var("PICHOST_STORAGE_RUSTFS_ENDPOINT").ok()?;
    let bucket = std::env::var("PICHOST_STORAGE_RUSTFS_BUCKET").ok()?;
    let access_key = std::env::var("PICHOST_STORAGE_RUSTFS_ACCESS_KEY").ok()?;
    let secret_key = std::env::var("PICHOST_STORAGE_RUSTFS_SECRET_KEY").ok()?;

    Some(RustfsStorageConfig {
        endpoint,
        bucket,
        access_key,
        secret_key,
        region: "us-east-1".to_string(),
        use_ssl: false,
        public_endpoint: None,
    })
}

#[tokio::test]
#[ignore = "requires running S3-compatible service (set PICHOST_STORAGE_RUSTFS_*)"]
async fn test_put_and_get() {
    let config = get_config().expect("set PICHOST_STORAGE_RUSTFS_* env vars");
    let storage = RustfsStorage::new(&config).await;
    let key = "test/hello.txt";

    storage.put(key, b"hello world", "text/plain").await.unwrap();
    let data = storage.get(key).await.unwrap();
    assert_eq!(data, b"hello world");

    storage.delete(key).await.unwrap();
}

#[tokio::test]
#[ignore = "requires running S3-compatible service"]
async fn test_exists() {
    let config = get_config().expect("set PICHOST_STORAGE_RUSTFS_* env vars");
    let storage = RustfsStorage::new(&config).await;
    let key = "test/exists_check.txt";

    assert!(!storage.exists(key).await.unwrap());
    storage.put(key, b"exists", "text/plain").await.unwrap();
    assert!(storage.exists(key).await.unwrap());

    storage.delete(key).await.unwrap();
    assert!(!storage.exists(key).await.unwrap());
}

#[tokio::test]
#[ignore = "requires running S3-compatible service"]
async fn test_get_not_found() {
    let config = get_config().expect("set PICHOST_STORAGE_RUSTFS_* env vars");
    let storage = RustfsStorage::new(&config).await;

    let err = storage.get("test/nonexistent.txt").await.unwrap_err();
    assert!(matches!(err, pichost_core::error::StorageError::NotFound(_)));
}

#[tokio::test]
async fn test_public_url_format_no_custom_endpoint() {
    let config = RustfsStorageConfig {
        endpoint: "http://localhost:9000".to_string(),
        bucket: "pichost".to_string(),
        access_key: "minioadmin".to_string(),
        secret_key: "minioadmin".to_string(),
        region: "us-east-1".to_string(),
        use_ssl: false,
        public_endpoint: None,
    };
    let storage = RustfsStorage::new(&config).await;
    let url = storage.public_url("users/uuid/file.png");
    assert_eq!(url, "http://localhost:9000/pichost/users/uuid/file.png");
}

#[tokio::test]
async fn test_backend_name() {
    let config = RustfsStorageConfig {
        endpoint: "http://localhost:9000".to_string(),
        bucket: "pichost".to_string(),
        access_key: "minioadmin".to_string(),
        secret_key: "minioadmin".to_string(),
        region: "us-east-1".to_string(),
        use_ssl: false,
        public_endpoint: None,
    };
    let storage = RustfsStorage::new(&config).await;
    assert_eq!(storage.backend_name(), "rustfs");
}
