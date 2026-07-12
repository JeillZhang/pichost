use async_trait::async_trait;
use crate::error::StorageError;

pub mod local;
pub mod s3;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<String, StorageError>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;
    fn public_url(&self, key: &str) -> String;
    fn backend_name(&self) -> &str;
}
