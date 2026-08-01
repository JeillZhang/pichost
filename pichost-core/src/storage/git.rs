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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    type Responder =
        Arc<dyn Fn(&str, &str) -> (u16, Vec<(String, String)>, String) + Send + Sync>;

    fn test_storage(api_base: &str, provider: GitProvider) -> GitStorage {
        GitStorage {
            provider,
            client: reqwest::Client::new(),
            owner: "owner".into(),
            repo: "repo".into(),
            branch: "main".into(),
            path_prefix: None,
            token: "tok".into(),
            raw_base_url: "127.0.0.1:9".into(),
            api_base_url: api_base.into(),
        }
    }

    async fn spawn_mock(responder: Responder) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                let responder = responder.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 8192];
                    let n = sock.read(&mut buf).await.unwrap();
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let mut parts = req.split_whitespace();
                    let method = parts.next().unwrap_or("").to_string();
                    let path = parts.next().unwrap_or("").to_string();
                    let (code, headers, body) = responder(&method, &path);
                    let mut resp = format!(
                        "HTTP/1.1 {code} R\r\nConnection: close\r\nContent-Length: {}\r\n",
                        body.len()
                    );
                    for (k, v) in headers {
                        resp.push_str(&format!("{k}: {v}\r\n"));
                    }
                    resp.push_str("\r\n");
                    resp.push_str(&body);
                    sock.write_all(resp.as_bytes()).await.unwrap();
                });
            }
        });
        format!("http://{addr}")
    }

    async fn closed_url() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{addr}")
    }

    fn github() -> GitStorage {
        GitStorage::new(
            GitProvider::GitHub,
            "owner".into(),
            "repo".into(),
            "main".into(),
            None,
            "tok".into(),
        )
    }

    fn gitcode() -> GitStorage {
        GitStorage::new(
            GitProvider::GitCode,
            "owner".into(),
            "repo".into(),
            "main".into(),
            None,
            "tok".into(),
        )
    }

    #[test]
    fn backend_names() {
        assert_eq!(github().backend_name(), "github");
        assert_eq!(gitcode().backend_name(), "gitcode");
    }

    #[test]
    fn public_url_formats() {
        assert_eq!(
            github().public_url("abc.png"),
            "https://raw.githubusercontent.com/owner/repo/main/abc.png"
        );
        assert_eq!(
            gitcode().public_url("abc.png"),
            "https://raw.gitcode.com/owner/repo/main/abc.png"
        );
    }

    #[test]
    fn contents_url_formats() {
        assert_eq!(
            github().contents_url("p/x.png"),
            "https://api.github.com/repos/owner/repo/contents/p/x.png"
        );
        assert_eq!(
            gitcode().contents_url("p/x.png"),
            "https://api.gitcode.com/api/v5/repos/owner/repo/contents/p/x.png"
        );
    }

    #[test]
    fn mime_to_ext_mapping() {
        let cases = [
            ("image/png", "png"),
            ("image/jpeg", "jpg"),
            ("image/gif", "gif"),
            ("image/webp", "webp"),
            ("image/svg+xml", "svg"),
            ("image/avif", "avif"),
            ("image/bmp", "bmp"),
            ("text/plain", "bin"),
        ];
        for (mime, ext) in cases {
            assert_eq!(GitStorage::mime_to_ext(mime), ext);
        }
    }

    #[test]
    fn build_commit_message_format() {
        assert_eq!(GitStorage::build_commit_message("abc"), "Upload abc");
    }

    #[test]
    fn github_no_size_limit() {
        let s = github();
        let big = vec![0u8; 20 * 1024 * 1024 + 1];
        assert!(s.check_gitcode_size_limit(&big).is_ok());
    }

    #[test]
    fn gitcode_size_limit() {
        let s = gitcode();
        let ok = vec![0u8; 20 * 1024 * 1024];
        assert!(s.check_gitcode_size_limit(&ok).is_ok());
        let big = vec![0u8; 20 * 1024 * 1024 + 1];
        let err = s.check_gitcode_size_limit(&big).unwrap_err();
        assert!(matches!(err, StorageError::PayloadTooLarge(_)));
    }

    #[test]
    fn build_contents_body_json() {
        let s = github();
        let body = s.build_contents_body("k", b"hi");
        assert_eq!(body["message"], "Upload k");
        assert_eq!(body["content"], BASE64.encode(b"hi"));
        assert_eq!(body["branch"], "main");
    }

    #[test]
    fn build_path_pattern() {
        let s = github();
        let path = s.build_path("key123", "png");
        assert!(path.contains("key123"));
        assert!(path.ends_with(".png"));
        let date = Utc::now().format("%Y/%m/%d").to_string();
        assert!(path.contains(&date));
    }

    #[test]
    fn build_path_uses_custom_prefix() {
        let s = GitStorage::new(
            GitProvider::GitHub,
            "o".into(),
            "r".into(),
            "main".into(),
            Some("myprefix".into()),
            "t".into(),
        );
        assert!(s.build_path("k", "jpg").starts_with("myprefix/"));
    }

    #[test]
    fn provider_derive_eq_and_clone() {
        assert_eq!(GitProvider::GitHub, GitProvider::GitHub);
        assert_ne!(GitProvider::GitHub, GitProvider::GitCode);
        let c = GitProvider::GitCode.clone();
        assert_eq!(c, GitProvider::GitCode);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_success_github_uses_put() {
        let base = spawn_mock(Arc::new(|method, path| {
            assert_eq!(method, "PUT");
            assert!(path.contains("/repos/owner/repo/contents/"));
            (201, vec![], "{}".into())
        }))
        .await;
        let s = test_storage(&base, GitProvider::GitHub);
        let path = s.put("key1", b"data", "image/png").await.unwrap();
        assert!(path.contains("key1") && path.ends_with(".png"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_success_status_200() {
        let base = spawn_mock(Arc::new(|_, _| (200, vec![], "{}".into()))).await;
        let s = test_storage(&base, GitProvider::GitHub);
        assert!(s.put("k2", b"d", "image/jpeg").await.unwrap().ends_with(".jpg"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_gitcode_uses_post() {
        let base = spawn_mock(Arc::new(|method, path| {
            assert_eq!(method, "POST");
            assert!(path.contains("/repos/owner/repo/contents/"));
            (201, vec![], "{}".into())
        }))
        .await;
        let s = test_storage(&base, GitProvider::GitCode);
        assert!(s.put("k3", b"d", "image/png").await.unwrap().contains("k3"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_rate_limited_reports_retry_after() {
        let base = spawn_mock(Arc::new(|_, _| {
            (429, vec![("retry-after".into(), "120".into())], "{}".into())
        }))
        .await;
        let s = test_storage(&base, GitProvider::GitHub);
        let err = s.put("k4", b"d", "image/png").await.unwrap_err();
        match err {
            StorageError::WriteFailed(m) => {
                assert!(m.contains("速率受限") && m.contains("120"));
            }
            other => panic!("expected WriteFailed, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_rate_limited_defaults_60() {
        let base = spawn_mock(Arc::new(|_, _| (429, vec![], "{}".into()))).await;
        let s = test_storage(&base, GitProvider::GitHub);
        let err = s.put("k5", b"d", "image/png").await.unwrap_err();
        match err {
            StorageError::WriteFailed(m) => assert!(m.contains("60")),
            other => panic!("expected WriteFailed, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_api_error_includes_status_and_body() {
        let base = spawn_mock(Arc::new(|_, _| (500, vec![], "boom".into()))).await;
        let s = test_storage(&base, GitProvider::GitHub);
        let err = s.put("k6", b"d", "image/png").await.unwrap_err();
        match err {
            StorageError::WriteFailed(m) => {
                assert!(m.contains("Git API 错误") && m.contains("500") && m.contains("boom"));
            }
            other => panic!("expected WriteFailed, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_network_error_maps_to_write_failed() {
        let base = closed_url().await;
        let s = test_storage(&base, GitProvider::GitHub);
        let err = s.put("k7", b"d", "image/png").await.unwrap_err();
        assert!(matches!(err, StorageError::WriteFailed(_)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_gitcode_oversized_rejected() {
        let base =
            spawn_mock(Arc::new(|_, _| unreachable!("no request expected"))).await;
        let s = test_storage(&base, GitProvider::GitCode);
        let big = vec![0u8; 20 * 1024 * 1024 + 1];
        let err = s.put("big", &big, "image/png").await.unwrap_err();
        assert!(matches!(err, StorageError::PayloadTooLarge(_)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_network_error_maps_to_read_failed() {
        let base = closed_url().await;
        let s = test_storage(&base, GitProvider::GitHub);
        let err = s.get("abc").await.unwrap_err();
        assert!(matches!(err, StorageError::ReadFailed(_)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn delete_existing_file_gets_sha_then_deletes() {
        let base = spawn_mock(Arc::new(|method, _| {
            if method == "DELETE" {
                (200, vec![], "{}".into())
            } else {
                (200, vec![], r#"{"sha":"abc123"}"#.into())
            }
        }))
        .await;
        let s = test_storage(&base, GitProvider::GitHub);
        s.delete("abc.png").await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn delete_missing_file_is_ok() {
        let base = spawn_mock(Arc::new(|_, _| (404, vec![], "{}".into()))).await;
        let s = test_storage(&base, GitProvider::GitHub);
        s.delete("gone.png").await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn delete_without_sha_fails() {
        let base = spawn_mock(Arc::new(|_, _| (200, vec![], "{}".into()))).await;
        let s = test_storage(&base, GitProvider::GitHub);
        let err = s.delete("nsha.png").await.unwrap_err();
        match err {
            StorageError::WriteFailed(m) => assert!(m.contains("获取文件SHA失败")),
            other => panic!("expected WriteFailed, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn delete_failure_reports_status() {
        let base = spawn_mock(Arc::new(|method, _| {
            if method == "DELETE" {
                (500, vec![], "{}".into())
            } else {
                (200, vec![], r#"{"sha":"abc123"}"#.into())
            }
        }))
        .await;
        let s = test_storage(&base, GitProvider::GitHub);
        let err = s.delete("fail.png").await.unwrap_err();
        match err {
            StorageError::WriteFailed(m) => assert!(m.contains("删除失败")),
            other => panic!("expected WriteFailed, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exists_true_on_success() {
        let base = spawn_mock(Arc::new(|_, path| {
            assert!(path.contains("/repos/owner/repo/contents/"));
            (200, vec![], "{}".into())
        }))
        .await;
        let s = test_storage(&base, GitProvider::GitHub);
        assert!(s.exists("here.png").await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exists_false_on_404() {
        let base = spawn_mock(Arc::new(|_, _| (404, vec![], "{}".into()))).await;
        let s = test_storage(&base, GitProvider::GitHub);
        assert!(!s.exists("gone.png").await.unwrap());
    }
}
