use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;

use crate::config::RustfsStorageConfig;
use crate::error::StorageError;
use super::StorageBackend;

pub struct RustfsStorage {
    client: Client,
    bucket: String,
    endpoint: String,
}

impl RustfsStorage {
    pub async fn new(config: &RustfsStorageConfig) -> Self {
        let creds = Credentials::new(
            config.access_key.clone(),
            config.secret_key.clone(),
            None,
            None,
            "rustfs",
        );

        let endpoint = config.endpoint.trim_end_matches('/').to_string();

        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(config.region.clone()))
            .credentials_provider(creds)
            .endpoint_url(&endpoint)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(true)
            .build();

        Self {
            client: Client::from_conf(s3_config),
            bucket: config.bucket.clone(),
            endpoint,
        }
    }
}

#[async_trait]
impl StorageBackend for RustfsStorage {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<String, StorageError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(data.to_vec()))
            .send()
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(key.to_string())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("NoSuchKey") || err_str.contains("NotFound") {
                    StorageError::NotFound(key.to_string())
                } else {
                    StorageError::ReadFailed(err_str)
                }
            })?;

        output
            .body
            .collect()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| StorageError::ReadFailed(e.to_string()))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let result = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(err) if err.as_service_error().is_some_and(|e| e.is_not_found()) => Ok(false),
            Err(e) => Err(StorageError::ReadFailed(e.to_string())),
        }
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}/{}", self.endpoint, self.bucket, key)
    }

    fn backend_name(&self) -> &str {
        "rustfs"
    }
}
