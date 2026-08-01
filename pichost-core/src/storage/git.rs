use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use reqwest::header::{AUTHORIZATION, USER_AGENT};

use super::StorageBackend;
use crate::error::StorageError;

#[derive(Debug, Clone, PartialEq)]
pub enum GitProvider {
    GitHub,
    GitCode,
}

pub struct GitStorage {
    provider: GitProvider,
    client: reqwest::Client,
    owner: String,
    repo: String,
    branch: String,
    path_prefix: Option<String>,
    token: String,
    raw_base_url: String,
    api_base_url: String,
}

impl GitStorage {
    const GITCODE_MAX_CONTENTS_BYTES: usize = 20 * 1024 * 1024;

    pub fn new(
        provider: GitProvider,
        owner: String,
        repo: String,
        branch: String,
        path_prefix: Option<String>,
        token: String,
    ) -> Self {
        let (raw_base_url, api_base_url) = match &provider {
            GitProvider::GitHub => (
                "raw.githubusercontent.com".to_string(),
                "https://api.github.com".to_string(),
            ),
            GitProvider::GitCode => (
                "raw.gitcode.com".to_string(),
                "https://api.gitcode.com/api/v5".to_string(),
            ),
        };

        Self {
            provider,
            client: reqwest::Client::new(),
            owner,
            repo,
            branch,
            path_prefix,
            token,
            raw_base_url,
            api_base_url,
        }
    }

    fn build_path(&self, key: &str, ext: &str) -> String {
        let now = Utc::now();
        let prefix = self.path_prefix.as_deref().unwrap_or("pichost");
        format!("{}/{}/{}.{}", prefix, now.format("%Y/%m/%d"), key, ext,)
    }

    fn contents_url(&self, path: &str) -> String {
        format!(
            "{}/repos/{}/{}/contents/{}",
            self.api_base_url, self.owner, self.repo, path
        )
    }

    fn raw_url(&self, path: &str) -> String {
        format!(
            "https://{}/{}/{}/{}/{}",
            self.raw_base_url, self.owner, self.repo, self.branch, path
        )
    }

    fn mime_to_ext(mime_type: &str) -> &str {
        match mime_type {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/svg+xml" => "svg",
            "image/avif" => "avif",
            "image/bmp" => "bmp",
            _ => "bin",
        }
    }

    fn build_commit_message(key: &str) -> String {
        format!("Upload {}", key)
    }

    fn check_gitcode_size_limit(&self, data: &[u8]) -> Result<(), StorageError> {
        if self.provider == GitProvider::GitCode && data.len() > Self::GITCODE_MAX_CONTENTS_BYTES {
            return Err(StorageError::PayloadTooLarge(
                "文件超过GitCode 20MB限制，请改用本地存储或GitHub".into(),
            ));
        }
        Ok(())
    }

    fn build_contents_body(&self, key: &str, data: &[u8]) -> serde_json::Value {
        serde_json::json!({
            "message": Self::build_commit_message(key),
            "content": BASE64.encode(data),
            "branch": self.branch,
        })
    }

    async fn send_contents_request(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, StorageError> {
        let method = match self.provider {
            GitProvider::GitHub => reqwest::Method::PUT,
            GitProvider::GitCode => reqwest::Method::POST,
        };
        self.client
            .request(method, self.contents_url(path))
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, "pichost/0.15.0")
            .json(body)
            .send()
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))
    }

    async fn map_put_response(resp: reqwest::Response, path: &str) -> Result<String, StorageError> {
        if resp.status().is_success() || resp.status().as_u16() == 201 {
            return Ok(path.to_string());
        }
        if resp.status().as_u16() == 429 {
            let retry = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("60");
            return Err(StorageError::WriteFailed(format!(
                "速率受限，请在{}秒后重试",
                retry
            )));
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(StorageError::WriteFailed(format!(
            "Git API 错误 ({}): {}",
            status, body
        )))
    }
}

#[async_trait]
impl StorageBackend for GitStorage {
    fn backend_name(&self) -> &str {
        match self.provider {
            GitProvider::GitHub => "github",
            GitProvider::GitCode => "gitcode",
        }
    }

    async fn put(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, StorageError> {
        let path = self.build_path(key, Self::mime_to_ext(content_type));
        self.check_gitcode_size_limit(data)?;

        let body = self.build_contents_body(key, data);
        let resp = self.send_contents_request(&path, &body).await?;
        Self::map_put_response(resp, &path).await
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let url = self.raw_url(key);

        let resp = self
            .client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, "pichost/0.15.0")
            .send()
            .await
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;

        if resp.status().is_success() {
            resp.bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| StorageError::ReadFailed(e.to_string()))
        } else if resp.status().as_u16() == 404 {
            Err(StorageError::NotFound(key.to_string()))
        } else {
            Err(StorageError::ReadFailed(format!(
                "Git API 错误 ({})",
                resp.status()
            )))
        }
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        // Step 1: get SHA
        let contents_url = self.contents_url(key);
        let resp = self
            .client
            .get(&contents_url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, "pichost/0.15.0")
            .query(&[("ref", &self.branch)])
            .send()
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;

        if resp.status().as_u16() == 404 {
            return Ok(()); // already gone
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;

        let sha = json["sha"]
            .as_str()
            .ok_or_else(|| StorageError::WriteFailed("获取文件SHA失败".into()))?;

        // Step 2: delete
        let resp = self
            .client
            .delete(&contents_url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, "pichost/0.15.0")
            .json(&serde_json::json!({
                "message": format!("Delete {}", key),
                "sha": sha,
                "branch": self.branch,
            }))
            .send()
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;

        if resp.status().is_success() || resp.status().as_u16() == 404 {
            Ok(())
        } else {
            Err(StorageError::WriteFailed(format!(
                "删除失败 ({})",
                resp.status()
            )))
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let url = self.contents_url(key);
        let resp = self
            .client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, "pichost/0.15.0")
            .query(&[("ref", &self.branch)])
            .send()
            .await
            .map_err(|_| StorageError::ReadFailed("请求失败".into()))?;

        Ok(resp.status().is_success())
    }

    fn public_url(&self, key: &str) -> String {
        self.raw_url(key)
    }
}
