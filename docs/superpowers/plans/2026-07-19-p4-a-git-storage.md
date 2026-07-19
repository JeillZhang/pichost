# P4-A: Git Storage Backends + Multi-Backend Upload — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add GitHub and GitCode as storage backends, allow per-upload backend selection (max 2), and gallery filtering by storage backend.

**Architecture:** Implement `GitStorage` (single struct for both GitHub/GitCode via `GitProvider` enum) using direct HTTP API calls. Add `user_storage_configs` table for per-user backend configurations. Inject `storage_config_id` into the upload pipeline and Gallery queries. Changes span pichost-core (models, trait impl, router), pichost-api (routes, services, DB queries), pichost-worker (task payload), and web-ui (Settings, Dashboard, Gallery).

**Tech Stack:** Rust 1.96+ (Axum, sqlx, reqwest, aes-gcm, base64), React 19 (TypeScript, TanStack Query, Zustand), PostgreSQL 18.

## Global Constraints

- Rust edition 2021, `rustfmt` + `clippy -- -D warnings` (zero warnings required)
- Functions ≤ 50 lines, lines ≤ 120 chars
- sqlx runtime-only queries (`query_as`, `query_scalar` — no `query!` macro)
- Frontend CSS via `var(--color-*)` tokens from `theme.css`, glassmorphism patterns
- Commit messages in English
- All API changes in `/api/v1/` namespace
- Token encryption uses independent `PICHOST_AUTH_TOKEN_ENCRYPTION_KEY` (32 bytes, base64/hex)
- Migrations auto-apply at API startup via `sqlx::migrate!()`

---

## File Structure

| File | Action | Purpose |
|------|--------|---------|
| `pichost-core/src/config.rs` | Modify | Add `TokenEncryptionKey` config field |
| `pichost-core/src/models.rs` | Modify | Add `UserStorageConfig`, `GitConfigDetail` structs; add `storage_config_id` to `Image` |
| `pichost-core/src/storage/git.rs` | **Create** | `GitStorage` struct + `StorageBackend` trait impl |
| `pichost-core/src/storage/mod.rs` | Modify | Add `pub mod git;` |
| `pichost-core/src/storage/router.rs` | Modify | Add `for_config()`, `get_or_create_git()` methods |
| `pichost-core/src/crypto.rs` | **Create** | Token encrypt/decrypt utility using AES-256-GCM |
| `migrations/0008_user_storage_configs.sql` | **Create** | `user_storage_configs` table + ALTER images |
| `pichost-api/src/routes/storage_configs.rs` | **Create** | CRUD endpoints for storage configurations |
| `pichost-api/src/services/upload.rs` | Modify | Multi-backend upload pipeline |
| `pichost-api/src/routes/images.rs` | Modify | Gallery filtering, upload handler changes |
| `pichost-api/src/main.rs` | Modify | Register new routes |
| `pichost-worker/src/queue.rs` | Modify | Add fields to `TaskPayload` |
| `web-ui/src/pages/Settings.tsx` | Modify | Storage config management section |
| `web-ui/src/pages/Dashboard.tsx` | Modify | Multi-backend selector above DropZone |
| `web-ui/src/pages/Gallery.tsx` | Modify | Backend filter dropdown |
| `web-ui/src/components/UploadCard.tsx` | Modify | Show backend name |
| `web-ui/src/api/client.ts` | Modify | New API functions |
| `web-ui/src/stores/auth.ts` | Modify | Add `UserStorageConfig` type |

---

### Task 1: Database Migration

**Files:**
- Create: `migrations/0008_user_storage_configs.sql`
- Modify: `pichost-core/src/models.rs` (add structs)
- Modify: `pichost-core/src/config.rs` (add TokenEncryptionKey)

**Interfaces:**
- Produces: `UserStorageConfig` struct, `GitConfigDetail` struct, `Image.storage_config_id` field, `Config.token_encryption_key` field

- [ ] **Step 1: Create migration file**

```sql
-- migrations/0008_user_storage_configs.sql

CREATE TABLE user_storage_configs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        VARCHAR(64) NOT NULL,
    provider    VARCHAR(16) NOT NULL,
    is_default  BOOLEAN NOT NULL DEFAULT false,
    config      JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(user_id, name)
);

CREATE UNIQUE INDEX idx_default_per_user
    ON user_storage_configs(user_id) WHERE is_default = true;

ALTER TABLE images
    ADD COLUMN storage_config_id UUID
    REFERENCES user_storage_configs(id);
```

- [ ] **Step 2: Add model structs to pichost-core/src/models.rs**

After the existing `Image` struct, add:

```rust
/// 用户的存储后端配置
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserStorageConfig {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub provider: String,
    pub is_default: bool,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Git 后端 config JSON 的反序列化结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfigDetail {
    pub token_encrypted: String,
    pub repo: String,
    pub branch: String,
    pub path_prefix: Option<String>,
}

/// API 响应用：掩码 token 的配置视图
#[derive(Debug, Clone, Serialize)]
pub struct UserStorageConfigResponse {
    pub id: Uuid,
    pub name: String,
    pub provider: String,
    pub repo: String,
    pub branch: String,
    pub path_prefix: Option<String>,
    pub is_default: bool,
    pub token_masked: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

Add to the existing `Image` struct:

```rust
pub storage_config_id: Option<Uuid>,
```

Also append to the `ImageRow` tuple type alias in `services/upload.rs` — add `Option<Uuid>` as position 14.

- [ ] **Step 3: Add TokenEncryptionKey to config**

In `pichost-core/src/config.rs`, add to `Config` struct:

```rust
/// AES-256-GCM 密钥，用于加密用户 Git PAT
/// 须 32 字节（base64 或 hex 编码），与 JWT secret 独立
#[serde(default)]
pub token_encryption_key: Option<String>,
```

Default impl returns `None`.

- [ ] **Step 4: Verify migration compiles**

```bash
cargo check -p pichost-core
```

Expected: no errors. The field additions are additive, no existing code should break.

- [ ] **Step 5: Commit**

```bash
git add migrations/0008_user_storage_configs.sql pichost-core/src/models.rs pichost-core/src/config.rs
git commit -m "feat: add UserStorageConfig model and migration 0008"
```

---

### Task 2: Token Encryption Utility

**Files:**
- Create: `pichost-core/src/crypto.rs`
- Modify: `pichost-core/src/lib.rs` (add `pub mod crypto;`)

**Interfaces:**
- Produces: `encrypt_token(plaintext: &str, key: &[u8; 32]) -> Result<String, CryptoError>`
- Produces: `decrypt_token(ciphertext: &str, key: &[u8; 32]) -> Result<String, CryptoError>`
- Produces: `mask_token(token: &str) -> String` (e.g., `"ghp_****abcd"`)

- [ ] **Step 1: Add dependencies to Cargo.toml**

In `pichost-core/Cargo.toml`, add:

```toml
aes-gcm = "0.10"
base64 = "0.22"
rand = "0.8"
```

- [ ] **Step 2: Create pichost-core/src/crypto.rs**

```rust
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("encryption failed: {0}")]
    Encrypt(String),
    #[error("decryption failed: invalid key or corrupted data")]
    Decrypt,
    #[error("invalid key length: expected 32 bytes, got {0}")]
    InvalidKey(usize),
}

const NONCE_SIZE: usize = 12;

/// Encrypt plaintext using AES-256-GCM.
/// Returns base64-encoded "nonce || ciphertext" string.
pub fn encrypt_token(plaintext: &str, key: &[u8; 32]) -> Result<String, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| CryptoError::InvalidKey(key.len()))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| CryptoError::Encrypt(e.to_string()))?;

    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

/// Decrypt base64-encoded "nonce || ciphertext" string.
pub fn decrypt_token(encoded: &str, key: &[u8; 32]) -> Result<String, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| CryptoError::InvalidKey(key.len()))?;

    let combined = BASE64.decode(encoded).map_err(|_| CryptoError::Decrypt)?;

    if combined.len() < NONCE_SIZE + 16 {
        return Err(CryptoError::Decrypt);
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::Decrypt)?;

    String::from_utf8(plaintext).map_err(|_| CryptoError::Decrypt)
}

/// Mask a token for API responses (show first 4 and last 4 chars).
pub fn mask_token(token: &str) -> String {
    if token.len() <= 8 {
        return "****".to_string();
    }
    format!("{}****{}", &token[..4], &token[token.len()-4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        key
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = "ghp_testToken1234567890abcdef";
        let encrypted = encrypt_token(plaintext, &key).unwrap();
        let decrypted = decrypt_token(&encrypted, &key).unwrap();
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn different_keys_fail() {
        let key1 = test_key();
        let key2 = test_key();
        let encrypted = encrypt_token("test", &key1).unwrap();
        assert!(decrypt_token(&encrypted, &key2).is_err());
    }

    #[test]
    fn mask_token_works() {
        assert_eq!(mask_token("ghp_abcdefgh12345678"), "ghp_****5678");
        assert_eq!(mask_token("short"), "****");
    }
}
```

- [ ] **Step 3: Register module**

In `pichost-core/src/lib.rs`, add:
```rust
pub mod crypto;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p pichost-core crypto
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add pichost-core/src/crypto.rs pichost-core/src/lib.rs pichost-core/Cargo.toml
git commit -m "feat: add AES-256-GCM token encryption utility"
```

---

### Task 3: GitStorage Implementation

**Files:**
- Create: `pichost-core/src/storage/git.rs`
- Modify: `pichost-core/src/storage/mod.rs` (add `pub mod git;`)

**Interfaces:**
- Produces: `GitProvider` enum, `GitStorage` struct implementing `StorageBackend`
- Produces: `GitStorage::new(provider, owner, repo, branch, path_prefix, token) -> Self`

- [ ] **Step 1: Add reqwest dependency**

In `pichost-core/Cargo.toml`, add:
```toml
reqwest = { version = "0.12", features = ["json"] }
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 2: Create pichost-core/src/storage/git.rs**

```rust
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, USER_AGENT};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;

use super::{StorageBackend, StorageError};

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
        format!(
            "{}/{}/{}/{}.{}",
            prefix,
            now.format("%Y/%m/%d"),
            key,
            ext,
        )
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
}

#[async_trait]
impl StorageBackend for GitStorage {
    fn backend_name(&self) -> &str {
        match self.provider {
            GitProvider::GitHub => "github",
            GitProvider::GitCode => "gitcode",
        }
    }

    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<String, StorageError> {
        let ext = Self::mime_to_ext(content_type);
        let path = self.build_path(key, ext);
        let base64_content = BASE64.encode(data);
        let commit_msg = Self::build_commit_message(key);

        // GitCode: check size limit, fall back to file upload if needed
        if self.provider == GitProvider::GitCode && data.len() > Self::GITCODE_MAX_CONTENTS_BYTES {
            return Err(StorageError::WriteFailed(
                "文件超过GitCode 20MB限制，请改用本地存储或GitHub".into(),
            ));
        }

        let http_method = match self.provider {
            GitProvider::GitHub => "PUT",
            GitProvider::GitCode => "POST",
        };

        let url = self.contents_url(&path);
        let body = serde_json::json!({
            "message": commit_msg,
            "content": base64_content,
            "branch": self.branch,
        });

        let resp = self
            .client
            .request(
                match http_method {
                    "PUT" => reqwest::Method::PUT,
                    _ => reqwest::Method::POST,
                },
                &url,
            )
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, "pichost/0.15.0")
            .json(&body)
            .send()
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;

        if resp.status().is_success() || resp.status().as_u16() == 201 {
            Ok(self.raw_url(&path))
        } else if resp.status().as_u16() == 429 {
            let retry = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("60");
            Err(StorageError::WriteFailed(format!(
                "速率受限，请在{}秒后重试",
                retry
            )))
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(StorageError::WriteFailed(format!(
                "Git API 错误 ({}): {}",
                resp.status(),
                body
            )))
        }
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
```

- [ ] **Step 3: Register in storage module**

In `pichost-core/src/storage/mod.rs`, add:
```rust
pub mod git;
pub use git::{GitProvider, GitStorage};
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p pichost-core
```

Expected: no errors. Compile-check only, no integration test yet (needs real token).

- [ ] **Step 5: Commit**

```bash
git add pichost-core/src/storage/git.rs pichost-core/src/storage/mod.rs pichost-core/Cargo.toml
git commit -m "feat: implement GitStorage backend for GitHub and GitCode"
```

---

### Task 4: StorageRouter — Dynamic Git Backend Cache

**Files:**
- Modify: `pichost-core/src/storage/router.rs`

**Interfaces:**
- Produces: `StorageRouter::for_config(&self, config: &UserStorageConfig) -> Result<Arc<dyn StorageBackend>, StorageError>`
- Produces: `StorageRouter::get_or_create_git(&self, config: &UserStorageConfig, encryption_key: &[u8; 32]) -> Result<Arc<dyn StorageBackend>, StorageError>`
- Consumes: `UserStorageConfig` from Task 1, `decrypt_token` from Task 2, `GitStorage` from Task 3

- [ ] **Step 1: Modify StorageRouter**

In `pichost-core/src/storage/router.rs`, add the new methods and change `backends` to use `RwLock` for interior mutability:

```rust
use std::sync::RwLock;
use crate::crypto::decrypt_token;
use crate::models::{GitConfigDetail, UserStorageConfig};
use super::git::{GitProvider, GitStorage};

pub struct StorageRouter {
    backends: RwLock<HashMap<String, Arc<dyn StorageBackend>>>,
    default: String,
}

impl StorageRouter {
    /// 根据配置 ID 解析后端（从缓存获取或动态创建 Git 后端）
    pub fn for_config(
        &self,
        config: &UserStorageConfig,
        encryption_key: &[u8; 32],
    ) -> Result<Arc<dyn StorageBackend>, StorageError> {
        if config.provider == "local" {
            return Ok(self.default_backend());
        }

        let cache_key = config.id.to_string();
        {
            let backends = self.backends.read().map_err(|_| {
                StorageError::Config("Router lock poisoned".into())
            })?;
            if let Some(backend) = backends.get(&cache_key) {
                return Ok(Arc::clone(backend));
            }
        }

        self.get_or_create_git(config, encryption_key)
    }

    /// 动态创建 GitStorage 并缓存
    pub fn get_or_create_git(
        &self,
        config: &UserStorageConfig,
        encryption_key: &[u8; 32],
    ) -> Result<Arc<dyn StorageBackend>, StorageError> {
        let detail: GitConfigDetail = serde_json::from_value(config.config.clone())
            .map_err(|e| StorageError::Config(format!("配置解析失败: {}", e)))?;

        let token = decrypt_token(&detail.token_encrypted, encryption_key)
            .map_err(|e| StorageError::Config(format!("Token解密失败: {}", e)))?;

        let provider = match config.provider.as_str() {
            "github" => GitProvider::GitHub,
            "gitcode" => GitProvider::GitCode,
            _ => return Err(StorageError::Config(format!("未知provider: {}", config.provider))),
        };

        let (owner, repo) = detail
            .repo
            .split_once('/')
            .ok_or_else(|| StorageError::Config("仓库格式错误，应为 owner/repo".into()))?;

        let git = Arc::new(GitStorage::new(
            provider,
            owner.to_string(),
            repo.to_string(),
            detail.branch,
            detail.path_prefix,
            token,
        ));

        let mut backends = self.backends.write().map_err(|_| {
            StorageError::Config("Router lock poisoned".into())
        })?;
        backends.insert(config.id.to_string(), Arc::clone(&git));

        Ok(git)
    }

    pub fn evict(&self, config_id: &str) {
        if let Ok(mut backends) = self.backends.write() {
            backends.remove(config_id);
        }
    }
}
```

Update the `new()` and `default_backend()` and `for_backend()` methods to use `RwLock`:

```rust
impl StorageRouter {
    pub fn new(backends: HashMap<String, Arc<dyn StorageBackend>>, default: String) -> Self {
        Self {
            backends: RwLock::new(backends),
            default,
        }
    }

    pub fn for_backend(&self, name: &str) -> Arc<dyn StorageBackend> {
        self.backends
            .read()
            .ok()
            .and_then(|b| b.get(name).cloned())
            .unwrap_or_else(|| self.default_backend())
    }

    pub fn default_backend(&self) -> Arc<dyn StorageBackend> {
        self.backends
            .read()
            .ok()
            .and_then(|b| b.get(&self.default).cloned())
            .expect("default backend must be registered")
    }

    pub fn default_name(&self) -> &str {
        &self.default
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check --workspace
```

Expected: compilation errors in API and Worker due to `RwLock` change — fix call sites by adding `.read().unwrap()` where needed.

- [ ] **Step 3: Commit**

```bash
git add pichost-core/src/storage/router.rs
git commit -m "feat: add dynamic GitStorage caching to StorageRouter"
```

---

### Task 5: Storage Config CRUD API

**Files:**
- Create: `pichost-api/src/routes/storage_configs.rs`
- Modify: `pichost-api/src/main.rs` (register routes)

**Interfaces:**
- Consumes: `UserStorageConfig`, `CryptoError` from pichost-core
- Produces: 6 REST endpoints under `/api/v1/users/me/storage-configs`

- [ ] **Step 1: Create pichost-api/src/routes/storage_configs.rs**

```rust
use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use pichost_core::{
    crypto::{encrypt_token, mask_token},
    models::{User, UserStorageConfig, UserStorageConfigResponse},
    error::{AppError, StorageError},
};
use crate::app::AppState;
use crate::auth::AuthUser;

#[derive(Debug, Deserialize)]
pub struct CreateConfigRequest {
    pub name: String,
    pub provider: String,       // "github" | "gitcode"
    pub token: String,          // plaintext PAT
    pub repo: String,           // "owner/repo"
    pub branch: Option<String>,
    pub path_prefix: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub name: Option<String>,
    pub token: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub path_prefix: Option<String>,
}

pub async fn list_configs(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<UserStorageConfigResponse>>, AppError> {
    let configs = sqlx::query_as!(
        UserStorageConfig,
        r#"SELECT id, user_id, name, provider, is_default,
           config, created_at, updated_at
           FROM user_storage_configs
           WHERE user_id = $1
           ORDER BY created_at"#,
        user.user_id
    )
    .fetch_all(&state.db)
    .await?;

    let responses: Vec<UserStorageConfigResponse> = configs
        .into_iter()
        .map(|c| {
            let detail: serde_json::Value = c.config;
            let repo = detail["repo"].as_str().unwrap_or("").to_string();
            let branch = detail["branch"].as_str().unwrap_or("main").to_string();
            let path_prefix = detail["path_prefix"].as_str().map(|s| s.to_string());
            let token = detail["token_encrypted"].as_str().unwrap_or("");
            let masked = mask_token(token);

            UserStorageConfigResponse {
                id: c.id,
                name: c.name,
                provider: c.provider,
                repo,
                branch,
                path_prefix,
                is_default: c.is_default,
                token_masked: masked,
                created_at: c.created_at,
                updated_at: c.updated_at,
            }
        })
        .collect();

    Ok(Json(responses))
}

pub async fn create_config(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateConfigRequest>,
) -> Result<(StatusCode, Json<UserStorageConfigResponse>), AppError> {
    // Validate provider
    if !["github", "gitcode"].contains(&req.provider.as_str()) {
        return Err(AppError::bad_request("不支持的存储类型，仅支持 github 和 gitcode"));
    }

    // Check max configs (5 per user)
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_storage_configs WHERE user_id = $1",
    )
    .bind(user.user_id)
    .fetch_one(&state.db)
    .await?;

    if count >= 5 {
        return Err(AppError::bad_request("最多只能创建5个存储配置"));
    }

    // Check name uniqueness
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_storage_configs WHERE user_id = $1 AND name = $2)",
    )
    .bind(user.user_id)
    .bind(&req.name)
    .fetch_one(&state.db)
    .await?;

    if exists {
        return Err(AppError::bad_request("配置名称已存在"));
    }

    // Verify PAT + repo reachability
    // TODO: call Git API GET /repos/{owner}/{repo} to validate

    // Encrypt token
    let encryption_key = state
        .config
        .token_encryption_key
        .as_ref()
        .ok_or_else(|| AppError::internal("系统未配置加密密钥"))?;

    let key_bytes: [u8; 32] = decode_key(encryption_key)?;
    let encrypted = encrypt_token(&req.token, &key_bytes)
        .map_err(|e| AppError::internal(format!("加密失败: {}", e)))?;

    let branch = req.branch.unwrap_or_else(|| "main".to_string());
    let is_default = req.is_default.unwrap_or(false);

    let config_json = serde_json::json!({
        "token_encrypted": encrypted,
        "repo": req.repo,
        "branch": branch,
        "path_prefix": req.path_prefix,
    });

    // Unset previous default if this one is default
    if is_default {
        sqlx::query(
            "UPDATE user_storage_configs SET is_default = false WHERE user_id = $1",
        )
        .bind(user.user_id)
        .execute(&state.db)
        .await?;
    }

    let config = sqlx::query_as!(
        UserStorageConfig,
        r#"INSERT INTO user_storage_configs
           (user_id, name, provider, is_default, config)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, user_id, name, provider, is_default,
                     config, created_at, updated_at"#,
        user.user_id,
        req.name,
        req.provider,
        is_default,
        config_json,
    )
    .fetch_one(&state.db)
    .await?;

    let response = build_response(&config, &req.name);

    Ok((StatusCode::CREATED, Json(response)))
}

// ... (other handlers: get_config, update_config, delete_config, set_default)

fn decode_key(encoded: &str) -> Result<[u8; 32], AppError> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let bytes = STANDARD.decode(encoded).map_err(|_| {
        AppError::internal("加密密钥格式错误，须为base64编码的32字节")
    })?;
    if bytes.len() != 32 {
        return Err(AppError::internal("加密密钥长度错误，须为32字节"));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

fn build_response(config: &UserStorageConfig, _name: &str) -> UserStorageConfigResponse {
    let detail: serde_json::Value = config.config.clone();
    let repo = detail["repo"].as_str().unwrap_or("").to_string();
    let branch = detail["branch"].as_str().unwrap_or("main").to_string();
    let path_prefix = detail["path_prefix"].as_str().map(|s| s.to_string());
    let token = detail["token_encrypted"].as_str().unwrap_or("");
    let masked = mask_token(token);

    UserStorageConfigResponse {
        id: config.id,
        name: config.name.clone(),
        provider: config.provider.clone(),
        repo,
        branch,
        path_prefix,
        is_default: config.is_default,
        token_masked: masked,
        created_at: config.created_at,
        updated_at: config.updated_at,
    }
}
```

Due to function length limits, split remaining handlers into separate files or extract helpers. The full CRUD (get_config, update_config, delete_config, set_default) follows the same pattern.

- [ ] **Step 2: Register routes in main.rs**

Add route group to `create_router()`:

```rust
.route("/api/v1/users/me/storage-configs", get(storage_configs::list_configs).post(storage_configs::create_config))
.route("/api/v1/users/me/storage-configs/{id}", get(storage_configs::get_config).patch(storage_configs::update_config).delete(storage_configs::delete_config))
.route("/api/v1/users/me/storage-configs/{id}/default", post(storage_configs::set_default))
```

- [ ] **Step 3: Commit**

```bash
git add pichost-api/src/routes/storage_configs.rs pichost-api/src/main.rs
git commit -m "feat: add storage config CRUD API endpoints"
```

---

### Task 6: Upload Pipeline — Multi-Backend Support

**Files:**
- Modify: `pichost-api/src/services/upload.rs`
- Modify: `pichost-api/src/routes/images.rs`

**Interfaces:**
- Consumes: `StorageRouter::for_config()`, `UserStorageConfig`
- Produces: Modified `process_upload()` accepting `storage_config_ids: Option<Vec<Uuid>>`

- [ ] **Step 1: Add storage_config_ids extraction in upload handler**

In `pichost-api/src/routes/images.rs`, modify the `upload_handler` signature:

```rust
pub async fn upload_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    // Extract storage_config_ids from multipart field
    let storage_config_ids = extract_storage_config_ids(&mut multipart).await?;
    let (bytes, filename) = extract_file_from_multipart(&mut multipart).await?;

    let results = upload::process_upload(
        &state,
        &user,
        bytes,
        filename,
        storage_config_ids,
    )
    .await?;

    Ok((StatusCode::OK, Json(results)))
}

async fn extract_storage_config_ids(
    multipart: &mut Multipart,
) -> Result<Option<Vec<Uuid>>, AppError> {
    // Read through multipart fields, find storage_config_ids
    // If found: parse comma-separated UUIDs, validate max 2
    // If not found: return None
    // ...
}
```

- [ ] **Step 2: Modify process_upload signature and logic**

In `pichost-api/src/services/upload.rs`, change `process_upload()` to accept `storage_config_ids: Option<Vec<Uuid>>` and loop over backends:

Key changes (pseudocode for brevity — full implementation delegates to subagent):

1. Resolve configs: if `storage_config_ids` is None → fetch user's default + local; else fetch by IDs
2. Validate: count ≤ 2, at least one is `local`
3. For each config: compute SHA256, dedup check `(user_id, sha256, config_id)`, generate public_key, write to storage, INSERT image, enqueue worker
4. Return `Vec<UploadResult>`

- [ ] **Step 3: Add storage_config to UploadResult**

```rust
#[derive(Debug, Serialize)]
pub struct StorageConfigInfo {
    pub id: Uuid,
    pub name: String,
    pub provider: String,
}

// Add to UploadResult:
pub storage_config: Option<StorageConfigInfo>,
```

- [ ] **Step 4: Update dedup query**

```rust
// Old:
let exists: bool = sqlx::query_scalar(
    "SELECT EXISTS(SELECT 1 FROM images WHERE user_id = $1 AND sha256 = $2)"
)
// New:
let exists: bool = sqlx::query_scalar(
    "SELECT EXISTS(SELECT 1 FROM images WHERE user_id = $1 AND sha256 = $2 AND storage_config_id = $3)"
)
.bind(storage_config_id)
```

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/services/upload.rs pichost-api/src/routes/images.rs
git commit -m "feat: add multi-backend upload support to upload pipeline"
```

---

### Task 7: Gallery — Storage Backend Filtering

**Files:**
- Modify: `pichost-api/src/services/upload.rs` (ImageListQuery, fetch functions)
- Modify: `pichost-api/src/routes/images.rs` (list_images handler)

**Interfaces:**
- Produces: `?storage_config_id=uuid` query parameter support

- [ ] **Step 1: Add storage_config_id to ImageListQuery**

```rust
pub struct ImageListQuery {
    pub page: u32,
    pub per_page: u32,
    pub sort: String,
    pub order: String,
    pub search: String,
    pub storage_config_id: Option<Uuid>,  // NEW
}
```

- [ ] **Step 2: Update fetch_user_images SQL**

Add conditional WHERE clause for `storage_config_id`:

```rust
let images: Vec<ImageRow> = if let Some(config_id) = &query.storage_config_id {
    sqlx::query_as(image_query_with_config)
        .bind(user.user_id)
        .bind(config_id)
        .bind(&search_pattern)
        .bind(query.per_page as i64)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
} else {
    // existing query unchanged
};
```

Where `image_query_with_config` appends `AND i.storage_config_id = $2`.

- [ ] **Step 3: Update count query similarly**

Same conditional logic for `count_user_images`.

- [ ] **Step 4: Extract storage_config_id from request**

```rust
pub async fn list_images(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(query): Query<ImageListQuery>,
) -> Result<Json<ImageListResponse>, AppError> {
    // query.storage_config_id auto-parsed from ?storage_config_id=uuid
}
```

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/services/upload.rs pichost-api/src/routes/images.rs
git commit -m "feat: add storage_config_id filter to gallery query"
```

---

### Task 8: Worker TaskPayload Extension

**Files:**
- Modify: `pichost-worker/src/queue.rs`

**Interfaces:**
- Produces: `TaskPayload.storage_config_id: Option<Uuid>`, `TaskPayload.storage_backend_name: String`

- [ ] **Step 1: Add fields to TaskPayload**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPayload {
    pub task_id: Uuid,
    pub image_id: Uuid,
    pub user_id: Uuid,
    pub storage_backend: String,
    pub storage_config_id: Option<Uuid>,    // NEW
    pub storage_backend_name: String,        // NEW: "github"|"gitcode"|"local"
    pub source_key: String,
    pub source_mime: String,
    pub retry_count: i32,
    pub max_retries: i32,
}
```

- [ ] **Step 2: Update enqueue code in upload service**

When enqueueing in `services/upload.rs`:

```rust
pub async fn enqueue_processing_task(
    state: &AppState,
    image_id: Uuid,
    user_id: Uuid,
    source_key: &str,
    source_mime: &str,
    storage_config_id: Option<Uuid>,   // NEW param
    storage_backend_name: &str,         // NEW param
) -> Result<(), AppError> {
    // ... set storage_config_id and storage_backend_name in payload
}
```

- [ ] **Step 3: Worker uses for_config for Git backends**

In `pichost-worker/src/pipeline.rs`, when resolving the backend:

```rust
let backend = if let Some(config_id) = &task.storage_config_id {
    // Fetch config from DB, then router.for_config()
    let config = fetch_storage_config(&state.db, config_id).await?;
    router.for_config(&config, &state.encryption_key)?
} else {
    router.for_backend(&task.storage_backend)
};
```

- [ ] **Step 4: Commit**

```bash
git add pichost-worker/src/queue.rs pichost-worker/src/pipeline.rs pichost-api/src/services/upload.rs
git commit -m "feat: extend worker TaskPayload for Git storage backends"
```

---

### Task 9: Frontend — API Client + Types

**Files:**
- Modify: `web-ui/src/api/client.ts`
- Modify: `web-ui/src/stores/auth.ts` (add types if needed)

- [ ] **Step 1: Add TypeScript types**

```typescript
// web-ui/src/api/client.ts

export interface UserStorageConfig {
  id: string;
  name: string;
  provider: 'github' | 'gitcode' | 'local';
  repo: string;
  branch: string;
  path_prefix: string | null;
  is_default: boolean;
  token_masked: string;
  created_at: string;
  updated_at: string;
}

export interface CreateStorageConfigRequest {
  name: string;
  provider: 'github' | 'gitcode';
  token: string;
  repo: string;
  branch?: string;
  path_prefix?: string;
  is_default?: boolean;
}

export interface UpdateStorageConfigRequest {
  name?: string;
  token?: string;
  repo?: string;
  branch?: string;
  path_prefix?: string;
}
```

- [ ] **Step 2: Add API functions**

```typescript
export async function listStorageConfigs(): Promise<UserStorageConfig[]> {
  return api.get('users/me/storage-configs').json();
}

export async function createStorageConfig(
  data: CreateStorageConfigRequest
): Promise<UserStorageConfig> {
  return api.post('users/me/storage-configs', { json: data }).json();
}

export async function updateStorageConfig(
  id: string,
  data: UpdateStorageConfigRequest
): Promise<UserStorageConfig> {
  return api.patch(`users/me/storage-configs/${id}`, { json: data }).json();
}

export async function deleteStorageConfig(id: string): Promise<void> {
  return api.delete(`users/me/storage-configs/${id}`).json();
}

export async function setDefaultStorageConfig(id: string): Promise<void> {
  return api.post(`users/me/storage-configs/${id}/default`).json();
}
```

- [ ] **Step 3: Add storage_config_ids to uploadImage**

```typescript
export async function uploadImage(
  file: File,
  storageConfigIds?: string[]
): Promise<UploadResult> {
  const form = new FormData();
  form.append('file', file);
  if (storageConfigIds?.length) {
    form.append('storage_config_ids', storageConfigIds.join(','));
  }
  return api.post('images', { body: form }).json();
}
```

- [ ] **Step 4: Add storage_config_id param to listImages**

```typescript
export async function listImages(params: {
  page?: number;
  per_page?: number;
  sort?: string;
  order?: string;
  search?: string;
  storage_config_id?: string;  // NEW
}): Promise<ImageListResponse> {
  // ...
}
```

- [ ] **Step 5: Commit**

```bash
cd web-ui && git add src/api/client.ts && git commit -m "feat: add storage config API functions to frontend client"
```

---

### Task 10: Frontend — Settings Page Storage Config Management

**Files:**
- Modify: `web-ui/src/pages/Settings.tsx`

- [ ] **Step 1: Create StorageConfigSection component**

Create a new file `web-ui/src/components/StorageConfigSection.tsx` containing:
- List of user's storage configs as radio-style cards
- Each card: name, provider icon, repo path, default badge
- `[+ Add]` button → modal form
- Edit/Delete actions per card
- Default toggle
- Test connection button

- [ ] **Step 2: Integrate into Settings.tsx**

Add `<StorageConfigSection />` as a new card in the Settings page layout, replacing the current `<select>` for `storageBackend`.

- [ ] **Step 3: Commit**

```bash
cd web-ui && git add src/components/StorageConfigSection.tsx src/pages/Settings.tsx && git commit -m "feat: add storage config management to Settings page"
```

---

### Task 11: Frontend — Dashboard Multi-Backend Selector

**Files:**
- Modify: `web-ui/src/pages/Dashboard.tsx`
- Modify: `web-ui/src/hooks/useUploadQueue.ts`
- Modify: `web-ui/src/components/UploadCard.tsx`

- [ ] **Step 1: Add backend selector above DropZone**

Two dropdown selects, max 2 selected, options from `listStorageConfigs()` query. Second selector appears on `[+ Add]` click.

- [ ] **Step 2: Pass storageConfigIds to useUploadQueue**

```typescript
const { queue, addFiles, clearQueue } = useUploadQueue();

const handleUpload = (files: File[]) => {
  addFiles(files, { storageConfigIds: selectedIds });
};
```

- [ ] **Step 3: Update UploadCard to show backend name**

```tsx
{task.storageConfigName && (
  <span className="text-xs text-[var(--color-text-secondary)]">
    → {task.storageConfigName}
  </span>
)}
```

- [ ] **Step 4: Commit**

```bash
cd web-ui && git add src/pages/Dashboard.tsx src/hooks/useUploadQueue.ts src/components/UploadCard.tsx && git commit -m "feat: add multi-backend selector to Dashboard upload"
```

---

### Task 12: Frontend — Gallery Backend Filter

**Files:**
- Modify: `web-ui/src/pages/Gallery.tsx`

- [ ] **Step 1: Add storage_config_id dropdown to filter bar**

```tsx
<select
  value={storageConfigFilter}
  onChange={(e) => setStorageConfigFilter(e.target.value)}
  className="..."
>
  <option value="">全部后端</option>
  {configs?.map(c => (
    <option key={c.id} value={c.id}>{c.name}</option>
  ))}
</select>
```

- [ ] **Step 2: Pass to useInfiniteQuery**

Include `storage_config_id` in query key and API call params. Sync to URL searchParams.

- [ ] **Step 3: Add provider badge to Gallery cards**

Show small icon (GitHub/GitCode/local) on image cards using `storage_config` field from response.

- [ ] **Step 4: Commit**

```bash
cd web-ui && git add src/pages/Gallery.tsx && git commit -m "feat: add storage backend filter to Gallery"
```

---

### Task 13: Integration Testing & Clippy

**Files:**
- Modify: various files (fix clippy warnings)

- [ ] **Step 1: Run clippy**

```bash
cargo clippy --workspace -- -D warnings
```

Expected: zero warnings. Fix any that arise.

- [ ] **Step 2: Run tests**

```bash
cargo test --workspace
```

Expected: 14 pass, 10 ignored (as before). No regressions.

- [ ] **Step 3: Manual smoke test**

1. Start API with `PICHOST_AUTH_TOKEN_ENCRYPTION_KEY=...` set
2. Register user, login
3. POST `/api/v1/users/me/storage-configs` with a real GitHub PAT + repo
4. Upload an image with `storage_config_ids` pointing to the new config
5. Verify image appears in Gallery, filter by backend works
6. Verify raw URL returns the image

- [ ] **Step 4: Commit final fixes**

```bash
git add -u && git commit -m "chore: clippy fixes and integration test cleanup"
```

---

## Completion Checklist

- [ ] Migration 0008 applied (user_storage_configs table)
- [ ] Token encryption/decryption round-trip test passes
- [ ] GitStorage implements all 6 StorageBackend trait methods
- [ ] StorageRouter caches Git backends dynamically
- [ ] 6 storage config CRUD endpoints return correct responses
- [ ] Upload with storage_config_ids writes to correct backends
- [ ] Gallery filtering by storage_config_id works
- [ ] Worker processes Git-backed images (thumbnail + WebP)
- [ ] Frontend Settings: create/edit/delete storage configs
- [ ] Frontend Dashboard: multi-backend selector + UploadCard changes
- [ ] Frontend Gallery: backend filter dropdown
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` no regressions
- [ ] Version bumped to 0.15.0
