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

