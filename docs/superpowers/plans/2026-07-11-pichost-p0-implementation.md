# PicHost P0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the P0 baseline of PicHost — a multi-user image hosting service with JWT auth, single-file upload to LocalFS, public URL access, and full-format link output (URL/Markdown/HTML/BBCode). Reachable at `http://localhost:3000` (backend) and `http://localhost:5173` (frontend dev) or through Docker Compose.

**Architecture:** Three Rust workspace crates (pichost-core lib + pichost-api bin + pichost-worker placeholder), React SPA (Vite + shadcn/ui + TanStack Query), PostgreSQL 18 for metadata, Redis 8.0 for sessions/rate-limiting. LocalFS only in P0; RustFS and async Worker deferred to P1.

**Tech Stack:** Rust 1.96, Axum 0.8, Tokio 1.52, sqlx 0.8, PostgreSQL 18, Redis 8.0, React 19, Vite 6, shadcn/ui, Tailwind CSS 4, TypeScript 5.7

## Global Constraints

- Rust: 1.96 stable via `rust-toolchain.toml`, edition 2021
- PostgreSQL 18, migrations via `sqlx::migrate!("../../migrations")`
- Redis 8.0, connection pool via deadpool-redis 0.15
- Tokio 1.52.x with `full` features
- Axum 0.8 with `macros` feature
- JWT: jsonwebtoken 9, access_token_ttl=900s, refresh_token_ttl=2592000s
- Passwords: argon2 0.5 Argon2id (mem 19456, time 2, par 1), min 8 chars
- Public image URL: `/u/{public_key}` (6-char random base62)
- Storage path: `users/{user_id}/{yyyy}/{mm}/{random_id}.{ext}`
- SHA256 dedup: same user + same content → skip insert, return 200
- File limits: admin 50MB, user 10MB
- Allowed MIME: `image/png, image/jpeg, image/gif, image/webp, image/svg+xml, image/avif, image/bmp`
- Response links: url, markdown, html, bbcode
- Rust naming: snake_case; TypeScript naming: camelCase
- Commits: conventional commits (`feat:`, `fix:`, `test:`, `chore:`)
- TDD: write failing test → implement minimal code → commit

---

### Task 1: Workspace scaffolding & Core crate basics

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `rust-toolchain.toml`
- Create: `crates/pichost-core/Cargo.toml`
- Create: `crates/pichost-core/src/lib.rs`
- Create: `crates/pichost-core/src/error.rs`
- Create: `crates/pichost-core/src/models.rs`
- Create: `crates/pichost-api/Cargo.toml`
- Create: `crates/pichost-api/src/main.rs`
- Create: `crates/pichost-worker/Cargo.toml`
- Create: `crates/pichost-worker/src/main.rs`
- Create: `.gitignore`

**Interfaces Produced:**
- `pichost_core::error::AppError` (enum), `pichost_core::error::StorageError` (enum)
- `pichost_core::models::User`, `Image`, `ImageStatus`, `UploadTask`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
[workspace]
members = ["crates/pichost-core", "crates/pichost-api", "crates/pichost-worker"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"

[workspace.dependencies]
tokio = { version = "1.52", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
```

- [ ] **Step 2: Create rust-toolchain.toml**

```toml
[toolchain]
channel = "1.96"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Create .gitignore**

```
/target/
.env
storage-local/
```

- [ ] **Step 4: Create pichost-core/Cargo.toml**

```toml
[package]
name = "pichost-core"
version.workspace = true
edition.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
chrono.workspace = true
thiserror.workspace = true
tracing.workspace = true
async-trait = "0.1"

[dev-dependencies]
tempfile = "3"

[lib]
name = "pichost_core"
path = "src/lib.rs"
```

- [ ] **Step 5: Create pichost-core/src/error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("write failed: {0}")]
    WriteFailed(String),
    #[error("read failed: {0}")]
    ReadFailed(String),
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("config error: {0}")]
    Config(String),
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("authentication failed: {0}")]
    Authentication(String),
    #[error("not authorized: {0}")]
    Authorization(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("upload failed: {0}")]
    Upload(String),
    #[error("rate limited")]
    RateLimited,
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("internal error")]
    Internal,
}
```

- [ ] **Step 6: Create pichost-core/src/models.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub storage_backend: String,
    pub storage_prefix: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub id: Uuid,
    pub user_id: Uuid,
    pub public_key: String,
    pub original_name: String,
    pub storage_key: String,
    pub storage_backend: String,
    pub mime_type: String,
    pub file_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub sha256: String,
    pub url: String,
    pub thumbnail_key: Option<String>,
    pub thumbnail_url: Option<String>,
    pub webp_key: Option<String>,
    pub webp_url: Option<String>,
    pub status: ImageStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageStatus {
    Pending,
    Processing,
    Ready,
    Failed,
}

impl std::fmt::Display for ImageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Ready => write!(f, "ready"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadTask {
    pub id: Uuid,
    pub image_id: Uuid,
    pub task_type: String,
    pub payload: Option<serde_json::Value>,
    pub status: String,
    pub error: Option<String>,
    pub retry_count: i32,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
```

- [ ] **Step 7: Create pichost-core/src/lib.rs**

```rust
pub mod error;
pub mod models;
```

- [ ] **Step 8: Create pichost-api/Cargo.toml**

```toml
[package]
name = "pichost-api"
version.workspace = true
edition.workspace = true

[dependencies]
pichost-core = { path = "../pichost-core" }
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true

[[bin]]
name = "pichost-api"
path = "src/main.rs"
```

- [ ] **Step 9: Create pichost-api/src/main.rs**

```rust
fn main() {
    println!("PicHost API starting...");
}
```

- [ ] **Step 10: Create pichost-worker/Cargo.toml**

```toml
[package]
name = "pichost-worker"
version.workspace = true
edition.workspace = true

[dependencies]
pichost-core = { path = "../pichost-core" }
tokio.workspace = true

[[bin]]
name = "pichost-worker"
path = "src/main.rs"
```

- [ ] **Step 11: Create pichost-worker/src/main.rs**

```rust
fn main() {
    println!("PicHost Worker placeholder — P1");
}
```

- [ ] **Step 12: Verify**

```bash
cargo build --workspace
```

Expected: Compilation succeeds (warnings for unused imports OK, no errors).

- [ ] **Step 13: Commit**

```bash
git init && git add -A && git commit -m "chore: scaffold Rust workspace"
```

---

### Task 2: Configuration system

**Files:**
- Create: `crates/pichost-core/src/config.rs`
- Modify: `crates/pichost-core/Cargo.toml` (add figment)
- Modify: `crates/pichost-core/src/lib.rs`

**Interfaces Produced:**
- `AppConfig` struct with fields: server, auth, storage, database, redis, upload, logging
- `load_config() -> Result<AppConfig>` using figment (env > toml > defaults)

- [ ] **Step 1: Add figment**

```toml
# pichost-core/Cargo.toml
figment = { version = "0.11", features = ["toml", "env"] }
```

- [ ] **Step 2: Create config.rs**

```rust
use figment::{Figment, providers::{Env, Format, Toml}};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub storage: StorageConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub upload: UploadConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub public_url: String,
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub access_token_ttl: u64,
    pub refresh_token_ttl: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub default_backend: String,
    pub local_base_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadConfig {
    pub max_file_size_admin: u64,
    pub max_file_size_user: u64,
    pub allowed_mimes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig { host: "0.0.0.0".into(), port: 3000, public_url: "http://localhost:3000".into(), cors_origins: vec!["http://localhost:5173".into()] },
            auth: AuthConfig { jwt_secret: String::new(), access_token_ttl: 900, refresh_token_ttl: 2_592_000 },
            storage: StorageConfig { default_backend: "local".into(), local_base_path: PathBuf::from("./storage-local") },
            database: DatabaseConfig { url: "postgres://pichost:pichost@localhost:5432/pichost".into(), max_connections: 10 },
            redis: RedisConfig { url: "redis://localhost:6379".into(), pool_size: 20 },
            upload: UploadConfig { max_file_size_admin: 52_428_800, max_file_size_user: 10_485_760, allowed_mimes: vec!["image/png".into(), "image/jpeg".into(), "image/gif".into(), "image/webp".into(), "image/svg+xml".into(), "image/avif".into(), "image/bmp".into()] },
            logging: LoggingConfig { level: "info".into(), format: "json".into() },
        }
    }
}

pub fn load_config() -> Result<AppConfig, figment::Error> {
    Figment::from(AppConfig::default())
        .merge(Toml::file("config.toml").nested())
        .merge(Env::prefixed("PICHOST_").global())
        .extract()
}
```

- [ ] **Step 3: Update lib.rs**

```rust
pub mod config;
pub mod error;
pub mod models;
```

- [ ] **Step 4: Build check**

```bash
cargo check -p pichost-core
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/pichost-core/src/config.rs crates/pichost-core/src/lib.rs crates/pichost-core/Cargo.toml
git commit -m "chore: add configuration system with figment"
```

---

### Task 3: LocalStorage implementation

**Files:**
- Create: `crates/pichost-core/src/storage/mod.rs`
- Create: `crates/pichost-core/src/storage/local.rs`
- Create: `crates/pichost-core/tests/storage_test.rs`
- Modify: `crates/pichost-core/src/lib.rs`
- Modify: `crates/pichost-core/Cargo.toml`

**Interfaces Produced:**
- `StorageBackend` trait (async: put, get, delete, exists, public_url, backend_name)
- `LocalStorage` struct implementing StorageBackend

- [ ] **Step 1: Create storage/mod.rs**

```rust
use async_trait::async_trait;
use crate::error::StorageError;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<String, StorageError>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;
    fn public_url(&self, key: &str) -> String;
    fn backend_name(&self) -> &str;
}
```

- [ ] **Step 2: Create storage/local.rs**

```rust
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use crate::error::StorageError;
use super::StorageBackend;

pub struct LocalStorage { base_path: PathBuf, base_url: String }

impl LocalStorage {
    pub fn new(base_path: PathBuf, base_url: String) -> Self { Self { base_path, base_url } }
    fn full_path(&self, key: &str) -> PathBuf { self.base_path.join(key) }
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

    fn backend_name(&self) -> &str { "local" }
}
```

- [ ] **Step 3: Update lib.rs**

```rust
pub mod config;
pub mod error;
pub mod models;
pub mod storage;
```

- [ ] **Step 4: Add test dep**

```toml
# pichost-core dev-dependencies
tempfile = "3"
tokio = { workspace = true, features = ["rt", "macros"] }
```

- [ ] **Step 5: Write storage_test.rs**

```rust
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
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p pichost-core --test storage_test
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/pichost-core/src/storage/ crates/pichost-core/tests/ crates/pichost-core/src/lib.rs crates/pichost-core/Cargo.toml
git commit -m "feat: LocalStorage backend with TDD"
```

---

### Task 4: Database migrations & sqlx

**Files:**
- Create: `migrations/0001_create_users.sql`
- Create: `migrations/0002_create_images.sql`
- Create: `crates/pichost-api/src/db/mod.rs`
- Create: `crates/pichost-api/src/lib.rs`
- Modify: `crates/pichost-api/Cargo.toml`

- [ ] **Step 1: Add sqlx dep**

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono", "migrate"] }
dotenvy = "0.15"
```

- [ ] **Step 2: Create migrations/0001_create_users.sql**

```sql
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(64) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    storage_backend VARCHAR(32) NOT NULL DEFAULT 'local',
    storage_prefix VARCHAR(128) NOT NULL DEFAULT '',
    is_admin BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- [ ] **Step 3: Create migrations/0002_create_images.sql**

```sql
CREATE TABLE images (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    public_key VARCHAR(16) UNIQUE NOT NULL,
    original_name VARCHAR(255) NOT NULL,
    storage_key VARCHAR(512) NOT NULL,
    storage_backend VARCHAR(32) NOT NULL,
    mime_type VARCHAR(128) NOT NULL,
    file_size BIGINT NOT NULL,
    width INTEGER,
    height INTEGER,
    sha256 VARCHAR(64) NOT NULL,
    url VARCHAR(1024) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX idx_images_user_sha256 ON images(user_id, sha256);
```

- [ ] **Step 4: Create db/mod.rs**

```rust
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

pub type DbPool = PgPool;

pub async fn create_pool(url: &str, max_connections: u32) -> Result<DbPool, sqlx::Error> {
    PgPoolOptions::new().max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5)).connect(url).await
}

pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}
```

- [ ] **Step 5: Create lib.rs**

```rust
pub mod db;
```

- [ ] **Step 6: Update main.rs**

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").json().init();
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://pichost:pichost@localhost:5432/pichost".into());
    let pool = pichost_api::db::create_pool(&url, 5).await?;
    pichost_api::db::run_migrations(&pool).await?;
    tracing::info!("migrations done");
    Ok(())
}
```

- [ ] **Step 7: Test migrations**

```bash
docker run -d --name pichost-pg -e POSTGRES_USER=pichost -e POSTGRES_PASSWORD=pichost -e POSTGRES_DB=pichost -p 5432:5432 postgres:18-alpine
sleep 3 && cargo run -p pichost-api && docker stop pichost-pg && docker rm pichost-pg
```

Expected: "migrations done" log

- [ ] **Step 8: Commit**

```bash
git add migrations/ crates/pichost-api/src/db/ crates/pichost-api/src/lib.rs crates/pichost-api/src/main.rs crates/pichost-api/Cargo.toml
git commit -m "feat: database migrations and sqlx pool"
```

---

### Task 5: Redis cache module

**Files:**
- Create: `crates/pichost-api/src/cache/mod.rs`
- Modify: `crates/pichost-api/src/lib.rs`
- Modify: `crates/pichost-api/Cargo.toml`

- [ ] **Step 1: Add deps**

```toml
deadpool-redis = "0.15"
redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }
```

- [ ] **Step 2: Create cache/mod.rs**

```rust
use deadpool_redis::{Config as PoolConfig, Pool, Runtime};

pub type CachePool = Pool;

pub fn create_pool(url: &str, pool_size: u32) -> CachePool {
    PoolConfig::from_url(url).create_pool(Some(Runtime::Tokio1)).unwrap()
}

pub struct Cache { pool: CachePool }

impl Cache {
    pub fn new(pool: CachePool) -> Self { Self { pool } }

    pub async fn get(&self, key: &str) -> Result<Option<String>, redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?; c.get(key).await
    }
    pub async fn set(&self, key: &str, val: &str) -> Result<(), redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?; c.set(key, val).await
    }
    pub async fn set_ex(&self, key: &str, val: &str, ttl: u64) -> Result<(), redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?; c.set_ex(key, val, ttl as usize).await
    }
    pub async fn del(&self, key: &str) -> Result<(), redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?; c.del(key).await
    }
    pub async fn exists(&self, key: &str) -> Result<bool, redis::RedisError> {
        let mut c = self.pool.get().await.map_err(pool_err)?; c.exists(key).await
    }
}

fn pool_err(e: deadpool_redis::PoolError) -> redis::RedisError {
    redis::RedisError::from(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
}
```

- [ ] **Step 3: Update lib.rs**

```rust
pub mod cache;
pub mod db;
```

- [ ] **Step 4: Quick smoke test**

```bash
docker run -d --name pichost-r -p 6379:6379 redis:8-alpine
# manually verify rust compiles:
cargo check -p pichost-api
docker stop pichost-r && docker rm pichost-r
```

- [ ] **Step 5: Commit**

```bash
git add crates/pichost-api/src/cache/ crates/pichost-api/src/lib.rs crates/pichost-api/Cargo.toml
git commit -m "feat: add Redis cache module"
```

---

### Task 6: Auth registration & login + JWT middleware

**Files:**
- Create: `crates/pichost-api/src/app.rs`
- Create: `crates/pichost-api/src/routes/mod.rs`
- Create: `crates/pichost-api/src/routes/auth.rs`
- Create: `crates/pichost-api/src/middleware/mod.rs`
- Create: `crates/pichost-api/src/middleware/auth.rs`
- Modify: `crates/pichost-api/src/lib.rs`
- Modify: `crates/pichost-api/src/main.rs`
- Modify: `crates/pichost-api/Cargo.toml`

- [ ] **Step 1: Add deps**

```toml
axum = { version = "0.8", features = ["macros"] }
jsonwebtoken = "9"
argon2 = "0.5"
sha2 = "0.10"
rand = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }
infer = "0.16"
```

- [ ] **Step 2: Create app.rs**

```rust
use std::sync::Arc;
use pichost_core::config::AppConfig;
use crate::db::DbPool;
use crate::cache::Cache;

pub struct AppState {
    pub pool: DbPool,
    pub cache: Arc<Cache>,
    pub config: Arc<AppConfig>,
}
```

- [ ] **Step 3: Create routes/mod.rs** `pub mod auth;`

- [ ] **Step 4: Create routes/auth.rs**

Includes `RegisterRequest`, `LoginRequest`, `TokenClaims`, `AuthResponse`, `UserInfo` structs. Two handlers:
- `register`: validate → hash password (Argon2) → insert → generate JWT → return
- `login`: fetch user → verify password → generate JWT → return

Both use `State<Arc<AppState>>`. Both return `Json<AuthResponse>` with `access_token`, `refresh_token`, `user`.

- [ ] **Step 5: Create middleware/mod.rs** `pub mod auth;`

- [ ] **Step 6: Create middleware/auth.rs**

`AuthUser { id: Uuid, is_admin: bool }` extractor + middleware function that decodes JWT, checks Redis blacklist, and injects `AuthUser` into request extensions.

- [ ] **Step 7: Update lib.rs**

```rust
pub mod app;
pub mod cache;
pub mod db;
pub mod middleware;
pub mod routes;
```

- [ ] **Step 8: Update main.rs with routes**

```rust
use axum::{Router, routing::post};
use std::sync::Arc;
use pichost_api::{app::AppState, db, cache, routes};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").json().init();
    let config = pichost_core::config::load_config()?;
    let pool = db::create_pool(&config.database.url, config.database.max_connections).await?;
    db::run_migrations(&pool).await?;
    let cache_pool = cache::create_pool(&config.redis.url, config.redis.pool_size);
    let state = Arc::new(AppState { pool, cache: Arc::new(cache::Cache::new(cache_pool)), config: Arc::new(config) });

    let app = Router::new()
        .nest("/api/v1/auth", Router::new()
            .route("/register", post(routes::auth::register))
            .route("/login", post(routes::auth::login)))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("API on :3000");
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 9: Manual smoke test**

```bash
# Start infra
docker run -d --name pg -e POSTGRES_USER=pichost -e POSTGRES_PASSWORD=pichost -e POSTGRES_DB=pichost -p 5432:5432 postgres:18-alpine
docker run -d --name r -p 6379:6379 redis:8-alpine

# Start API
PICHOST_JWT_SECRET=test-secret-key-at-least-32-bytes-long!!! cargo run -p pichost-api &
sleep 5

# Register
curl -s -X POST http://localhost:3000/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123456"}'
# → expect 201 with tokens

# Login
curl -s -X POST http://localhost:3000/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123456"}'
# → expect 200 with tokens

kill %1; docker stop pg r; docker rm pg r
```

- [ ] **Step 10: Commit**

```bash
git add crates/pichost-api/src/app.rs crates/pichost-api/src/routes/ crates/pichost-api/src/middleware/ crates/pichost-api/src/lib.rs crates/pichost-api/src/main.rs crates/pichost-api/Cargo.toml
git commit -m "feat: auth register, login, JWT middleware"
```

---

### Task 7: Image upload & public serve endpoints

**Files:**
- Create: `crates/pichost-api/src/services/mod.rs`
- Create: `crates/pichost-api/src/services/upload.rs`
- Create: `crates/pichost-api/src/routes/images.rs`
- Modify: `crates/pichost-api/src/main.rs`
- Modify: `crates/pichost-api/src/lib.rs`
- Modify: `crates/pichost-api/Cargo.toml`

- [ ] **Step 1: Create services/mod.rs** `pub mod upload;`

- [ ] **Step 2: Create services/upload.rs**

Includes `UploadResult { id, public_key, original_name, url, markdown, html, bbcode, sha256, file_size }`.
Main function `process_upload(state, user, multipart)`:
1. Parse multipart field "file"
2. Validate MIME & magic bytes (infer)
3. Check file size (user/admin)
4. Compute SHA256
5. Dedup check (SELECT EXISTS ... WHERE user_id AND sha256)
6. Generate public_key (6-char random) + storage_key
7. Write to LocalStorage
8. INSERT INTO images
9. Return UploadResult with full link formats

- [ ] **Step 3: Create routes/images.rs**

Three handlers:
- `upload_handler`: protected, POST, calls upload::process_upload, returns 201 + ImageResponse
- `get_image`: protected, GET /:id, returns single image with all links
- `public_get`: public, GET /u/:public_key, reads from storage, returns raw bytes

- [ ] **Step 4: Update lib.rs**

```rust
pub mod services;
```

- [ ] **Step 5: Update main.rs — add image routes**

```rust
// Add imports
use axum::{middleware, extract::DefaultBodyLimit, routing::get};

// After auth_routes:
let protected = middleware::from_fn_with_state(state.clone(), pichost_api::middleware::auth::middleware);

let image_routes = Router::new()
    .route("/", post(routes::images::upload_handler))
    .route("/{id}", get(routes::images::get_image))
    .layer(protected);

let public_routes = Router::new()
    .route("/{public_key}", get(routes::images::public_get));

let app = Router::new()
    .nest("/api/v1/auth", auth_routes)
    .nest("/api/v1/images", image_routes)
    .nest("/u", public_routes)
    .layer(CorsLayer::permissive())
    .layer(DefaultBodyLimit::max(52_428_800))
    .with_state(state);
```

- [ ] **Step 6: Full smoke test (register → upload → public access)**

```bash
# Start infra + API (same as Task 6)
# Register & login to get token
TOKEN=$(curl -s http://localhost:3000/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123456"}' | \
  grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)

# Upload a 1x1 PNG
printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\x0f\x00\x00\x01\x01\x00\x05\x18\xd8N\x00\x00\x00\x00IEND\xaeB`\x82' > /tmp/test.png

curl -s -X POST http://localhost:3000/api/v1/images \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@/tmp/test.png"
# → expect 201 with url, markdown, html, bbcode

# Verify public access
PK=$(curl -s ... | grep -o '"public_key":"[^"]*"' | cut -d'"' -f4)
curl -s -o /dev/null -w "%{http_code}" http://localhost:3000/u/$PK
# → expect 200
```

- [ ] **Step 7: Commit**

```bash
git add crates/pichost-api/src/services/ crates/pichost-api/src/routes/images.rs crates/pichost-api/src/lib.rs crates/pichost-api/src/main.rs crates/pichost-api/Cargo.toml
git commit -m "feat: image upload and public serve"
```

---

### Task 8: Docker Compose deployment

**Files:**
- Create: `Dockerfile.api`
- Create: `docker-compose.yml`
- Create: `.env.example`

- [ ] **Step 1: Create Dockerfile.api**

```dockerfile
FROM rust:1.96-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release -p pichost-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/pichost-api /usr/local/bin/
CMD ["pichost-api"]
```

- [ ] **Step 2: Create docker-compose.yml**

```yaml
services:
  postgres:
    image: postgres:18-alpine
    environment:
      POSTGRES_USER: pichost; POSTGRES_PASSWORD: pichost; POSTGRES_DB: pichost
    ports: ["5432:5432"]
    volumes: [pgdata:/var/lib/postgresql/data]
    healthcheck: { test: ["CMD-SHELL","pg_isready -U pichost"], interval: 5s, timeout: 5s, retries: 5 }

  redis:
    image: redis:8-alpine
    ports: ["6379:6379"]
    command: redis-server --maxmemory 256mb --maxmemory-policy allkeys-lru

  api:
    build: { context: ., dockerfile: Dockerfile.api }
    ports: ["3000:3000"]
    environment:
      DATABASE_URL: postgres://pichost:pichost@postgres/pichost
      PICHOST_REDIS_URL: redis://redis:6379
      PICHOST_JWT_SECRET: dev-secret-32-bytes-long-for-pichost-!!!
      PICHOST_STORAGE_LOCAL_BASE_PATH: /app/storage-local
    volumes: [./storage-local:/app/storage-local]
    depends_on:
      postgres: { condition: service_healthy }
      redis: { condition: service_started }

volumes: { pgdata: }
```

- [ ] **Step 3: Create .env.example** with `PICHOST_JWT_SECRET=change-me`

- [ ] **Step 4: Test**

```bash
docker compose up --build -d && sleep 5
curl -s http://localhost:3000/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123456"}'
docker compose down -v
```

- [ ] **Step 5: Commit**

```bash
git add Dockerfile.api docker-compose.yml .env.example
git commit -m "chore: add Docker Compose dev deployment"
```

---

### Task 9: React frontend scaffold

**Files:**
- Create: `web-ui/package.json`
- Create: `web-ui/vite.config.ts`
- Create: `web-ui/tsconfig.json`
- Create: `web-ui/index.html`
- Create: `web-ui/src/main.tsx`
- Create: `web-ui/src/App.tsx`
- Create: `web-ui/src/index.css`

- [ ] **Step 1-7:** Create all scaffold files with dependencies: React 19, Vite 6, Tailwind CSS 4, react-router-dom 7, @tanstack/react-query 5, zustand 5, ky, react-dropzone, sonner, lucide-react

- [ ] **Step 8:** Install and verify

```bash
cd web-ui && npm install && npm run build
```

- [ ] **Step 9:** Commit

```bash
git add web-ui/ && git commit -m "chore: scaffold React frontend"
```

---

### Task 10: Auth store & Login page

**Files:**
- Create: `web-ui/src/api/client.ts`
- Create: `web-ui/src/stores/auth.ts`
- Create: `web-ui/src/pages/Login.tsx`
- Modify: `web-ui/src/App.tsx`

- [ ] **Step 1-4:** Create the store, API client with JWT auto-attach, and login form. Wire routes with Protected wrapper.

- [ ] **Step 5:** Build check

```bash
cd web-ui && npm run build
```

- [ ] **Step 6:** Commit

```bash
git add web-ui/src/api/ web-ui/src/stores/ web-ui/src/pages/ web-ui/src/App.tsx
git commit -m "feat: auth store and login page"
```

---

### Task 11: Dashboard with image upload

**Files:**
- Create: `web-ui/src/components/DropZone.tsx`
- Create: `web-ui/src/pages/Dashboard.tsx`
- Modify: `web-ui/src/App.tsx`

- [ ] **Step 1:** Create DropZone with react-dropzone, drag-and-drop + click to upload
- [ ] **Step 2:** Create Dashboard page — DropZone at top, list of uploaded images with link copy buttons
- [ ] **Step 3:** Wire route in App.tsx (`/dashboard` protected)
- [ ] **Step 4:** Build and commit

---

### Task 12: Gallery & Image detail pages

**Files:**
- Create: `web-ui/src/pages/Gallery.tsx`
- Create: `web-ui/src/pages/ImageDetail.tsx`
- Modify: `web-ui/src/App.tsx`

- [ ] **Step 1:** Gallery page — grid of uploaded images, click navigates to detail
- [ ] **Step 2:** ImageDetail page — image preview + 4-format link copy panel (URL/Markdown/HTML/BBCode)
- [ ] **Step 3:** Wire routes in App.tsx
- [ ] **Step 4:** Build and commit

---

### Task 13: P0 E2E verification & README

**Files:**
- Create: `README.md`

- [ ] **Step 1:** Write README.md with quick start, dev setup, and feature list
- [ ] **Step 2:** Run full E2E smoke test via Docker Compose: register → login → upload → public access
- [ ] **Step 3:** Tag P0

```bash
git tag -a v0.1.0-p0 -m "P0 baseline complete"
```

- [ ] **Step 4:** Done
