use pichost_core::storage::local::LocalStorage;
use pichost_core::storage::StorageBackend;
use tempfile::TempDir;

fn setup() -> (LocalStorage, TempDir) {
    let dir = TempDir::new().unwrap();
    (LocalStorage::new(dir.path().to_path_buf(), "http://localhost/u".into()), dir)
}

#[tokio::test]
async fn test_put_and_get() {
    let (s, _) = setup();
    s.put("a.png", b"hello", "image/png").await.unwrap();
    assert_eq!(s.get("a.png").await.unwrap(), b"hello");
}

#[tokio::test]
async fn test_delete() {
    let (s, _) = setup();
    s.put("x", b"d", "text/plain").await.unwrap();
    s.delete("x").await.unwrap();
    assert!(s.get("x").await.is_err());
}

#[tokio::test]
async fn test_exists() {
    let (s, _) = setup();
    assert!(!s.exists("n").await.unwrap());
    s.put("y", b"d", "text/plain").await.unwrap();
    assert!(s.exists("y").await.unwrap());
}

#[tokio::test]
async fn test_public_url() {
    let dir = TempDir::new().unwrap();
    let s = LocalStorage::new(dir.path().into(), "http://localhost/u".into());
    assert_eq!(s.public_url("x.png"), "http://localhost/u/x.png");
}
