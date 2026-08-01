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
                let not_found = e
                    .as_service_error()
                    .is_some_and(|se| se.is_no_such_key())
                    || e.to_string().contains("NoSuchKey")
                    || e.to_string().contains("NotFound");
                if not_found {
                    StorageError::NotFound(key.to_string())
                } else {
                    StorageError::ReadFailed(e.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config(endpoint: &str) -> RustfsStorageConfig {
        RustfsStorageConfig {
            endpoint: endpoint.into(),
            bucket: "pichost".into(),
            access_key: "minioadmin".into(),
            secret_key: "minioadmin".into(),
            region: "us-east-1".into(),
            use_ssl: false,
            public_endpoint: None,
        }
    }

    #[tokio::test]
    async fn public_url_format() {
        let s = RustfsStorage::new(&config("http://localhost:9000/")).await;
        assert_eq!(
            s.public_url("users/u/file.png"),
            "http://localhost:9000/pichost/users/u/file.png"
        );
    }

    #[tokio::test]
    async fn public_url_no_trailing_slash() {
        let s = RustfsStorage::new(&config("http://localhost:9000")).await;
        assert_eq!(
            s.public_url("k.png"),
            "http://localhost:9000/pichost/k.png"
        );
    }

    #[tokio::test]
    async fn backend_name_is_rustfs() {
        let s = RustfsStorage::new(&config("http://localhost:9000")).await;
        assert_eq!(s.backend_name(), "rustfs");
    }
}
