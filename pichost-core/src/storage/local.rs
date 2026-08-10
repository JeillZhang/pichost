use super::StorageBackend;
use crate::error::StorageError;
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;

pub struct LocalStorage {
    base_path: PathBuf,
    base_url: String,
}

impl LocalStorage {
    pub fn new(base_path: PathBuf, base_url: String) -> Self {
        Self {
            base_path,
            base_url,
        }
    }

    fn full_path(&self, key: &str) -> PathBuf {
        self.base_path.join(key)
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn put(&self, key: &str, data: &[u8], _ct: &str) -> Result<String, StorageError> {
        let path = self.full_path(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        }
        fs::write(&path, data)
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(key.to_string())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        fs::read(self.full_path(key))
            .await
            .map_err(|e| StorageError::ReadFailed(e.to_string()))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        fs::remove_file(self.full_path(key))
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self
            .full_path(key)
            .try_exists()
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?)
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), key)
    }

    fn backend_name(&self) -> &str {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage() -> (LocalStorage, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let s = LocalStorage::new(dir.path().to_path_buf(), "http://cdn.local/".into());
        (s, dir)
    }

    #[tokio::test]
    async fn backend_name_is_local() {
        let (s, _d) = storage();
        assert_eq!(s.backend_name(), "local");
    }

    #[tokio::test]
    async fn put_nested_key_creates_parent_dirs() {
        let (s, d) = storage();
        let key = "u/1/2026/01/01/ab12cd.png";
        let url = s.put(key, b"data", "image/png").await.unwrap();
        assert_eq!(url, key);
        assert!(d.path().join(key).exists());
    }

    #[tokio::test]
    async fn get_roundtrip() {
        let (s, _d) = storage();
        s.put("a/b.txt", b"hello", "text/plain").await.unwrap();
        let data = s.get("a/b.txt").await.unwrap();
        assert_eq!(data, b"hello");
    }

    #[tokio::test]
    async fn get_missing_returns_read_failed() {
        let (s, _d) = storage();
        let err = s.get("nope.txt").await.unwrap_err();
        assert!(matches!(err, StorageError::ReadFailed(_)));
    }

    #[tokio::test]
    async fn delete_missing_returns_error() {
        let (s, _d) = storage();
        let err = s.delete("nope.txt").await.unwrap_err();
        assert!(matches!(err, StorageError::WriteFailed(_)));
    }

    #[tokio::test]
    async fn delete_existing_succeeds() {
        let (s, _d) = storage();
        s.put("f.txt", b"x", "text/plain").await.unwrap();
        s.delete("f.txt").await.unwrap();
        assert!(!s.exists("f.txt").await.unwrap());
    }

    #[test]
    fn public_url_trims_trailing_slash() {
        let (s, _d) = storage();
        assert_eq!(s.public_url("abc.png"), "http://cdn.local/abc.png");
    }

    #[test]
    fn public_url_without_trailing_slash() {
        let dir = tempfile::tempdir().unwrap();
        let s = LocalStorage::new(dir.path().to_path_buf(), "http://cdn.local".into());
        assert_eq!(s.public_url("abc.png"), "http://cdn.local/abc.png");
    }
}
