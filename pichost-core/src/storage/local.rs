use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use crate::error::StorageError;
use super::StorageBackend;

pub struct LocalStorage {
    base_path: PathBuf,
    base_url: String,
}

impl LocalStorage {
    pub fn new(base_path: PathBuf, base_url: String) -> Self {
        Self { base_path, base_url }
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
            fs::create_dir_all(parent).await.map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        }
        fs::write(&path, data).await.map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(key.to_string())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        fs::read(self.full_path(key)).await.map_err(|e| StorageError::ReadFailed(e.to_string()))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        fs::remove_file(self.full_path(key)).await.map_err(|e| StorageError::WriteFailed(e.to_string()))
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.full_path(key).try_exists().map_err(|e| StorageError::ReadFailed(e.to_string()))?)
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), key)
    }

    fn backend_name(&self) -> &str {
        "local"
    }
}
