# PicHost P3 — Gap Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close all 10 identified gaps between the design document and current v0.14.0 implementation — user settings, per-user storage backend selection, status enum reconciliation, route alignment, and operational supplements.

**Architecture:** Four independent implementation units. Unit B (storage backend + status) is the only hard prerequisite for Unit A (settings, which validates backend names). Units C and D can run in parallel with each other and with A. All backend changes go through `pichost-core` (models/traits) → `pichost-api` (routes/services). Frontend changes are isolated to `web-ui/`.

**Tech Stack:** Rust 1.96+ (Axum 0.8, sqlx, tokio), React 19 (TypeScript 7, Tailwind CSS 4, TanStack Query v5, Zustand), PostgreSQL 18, Redis 8.

## Global Constraints

- `cargo clippy --workspace -- -D warnings` — zero warnings at every commit
- `cargo test --workspace` — 14 pass, 10 ignored (no regressions)
- `cd web-ui && npm run build` — tsc + vite build pass
- No migration files added — all schema changes already exist in DB
- Backward compatible: ImageStatus::Active added to enum, no DB data changes
- Commit granularity: one commit per completed task

---

## File Structure

```
pichost-core/src/
├── models.rs              ← B: +ImageStatus::Active + User.storage_quota; D: +UploadTask.max_retries
└── storage/router.rs      ← B: +for_user()

pichost-api/src/
├── main.rs                ← A: +3 user routes; C: +/t/{pk} route; D: +/images/{id}/links
├── routes/
│   ├── users.rs           ← A: +get_my_profile, update_my_profile, change_my_password
│   ├── images.rs          ← C: +public_get_thumb_by_key; D: +get_image_links
│   └── auth.rs            ← B: +storage_prefix in register
└── services/upload.rs     ← B: wire user storage_backend through upload pipeline

web-ui/src/
├── App.tsx                ← A: +/register route
├── api/client.ts          ← A: +getUserMe, updateUserMe, changePassword + types
├── pages/
│   ├── Settings.tsx       ← A: full rewrite (profile/password/storage/oauth)
│   ├── Register.tsx       ← A: new, extracted from Login.tsx
│   ├── Login.tsx          ← A: remove isRegister toggle
│   ├── Admin.tsx          ← C: update imports to subdirectory
│   └── admin/             ← C: new subdirectory
│       ├── AdminStats.tsx  (moved from pages/)
│       ├── AdminUsers.tsx  (moved from pages/)
│       └── AdminInvites.tsx (moved from pages/)

docker-compose.prod.yml    ← D: new production compose file
```

---

## Unit B: 存储后端按用户选择 + Status 统一

### Task B1: core — for_user() + ImageStatus::Active

**Files:**
- Modify: `pichost-core/src/storage/router.rs:48-49` (add method + test)
- Modify: `pichost-core/src/models.rs:40-58` (add Active variant + Display)

**Interfaces:**
- Produces: `StorageRouter::for_user(&self, backend: &str) -> &Arc<dyn StorageBackend>` — looks up backend by name, falls back to default
- Produces: `ImageStatus::Active` — new enum variant, serialized as `"active"`

- [ ] **Step 1: Add `for_user()` to StorageRouter**

In `pichost-core/src/storage/router.rs`, insert after `for_backend()` (line 31):

```rust
    /// Route to the backend identified by user's storage_backend preference.
    /// Falls back to the default backend if the user's preferred backend is
    /// not registered.
    pub fn for_user(&self, backend: &str) -> &Arc<dyn StorageBackend> {
        self.backends
            .get(backend)
            .unwrap_or_else(|| self.default_backend())
    }
```

- [ ] **Step 2: Add test for `for_user()`**

In `pichost-core/src/storage/router.rs`, inside `mod tests`, after `test_router_for_backend` (line 106):

```rust
    #[test]
    fn test_router_for_user() {
        let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
        backends.insert("local".into(), Arc::new(MockBackend("local")));
        backends.insert("rustfs".into(), Arc::new(MockBackend("rustfs")));

        let router = super::StorageRouter::new(backends, "local".into());
        // User prefers rustfs → should get rustfs
        assert_eq!(router.for_user("rustfs").backend_name(), "rustfs");
        // User prefers nonexistent → falls back to default
        assert_eq!(router.for_user("nonexistent").backend_name(), "local");
    }
```

- [ ] **Step 3: Run core tests**

```bash
cargo test -p pichost-core
```
Expected: `test_router_for_user` PASS, all existing tests PASS.

- [ ] **Step 4: Add `Active` variant to `ImageStatus`**

In `pichost-core/src/models.rs`, modify the enum (line 40-47):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageStatus {
    Pending,
    Active,
    Processing,
    Ready,
    Failed,
}
```

- [ ] **Step 5: Update `Display` impl for `ImageStatus`**

In `pichost-core/src/models.rs`, add the Active arm (after line 51):

```rust
impl std::fmt::Display for ImageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Active => write!(f, "active"),
            Self::Processing => write!(f, "processing"),
            Self::Ready => write!(f, "ready"),
            Self::Failed => write!(f, "failed"),
        }
    }
}
```

- [ ] **Step 6: Build and check**

```bash
cargo check -p pichost-core
cargo clippy -p pichost-core -- -D warnings
```
Expected: zero errors, zero warnings.

- [ ] **Step 7: Commit**

```bash
git add pichost-core/src/storage/router.rs pichost-core/src/models.rs
git commit -m "feat(core): add StorageRouter::for_user() and ImageStatus::Active variant"
```

---

### Task B2: upload pipeline — wire user storage_backend

**Files:**
- Modify: `pichost-api/src/services/upload.rs:352-469` (write_to_storage, persist_image, enqueue_processing_task, process_upload)
- Modify: `pichost-api/src/routes/images.rs` (pass storage_backend from upload handler)

**Interfaces:**
- Consumes: `StorageRouter::for_user(&str)` from Task B1
- Produces: `process_upload()` now queries user's storage_backend and passes it through the pipeline
- Changed signatures: `write_to_storage` gains `storage_backend: &str`, `persist_image` gains `storage_backend: &str`, `enqueue_processing_task` gains `storage_backend: &str`

- [ ] **Step 1: Modify `write_to_storage()` — use for_user instead of default_backend**

In `pichost-api/src/services/upload.rs`, replace lines 354-379:

```rust
/// Writes bytes to storage using the user's preferred backend, and builds
/// the public URL. Returns `(storage_key, url)`.
async fn write_to_storage(
    router: &StorageRouter,
    public_url: &str,
    user_id: Uuid,
    public_key: &str,
    bytes: &[u8],
    mime_type: &str,
    storage_backend: &str,
) -> Result<(String, String), ApiError> {
    let storage_key = format!("{}/{}", user_id, public_key);
    let storage = router.for_user(storage_backend);
    storage
        .put(&storage_key, bytes, mime_type)
        .await
        .map_err(|e| {
            tracing::warn!("Storage write failed on {}: {e}", storage.backend_name());
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "storage write failed"})),
            )
        })?;
    let url = if storage.backend_name() == "local" {
        format!("{}/u/{}", public_url.trim_end_matches('/'), public_key)
    } else {
        storage.public_url(&storage_key)
    };
    Ok((storage_key, url))
}
```

- [ ] **Step 2: Modify `persist_image()` — accept storage_backend parameter**

In `pichost-api/src/services/upload.rs`, replace the `persist_image` signature (line 431) and body (lines 441-469):

```rust
/// Orchestrates storage write, URL construction, and DB insert.
/// Returns `(image_id, storage_key)`.
#[allow(clippy::too_many_arguments)]
async fn persist_image(
    state: &AppState,
    user: &AuthUser,
    public_key: &str,
    original_name: &str,
    bytes: &[u8],
    mime_type: &str,
    width: Option<i32>,
    height: Option<i32>,
    sha256: &str,
    storage_backend: &str,
) -> Result<(Uuid, String), ApiError> {
    let (storage_key, url) = write_to_storage(
        &state.router,
        &state.config.server.public_url,
        user.id,
        public_key,
        bytes,
        mime_type,
        storage_backend,
    )
    .await?;

    let image_id = insert_image_record(
        &state.pool,
        user.id,
        public_key,
        original_name,
        &storage_key,
        storage_backend,
        mime_type,
        bytes.len() as i64,
        width,
        height,
        sha256,
        &url,
    )
    .await?;

    Ok((image_id, storage_key))
}
```

- [ ] **Step 3: Modify `enqueue_processing_task()` — accept storage_backend**

In `pichost-api/src/services/upload.rs`, replace the signature and payload in lines 130-176:

```rust
async fn enqueue_processing_task(
    redis_pool: &CachePool,
    image_id: Uuid,
    user_id: Uuid,
    storage_key: &str,
    mime_type: &str,
    storage_backend: &str,
) {
    let task_id = Uuid::new_v4();
    let payload = serde_json::json!({
        "task_id": task_id.to_string(),
        "image_id": image_id.to_string(),
        "user_id": user_id.to_string(),
        "storage_backend": storage_backend,
        "source_key": storage_key,
        "source_mime": mime_type,
        "retry_count": 0,
        "max_retries": 3,
    });
    // ... rest of the function unchanged ...
```

(Make sure to update the `enqueue_processing_task` call site in `process_upload` to pass `storage_backend`.)

- [ ] **Step 4: Modify `process_upload()` — query user's storage_backend and pass through**

In `pichost-api/src/services/upload.rs`, replace `process_upload` body (lines 516-557):

```rust
pub async fn process_upload(
    state: Arc<AppState>,
    user: AuthUser,
    multipart: Multipart,
) -> Result<UploadResult, ApiError> {
    let (bytes, file_name) = extract_file_from_multipart(multipart).await?;

    if !infer::is_image(&bytes) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "file is not a valid image"})),
        ));
    }

    check_upload_quotas(&state, &user, bytes.len() as u64).await?;

    let sha256 = format!("{:x}", sha2::Sha256::digest(&bytes));

    if let Some(existing) = try_dedup(&state, &user, &sha256).await? {
        return Ok(existing);
    }

    // Query user's storage_backend preference
    let storage_backend: String = sqlx::query_scalar(
        "SELECT storage_backend FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Failed to fetch user storage_backend: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    let public_key = generate_public_key(&state).await?;
    let mime_type = detect_mime(&bytes);
    let (width, height) = image_dimensions(&bytes);

    let (image_id, storage_key) = persist_image(
        &state, &user, &public_key, &file_name, &bytes, &mime_type,
        width, height, &sha256, &storage_backend,
    )
    .await?;

    enqueue_processing_task(
        &state.cache.get_pool(), image_id, user.id, &storage_key, &mime_type, &storage_backend,
    )
    .await;

    Ok(build_result(
        &state, image_id, public_key, file_name, &bytes,
        mime_type, width, height, sha256,
    ))
}
```

- [ ] **Step 5: Update `build_result()` — use Dynamic status if needed**

The `build_result()` function at line 474 hardcodes `"active"` on line 507. This is fine — no change needed. The `ImageStatus::Active` variant now covers this.

- [ ] **Step 6: Build and lint check**

```bash
cargo check -p pichost-api
cargo clippy -p pichost-api -- -D warnings
```
Expected: zero errors, zero warnings.

- [ ] **Step 7: Run tests**

```bash
cargo test -p pichost-api
```
Expected: no regressions.

- [ ] **Step 8: Commit**

```bash
git add pichost-api/src/services/upload.rs
git commit -m "feat(upload): wire user storage_backend preference through upload pipeline"
```

---

### Task B3: auth — set storage_prefix at registration

**Files:**
- Modify: `pichost-api/src/routes/auth.rs:221-254` (insert_user function)

**Interfaces:**
- Consumes: `Uuid` from insert_user return value
- Produces: `storage_prefix` column gets a value like `"users/{user_id}"`

- [ ] **Step 1: Add storage_prefix column to insert_user**

In `pichost-api/src/routes/auth.rs`, modify `insert_user` SQL (line 229-231) to include `storage_prefix`:

```rust
async fn insert_user(
    state: &AppState,
    username: &str,
    email: &Option<String>,
    hash: &str,
    is_admin: bool,
    storage_quota: Option<i64>,
    storage_prefix: &str,
) -> Result<Uuid, (StatusCode, Json<serde_json::Value>)> {
    sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, is_admin, storage_quota, storage_prefix) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(username)
    .bind(email)
    .bind(hash)
    .bind(is_admin)
    .bind(storage_quota)
    .bind(storage_prefix)
    .fetch_one(&state.pool)
    .await
    // ... error handling unchanged ...
}
```

- [ ] **Step 2: Update register handler call site**

In `pichost-api/src/routes/auth.rs`, in the `register` function (around line 275), compute `storage_prefix` before calling `insert_user`:

```rust
    let storage_prefix = format!("users/{}", user_id.to_string());
    let user_id: Uuid = insert_user(
        &state, &payload.username, &payload.email, &hash, is_first_user, storage_quota, &storage_prefix,
    )
    .await?;
```

Wait — `user_id` is not available until after `insert_user` returns. So we need to split this: insert first without storage_prefix (DB default `''`), then UPDATE with the computed prefix:

```rust
    let user_id: Uuid = insert_user(
        &state, &payload.username, &payload.email, &hash, is_first_user, storage_quota,
    )
    .await?;

    // Set storage_prefix using the newly generated user_id
    let storage_prefix = format!("users/{}", user_id);
    sqlx::query("UPDATE users SET storage_prefix = $1 WHERE id = $2")
        .bind(&storage_prefix)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to set storage_prefix: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?;
```

Actually, the simpler approach: keep the existing `insert_user` signature unchanged, and add the UPDATE after `insert_user` returns. No need to change `insert_user` at all.

- [ ] **Step 2 (revised): Add storage_prefix UPDATE after insert_user in register**

In `pichost-api/src/routes/auth.rs`, in the `register` function, after `insert_user` returns (after line 278), add:

```rust
    // Set storage_prefix using the newly generated user_id
    let storage_prefix = format!("users/{}", user_id);
    let _ = sqlx::query("UPDATE users SET storage_prefix = $1 WHERE id = $2")
        .bind(&storage_prefix)
        .bind(user_id)
        .execute(&state.pool)
        .await;
```

- [ ] **Step 3: Build and lint**

```bash
cargo check -p pichost-api
cargo clippy -p pichost-api -- -D warnings
```

- [ ] **Step 4: Run tests**

```bash
cargo test --workspace
```
Expected: 14 pass, no new failures.

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/routes/auth.rs
git commit -m "feat(auth): set storage_prefix at user registration"
```

---

## Unit A: 用户自行设置系统

### Task A1: core — add User.storage_quota to domain model

**Files:**
- Modify: `pichost-core/src/models.rs:5-16`

- [ ] **Step 1: Add storage_quota field**

In `pichost-core/src/models.rs`, add `storage_quota` after `storage_prefix`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub storage_backend: String,
    pub storage_prefix: String,
    pub storage_quota: Option<i64>,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 2: Add new request/response types for settings**

In `pichost-core/src/models.rs`, add after the UploadTask struct:

```rust
/// Response for GET /users/me — full user profile
#[derive(Debug, Clone, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub storage_backend: String,
    pub storage_prefix: String,
    pub storage_quota: Option<i64>,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request body for PATCH /users/me
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub storage_backend: Option<String>,
}

/// Request body for POST /users/me/password
#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}
```

- [ ] **Step 3: Build and lint**

```bash
cargo check -p pichost-core
cargo clippy -p pichost-core -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add pichost-core/src/models.rs
git commit -m "feat(core): add User.storage_quota, UserProfile, UpdateProfileRequest, ChangePasswordRequest types"
```

---

### Task A2: backend — GET /users/me handler

**Files:**
- Modify: `pichost-api/src/routes/users.rs`

- [ ] **Step 1: Add `get_my_profile` handler**

In `pichost-api/src/routes/users.rs`, add after the existing imports:

```rust
use pichost_core::models::UserProfile;
```

Add the handler at the end of the file:

```rust
/// GET /api/v1/users/me — current user's full profile
pub async fn get_my_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<UserProfile>, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (Uuid, String, Option<String>, String, String, Option<i64>, bool, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, username, email, storage_backend, storage_prefix, storage_quota, is_admin, created_at, updated_at FROM users WHERE id = $1"
    )
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("User profile query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    match row {
        Some((id, username, email, storage_backend, storage_prefix, storage_quota, is_admin, created_at, updated_at)) => {
            Ok(Json(UserProfile {
                id,
                username,
                email,
                storage_backend,
                storage_prefix,
                storage_quota,
                is_admin,
                created_at,
                updated_at,
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "user not found"})),
        )),
    }
}
```

- [ ] **Step 2: Build and lint**

```bash
cargo check -p pichost-api
cargo clippy -p pichost-api -- -D warnings
```

- [ ] **Step 3: Commit**

```bash
git add pichost-api/src/routes/users.rs
git commit -m "feat(api): add GET /users/me endpoint returning full user profile"
```

---

### Task A3: backend — PATCH /users/me + POST /users/me/password handlers

**Files:**
- Modify: `pichost-api/src/routes/users.rs`

- [ ] **Step 1: Add `update_my_profile` handler**

Append to `pichost-api/src/routes/users.rs`:

```rust
use pichost_core::models::{UpdateProfileRequest, ChangePasswordRequest};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

/// PATCH /api/v1/users/me — update own profile
pub async fn update_my_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<UserProfile>, (StatusCode, Json<serde_json::Value>)> {
    // Validate storage_backend if provided
    if let Some(ref backend) = payload.storage_backend {
        if state.router.get(backend).is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("unknown backend: {}", backend)})),
            ));
        }
    }

    // Build dynamic UPDATE
    if let Some(ref username) = payload.username {
        // Check uniqueness
        let conflict: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE username = $1 AND id != $2)",
        )
        .bind(username)
        .bind(user.id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Username uniqueness check failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;
        if conflict {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({"error": "username already taken"})),
            ));
        }
    }

    if let Some(ref email) = payload.email {
        let conflict: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE email = $1 AND id != $2)",
        )
        .bind(email)
        .bind(user.id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Email uniqueness check failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;
        if conflict {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({"error": "email already taken"})),
            ));
        }
    }

    // Update each field individually (simple approach with COALESCE)
    sqlx::query(
        "UPDATE users SET \
         username = COALESCE($1, username), \
         email = CASE WHEN $2::boolean THEN $3 ELSE email END, \
         storage_backend = COALESCE($4, storage_backend), \
         updated_at = now() \
         WHERE id = $5",
    )
    .bind(&payload.username)
    .bind(payload.email.is_some())  // flag: update email?
    .bind(&payload.email)
    .bind(&payload.storage_backend)
    .bind(user.id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if let Some(code) = db_err.code() {
                if code == "23505" {
                    return (
                        StatusCode::CONFLICT,
                        Json(serde_json::json!({"error": "username or email already exists"})),
                    );
                }
            }
        }
        tracing::warn!("Profile update failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    // Return updated profile by delegating to get_my_profile logic
    // (call the same query inline)
    let row = sqlx::query_as::<_, (Uuid, String, Option<String>, String, String, Option<i64>, bool, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, username, email, storage_backend, storage_prefix, storage_quota, is_admin, created_at, updated_at FROM users WHERE id = $1"
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Profile re-fetch after update failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;

    Ok(Json(UserProfile {
        id: row.0, username: row.1, email: row.2,
        storage_backend: row.3, storage_prefix: row.4,
        storage_quota: row.5, is_admin: row.6,
        created_at: row.7, updated_at: row.8,
    }))
}
```

- [ ] **Step 2: Add `change_my_password` handler**

Append to `pichost-api/src/routes/users.rs`:

```rust
/// POST /api/v1/users/me/password — change own password
pub async fn change_my_password(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if payload.new_password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "new password must be at least 8 characters"})),
        ));
    }

    // Fetch current password_hash
    let current_hash: String = sqlx::query_scalar(
        "SELECT password_hash FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Password hash fetch failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?
    .ok_or_else(|| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "user not found"})),
    ))?;

    // Verify current password
    let parsed_hash = PasswordHash::new(&current_hash).map_err(|e| {
        tracing::warn!("Invalid stored password hash: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal server error"})),
        )
    })?;
    Argon2::default()
        .verify_password(payload.current_password.as_bytes(), &parsed_hash)
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "current password incorrect"})),
            )
        })?;

    // Hash new password
    let salt = SaltString::generate(&mut OsRng);
    let new_hash = Argon2::default()
        .hash_password(payload.new_password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::warn!("Password hashing failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;

    sqlx::query("UPDATE users SET password_hash = $1, updated_at = now() WHERE id = $2")
        .bind(&new_hash)
        .bind(user.id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Password update failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
        })?;

    Ok(Json(serde_json::json!({"message": "password updated"})))
}
```

- [ ] **Step 3: Build and lint**

```bash
cargo check -p pichost-api
cargo clippy -p pichost-api -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/users.rs
git commit -m "feat(api): add PATCH /users/me and POST /users/me/password endpoints"
```

---

### Task A4: backend — route registration

**Files:**
- Modify: `pichost-api/src/main.rs:94-105` (user_routes function)

- [ ] **Step 1: Register new routes in user_routes()**

In `pichost-api/src/main.rs`, replace `user_routes` function (lines 94-105):

```rust
fn user_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let protected =
        middleware::from_fn_with_state(state.clone(), pichost_api::middleware::auth::require_auth);
    Router::new()
        .route("/me", get(routes::users::get_my_profile).patch(routes::users::update_my_profile))
        .route("/me/stats", get(routes::users::get_my_stats))
        .route("/me/password", post(routes::users::change_my_password))
        .route("/oauth/link", post(routes::oauth::oauth_link))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(protected)
}
```

Note: The `patch` method import already exists at line 8 of main.rs (`use axum::{routing::{get, patch, post}, ...}` — but `delete` is also used somewhere. Wait, let me check — line 8 has `{get, patch, post}`. But `delete_image` uses `delete`. Let me re-check...

Actually in main.rs line 8 we have `routing::{get, patch, post}` and line 85 uses `.delete()`. The `delete` method is available via method chaining on `MethodRouter`, not as a standalone import. So no change needed to imports.

- [ ] **Step 2: Build and lint**

```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 3: Run tests**

```bash
cargo test --workspace
```
Expected: 14 pass, no regressions.

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/main.rs
git commit -m "feat(api): register GET/PATCH /users/me and POST /users/me/password routes"
```

---

### Task A5: frontend — API client functions

**Files:**
- Modify: `web-ui/src/api/client.ts`

- [ ] **Step 1: Add UserProfile type and new API functions**

In `web-ui/src/api/client.ts`, add after the `UserInfo` interface (line 11):

```typescript
export interface UserProfile {
  id: string
  username: string
  email: string | null
  storage_backend: string
  storage_prefix: string
  storage_quota: number | null
  is_admin: boolean
  created_at: string
  updated_at: string
}

export interface UpdateProfileRequest {
  username?: string
  email?: string
  storage_backend?: string
}

export interface ChangePasswordRequest {
  current_password: string
  new_password: string
}
```

Add after `getUserStats` (line 181):

```typescript
export async function getUserMe(): Promise<UserProfile> {
  return api.get('users/me').json<UserProfile>()
}

export async function updateUserMe(body: UpdateProfileRequest): Promise<UserProfile> {
  return api.patch('users/me', { json: body }).json<UserProfile>()
}

export async function changePassword(body: ChangePasswordRequest): Promise<{ message: string }> {
  return api.post('users/me/password', { json: body }).json<{ message: string }>()
}
```

- [ ] **Step 2: Verify build**

```bash
cd web-ui && npm run build
```

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/api/client.ts
git commit -m "feat(web): add getUserMe, updateUserMe, changePassword API functions"
```

---

### Task A6: frontend — Settings.tsx full rewrite

**Files:**
- Modify: `web-ui/src/pages/Settings.tsx`

- [ ] **Step 1: Full Settings.tsx rewrite**

Replace `web-ui/src/pages/Settings.tsx` entirely:

```tsx
import { useState, useEffect, type FormEvent } from 'react'
import { toast } from 'sonner'
import { Loader2, Save, Lock, HardDrive } from 'lucide-react'
import { getUserMe, updateUserMe, changePassword, getUserStats } from '../api/client'
import type { UserProfile, UserStats } from '../api/client'

export default function Settings() {
  const [profile, setProfile] = useState<UserProfile | null>(null)
  const [stats, setStats] = useState<UserStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)

  // Profile form
  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')
  const [storageBackend, setStorageBackend] = useState('local')

  // Password form
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [changingPw, setChangingPw] = useState(false)

  useEffect(() => {
    Promise.all([getUserMe(), getUserStats()])
      .then(([p, s]) => {
        setProfile(p)
        setStats(s)
        setUsername(p.username)
        setEmail(p.email ?? '')
        setStorageBackend(p.storage_backend)
      })
      .catch(() => toast.error('Failed to load profile'))
      .finally(() => setLoading(false))
  }, [])

  async function handleSaveProfile(e: FormEvent) {
    e.preventDefault()
    setSaving(true)
    try {
      const updated = await updateUserMe({
        username: username || undefined,
        email: email || undefined,
        storage_backend: storageBackend,
      })
      setProfile(updated)
      toast.success('Profile updated')
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to save')
    } finally {
      setSaving(false)
    }
  }

  async function handleChangePassword(e: FormEvent) {
    e.preventDefault()
    if (newPassword.length < 8) {
      toast.error('Password must be at least 8 characters')
      return
    }
    setChangingPw(true)
    try {
      await changePassword({ current_password: currentPassword, new_password: newPassword })
      toast.success('Password changed')
      setCurrentPassword('')
      setNewPassword('')
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to change password'
      // ky throws on non-2xx, extract response body
      toast.error(msg)
    } finally {
      setChangingPw(false)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center p-8">
        <Loader2 className="h-6 w-6 animate-spin" style={{ color: 'var(--color-text-muted)' }} />
      </div>
    )
  }

  const used = stats?.total_size ?? 0
  const quota = profile?.storage_quota
  const usagePercent = quota && quota > 0 ? Math.min(100, (used / quota) * 100) : 0
  const quotaColor = usagePercent > 80 ? 'var(--color-danger)' : usagePercent > 50 ? '#eab308' : 'var(--color-accent)'

  return (
    <div className="mx-auto max-w-2xl space-y-4 p-4">
      <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>Settings</h2>

      {/* ── Profile Card ── */}
      <form onSubmit={handleSaveProfile} className="space-y-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>Profile</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Username</label>
            <input type="text" value={username} onChange={e => setUsername(e.target.value)}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm"
              style={{ color: 'var(--color-text-primary)' }} />
          </div>
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Email</label>
            <input type="email" value={email} onChange={e => setEmail(e.target.value)}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm"
              style={{ color: 'var(--color-text-primary)' }} />
          </div>
        </div>
        <button type="submit" disabled={saving}
          className="flex items-center gap-2 rounded-lg px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50"
          style={{ backgroundColor: 'var(--color-accent)' }}>
          {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Save className="h-3.5 w-3.5" />}
          Save Profile
        </button>
      </form>

      {/* ── Password Card ── */}
      <form onSubmit={handleChangePassword} className="space-y-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>Password</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Current Password</label>
            <input type="password" required value={currentPassword} onChange={e => setCurrentPassword(e.target.value)}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm"
              style={{ color: 'var(--color-text-primary)' }} />
          </div>
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>New Password (min 8 chars)</label>
            <input type="password" required minLength={8} value={newPassword} onChange={e => setNewPassword(e.target.value)}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm"
              style={{ color: 'var(--color-text-primary)' }} />
          </div>
        </div>
        <button type="submit" disabled={changingPw}
          className="flex items-center gap-2 rounded-lg px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50"
          style={{ backgroundColor: 'var(--color-accent)' }}>
          {changingPw ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Lock className="h-3.5 w-3.5" />}
          Change Password
        </button>
      </form>

      {/* ── Storage Card ── */}
      <div className="space-y-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>Storage</h3>
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Backend</label>
          <select value={storageBackend} onChange={e => setStorageBackend(e.target.value)}
            className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm"
            style={{ color: 'var(--color-text-primary)' }}>
            <option value="local">local</option>
            <option value="rustfs">rustfs</option>
          </select>
        </div>
        {quota && quota > 0 ? (
          <div>
            <div className="flex justify-between text-xs" style={{ color: 'var(--color-text-muted)' }}>
              <span>{formatBytes(used)} / {formatBytes(quota)}</span>
              <span>{usagePercent.toFixed(0)}%</span>
            </div>
            <div className="mt-1 h-2 overflow-hidden rounded-full" style={{ backgroundColor: 'var(--color-surface)' }}>
              <div className="h-full rounded-full transition-all" style={{ width: `${usagePercent}%`, backgroundColor: quotaColor }} />
            </div>
          </div>
        ) : (
          <p className="text-xs" style={{ color: 'var(--color-text-muted)' }}>{formatBytes(used)} used (unlimited)</p>
        )}
      </div>

      {/* ── OAuth Card ── */}
      <div className="rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
        <h3 className="mb-2 text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>OAuth Accounts</h3>
        <p className="mb-3 text-xs" style={{ color: 'var(--color-text-muted)' }}>
          Link your GitHub or Google account for one-click login.
        </p>
        <div className="flex gap-2">
          <a href="/api/v1/auth/oauth/github"
            className="flex items-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-3 py-1.5 text-xs hover:bg-[var(--color-surface)] transition-colors"
            style={{ color: 'var(--color-text-primary)' }}>
            Link GitHub
          </a>
          <a href="/api/v1/auth/oauth/google"
            className="flex items-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-3 py-1.5 text-xs hover:bg-[var(--color-surface)] transition-colors"
            style={{ color: 'var(--color-text-primary)' }}>
            Link Google
          </a>
        </div>
      </div>
    </div>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`
  return `${(bytes / 1073741824).toFixed(2)} GB`
}
```

- [ ] **Step 2: Build frontend**

```bash
cd web-ui && npm run build
```
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/pages/Settings.tsx
git commit -m "feat(web): full Settings page with profile, password, storage, and OAuth cards"
```

---

### Task A7: frontend — Register.tsx extraction

**Files:**
- Create: `web-ui/src/pages/Register.tsx`
- Modify: `web-ui/src/pages/Login.tsx` (remove isRegister toggle)
- Modify: `web-ui/src/App.tsx` (add /register route)

- [ ] **Step 1: Create Register.tsx**

Create `web-ui/src/pages/Register.tsx` with the register form extracted from Login.tsx:

```tsx
import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { UserPlus, Loader2, KeyRound } from 'lucide-react'
import { toast } from 'sonner'
import { useAuthStore } from '../stores/auth'

export default function Register() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [inviteCode, setInviteCode] = useState('')
  const { register, isLoading, error, clearError } = useAuthStore()
  const navigate = useNavigate()

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    await register(username, password, inviteCode || undefined)
    const state = useAuthStore.getState()
    if (state.isAuthenticated) {
      if (state.user?.is_admin) {
        toast.success('Admin account created! You are now the administrator.', { duration: 6000 })
      } else {
        toast.success('Registered!')
      }
      navigate('/dashboard', { replace: true })
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4" style={{ backgroundColor: 'var(--color-bg)' }}>
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <h1 className="text-4xl font-bold" style={{
            background: 'linear-gradient(135deg, #3b82f6, #8b5cf6)',
            WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent',
          }}>
            PicHost
          </h1>
          <p className="mt-1 text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Create your account
          </p>
        </div>
        <form onSubmit={handleSubmit} className="space-y-4 rounded-xl p-6"
          style={{
            backgroundColor: 'var(--glass-bg)', border: '1px solid var(--glass-border)',
            backdropFilter: 'blur(var(--glass-blur))', boxShadow: 'var(--glass-shadow)',
          }}>
          {error && (
            <div className="rounded-lg px-4 py-2 text-sm"
              style={{ backgroundColor: 'var(--color-danger-subtle)', color: 'var(--color-danger)' }}>
              {error}
            </div>
          )}
          <div>
            <label htmlFor="username" className="block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>Username</label>
            <input id="username" type="text" required minLength={3} value={username}
              onChange={e => setUsername(e.target.value)}
              className="mt-1 block w-full rounded-lg px-3 py-2 placeholder-gray-500 focus:outline-none focus:ring-1"
              style={{ backgroundColor: 'var(--color-surface)', border: '1px solid var(--color-border)', color: 'var(--color-text-primary)' }}
              placeholder="your username" />
          </div>
          <div>
            <label htmlFor="password" className="block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>Password</label>
            <input id="password" type="password" required minLength={8} value={password}
              onChange={e => setPassword(e.target.value)}
              className="mt-1 block w-full rounded-lg px-3 py-2 placeholder-gray-500 focus:outline-none focus:ring-1"
              style={{ backgroundColor: 'var(--color-surface)', border: '1px solid var(--color-border)', color: 'var(--color-text-primary)' }}
              placeholder="••••••••" />
          </div>
          <div>
            <label htmlFor="inviteCode" className="block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>Invite Code</label>
            <div className="relative mt-1">
              <div className="pointer-events-none absolute inset-y-0 left-0 flex items-center pl-3">
                <KeyRound className="h-4 w-4" style={{ color: 'var(--color-text-muted)' }} />
              </div>
              <input id="inviteCode" type="text" value={inviteCode}
                onChange={e => setInviteCode(e.target.value)}
                className="block w-full rounded-lg py-2 pl-10 pr-3 placeholder-gray-500 focus:outline-none focus:ring-1"
                style={{ backgroundColor: 'var(--color-surface)', border: '1px solid var(--color-border)', color: 'var(--color-text-primary)' }}
                placeholder="optional invite code" />
            </div>
          </div>
          <button type="submit" disabled={isLoading}
            className="flex w-full items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium text-white disabled:opacity-50"
            style={{ backgroundColor: 'var(--color-accent)' }}>
            {isLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : <UserPlus className="h-4 w-4" />}
            Register
          </button>
          <p className="text-center text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Already have an account?{' '}
            <Link to="/login" style={{ color: 'var(--color-accent)' }} className="hover:opacity-80">Sign in</Link>
          </p>
        </form>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Simplify Login.tsx — remove isRegister toggle**

Replace `web-ui/src/pages/Login.tsx` — keep the login-only form, change the bottom text to link to `/register`:

```tsx
import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { LogIn, Loader2 } from 'lucide-react'
import { toast } from 'sonner'
import { useAuthStore } from '../stores/auth'

export default function Login() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const { login, isLoading, error, clearError } = useAuthStore()
  const navigate = useNavigate()

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    await login(username, password)
    const state = useAuthStore.getState()
    if (state.isAuthenticated) {
      toast.success('Logged in!')
      navigate('/dashboard', { replace: true })
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4" style={{ backgroundColor: 'var(--color-bg)' }}>
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <h1 className="text-4xl font-bold" style={{
            background: 'linear-gradient(135deg, #3b82f6, #8b5cf6)',
            WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent',
          }}>
            PicHost
          </h1>
          <p className="mt-1 text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Self-hosted image hosting
          </p>
        </div>
        <form onSubmit={handleSubmit} className="space-y-4 rounded-xl p-6"
          style={{
            backgroundColor: 'var(--glass-bg)', border: '1px solid var(--glass-border)',
            backdropFilter: 'blur(var(--glass-blur))', boxShadow: 'var(--glass-shadow)',
          }}>
          {error && (
            <div className="rounded-lg px-4 py-2 text-sm"
              style={{ backgroundColor: 'var(--color-danger-subtle)', color: 'var(--color-danger)' }}>
              {error}
            </div>
          )}
          <div>
            <label htmlFor="username" className="block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>Username</label>
            <input id="username" type="text" required minLength={3} value={username}
              onChange={e => setUsername(e.target.value)}
              className="mt-1 block w-full rounded-lg px-3 py-2 placeholder-gray-500 focus:outline-none focus:ring-1"
              style={{ backgroundColor: 'var(--color-surface)', border: '1px solid var(--color-border)', color: 'var(--color-text-primary)' }}
              placeholder="your username" />
          </div>
          <div>
            <label htmlFor="password" className="block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>Password</label>
            <input id="password" type="password" required minLength={8} value={password}
              onChange={e => setPassword(e.target.value)}
              className="mt-1 block w-full rounded-lg px-3 py-2 placeholder-gray-500 focus:outline-none focus:ring-1"
              style={{ backgroundColor: 'var(--color-surface)', border: '1px solid var(--color-border)', color: 'var(--color-text-primary)' }}
              placeholder="••••••••" />
          </div>
          <button type="submit" disabled={isLoading}
            className="flex w-full items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium text-white disabled:opacity-50"
            style={{ backgroundColor: 'var(--color-accent)' }}>
            {isLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : <LogIn className="h-4 w-4" />}
            Sign In
          </button>
          <p className="text-center text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Don't have an account?{' '}
            <Link to="/register" style={{ color: 'var(--color-accent)' }} className="hover:opacity-80">Register</Link>
          </p>
        </form>
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Add /register route to App.tsx**

In `web-ui/src/App.tsx`, add import and route:

```tsx
import Register from './pages/Register'
```

Add the route after the `<Route path="/login">` block:

```tsx
        <Route path="/register" element={<Register />} />
```

- [ ] **Step 4: Build frontend**

```bash
cd web-ui && npm run build
```
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/pages/Register.tsx web-ui/src/pages/Login.tsx web-ui/src/App.tsx
git commit -m "feat(web): extract Register page, add /register route, simplify Login"
```

---

## Unit C: 前端路由/路径对齐

### Task C1: backend — /t/{public_key} thumbnail alias

**Files:**
- Modify: `pichost-api/src/routes/images.rs` (add handler)
- Modify: `pichost-api/src/main.rs` (add route)

- [ ] **Step 1: Add `public_get_thumb_by_key` handler**

In `pichost-api/src/routes/images.rs`, add after `public_get_thumb` (around line 300):

```rust
/// GET /t/{public_key} — serve thumbnail by public_key (alias)
pub async fn public_get_thumb_by_key(
    State(state): State<Arc<AppState>>,
    Path(public_key): Path<String>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let row: (Option<String>, String) = sqlx::query_as(
        "SELECT thumbnail_key, storage_backend FROM images \
         WHERE public_key = $1 AND status IN ('active', 'ready')",
    )
    .bind(&public_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Thumbnail-by-key query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal server error"})),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "image not found"})),
        )
    })?;

    let (thumb_key, storage_backend) = row;
    let thumb_key = thumb_key.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "thumbnail not yet generated"})),
        )
    })?;

    let backend = state.router.for_backend(&storage_backend);
    let bytes = state
        .cache
        .cached_thumb(&format!("thumb:pk:{}", public_key), 3600, async {
            backend.get(&thumb_key).await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "thumbnail not found"})),
                )
            })
        })
        .await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_for_thumb_key(&thumb_key))
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}
```

- [ ] **Step 2: Register /t route in main.rs**

In `pichost-api/src/main.rs`, add in `build_router()` (after line 150, before the `/api/health` line):

```rust
        .nest("/t", public_routes_thumb(state.clone()))
```

And add the helper function after `public_routes`:

```rust
fn public_routes_thumb(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/{public_key}", get(routes::images::public_get_thumb_by_key))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_public,
        ))
}
```

Actually, simpler approach: just add the route to the existing `public_routes` function — but that would collide with the existing `/{public_key}` route. So a separate `/t` nest is cleaner.

Wait, the existing public_routes is nested under `/u`. So `/t` needs its own nest. Let me think about the simplest approach.

Actually the simplest: add a `/t/{public_key}` route directly in `build_router()`. No need for a separate function. But that would require adding rate limiting too. Let me use the function approach.

- [ ] **Step 2 (revised): Add /t route nest**

In `pichost-api/src/main.rs`, add after `public_routes` function (line 140):

```rust
fn thumb_alias_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/{public_key}", get(routes::images::public_get_thumb_by_key))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_public,
        ))
}
```

In `build_router()` (line 143-159), add after `.nest("/u", public_routes(...))`:

```rust
        .nest("/t", thumb_alias_routes(state.clone()))
```

- [ ] **Step 3: Build and lint**

```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/images.rs pichost-api/src/main.rs
git commit -m "feat(api): add /t/{public_key} thumbnail alias route"
```

---

### Task C2: frontend — Admin subdirectory restructure

**Files:**
- Move: `web-ui/src/pages/AdminStats.tsx` → `web-ui/src/pages/admin/AdminStats.tsx`
- Move: `web-ui/src/pages/AdminUsers.tsx` → `web-ui/src/pages/admin/AdminUsers.tsx`
- Move: `web-ui/src/pages/AdminInvites.tsx` → `web-ui/src/pages/admin/AdminInvites.tsx`
- Modify: `web-ui/src/pages/Admin.tsx` (update imports)

- [ ] **Step 1: Create admin subdirectory and move files**

```bash
mkdir -p web-ui/src/pages/admin
git mv web-ui/src/pages/AdminStats.tsx web-ui/src/pages/admin/AdminStats.tsx
git mv web-ui/src/pages/AdminUsers.tsx web-ui/src/pages/admin/AdminUsers.tsx
git mv web-ui/src/pages/AdminInvites.tsx web-ui/src/pages/admin/AdminInvites.tsx
```

- [ ] **Step 2: Update Admin.tsx imports**

In `web-ui/src/pages/Admin.tsx`, change:

```tsx
import AdminStats from './AdminStats'
import AdminUsers from './AdminUsers'
import AdminInvites from './AdminInvites'
```

To:

```tsx
import AdminStats from './admin/AdminStats'
import AdminUsers from './admin/AdminUsers'
import AdminInvites from './admin/AdminInvites'
```

- [ ] **Step 3: Build frontend**

```bash
cd web-ui && npm run build
```
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add web-ui/src/pages/admin/ web-ui/src/pages/Admin.tsx
git commit -m "refactor(web): restructure admin pages into admin/ subdirectory"
```

---

## Unit D: 运维与补充

### Task D1: backend — GET /images/{id}/links endpoint

**Files:**
- Modify: `pichost-api/src/routes/images.rs` (add handler)
- Modify: `pichost-api/src/main.rs` (register route)

- [ ] **Step 1: Add `get_image_links` handler**

In `pichost-api/src/routes/images.rs`, add after the existing handlers:

```rust
#[derive(serde::Serialize)]
pub struct ImageLinks {
    pub url: String,
    pub markdown: String,
    pub html: String,
    pub bbcode: String,
}

/// GET /api/v1/images/{id}/links — get share link formats only
pub async fn get_image_links(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(image_id): Path<Uuid>,
) -> Result<Json<ImageLinks>, RouteError> {
    use crate::services::html_escape;

    let row: (String, String, String) = sqlx::query_as(
        "SELECT public_key, original_name, url FROM images WHERE id = $1 AND user_id = $2",
    )
    .bind(image_id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Image links query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal server error"})),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "image not found"})),
        )
    })?;

    let (_public_key, original_name, url) = row;
    let markdown = format!("![{}]({})", original_name, url);
    let html = format!("<img src=\"{}\" alt=\"{}\" />", url, html_escape(&original_name));
    let bbcode = format!("[img]{}[/img]", url);

    Ok(Json(ImageLinks { url, markdown, html, bbcode }))
}
```

- [ ] **Step 2: Register route in image_routes()**

In `pichost-api/src/main.rs`, in `image_routes()` (lines 77-92), add after the batch-delete line:

```rust
        .route("/{id}/links", get(routes::images::get_image_links))
```

- [ ] **Step 3: Build and lint**

```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/images.rs pichost-api/src/main.rs
git commit -m "feat(api): add GET /images/{id}/links endpoint for share link formats"
```

---

### Task D2: docker-compose.prod.yml

**Files:**
- Create: `docker-compose.prod.yml`

- [ ] **Step 1: Create production compose file**

Create `docker-compose.prod.yml`:

```yaml
# =============================================================================
# PicHost — Production Docker Compose (external PG/Redis/S3, SSL via Nginx)
# =============================================================================
#
# Usage: docker compose -f docker-compose.prod.yml up --build -d
#
# Prerequisites:
#   1. External PostgreSQL, Redis, and S3-compatible storage
#   2. SSL certificate at ./nginx/certs/fullchain.pem and ./nginx/certs/privkey.pem
#   3. .env with PICHOST_AUTH_JWT_SECRET (min 32 chars) and connection strings

services:
  # ── Nginx — reverse proxy + SSL termination + CDN cache ──
  nginx:
    image: nginx:1.27-alpine
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx/nginx.prod.conf:/etc/nginx/nginx.conf:ro
      - ./nginx/certs:/etc/nginx/certs:ro
      - ./web-ui/dist:/usr/share/nginx/html:ro
    depends_on:
      - api
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost/health"]
      interval: 10s
      timeout: 5s
      retries: 3

  # ── API (stateless, scale as needed) ──
  api:
    build:
      dockerfile: Dockerfile.api
      context: .
    restart: unless-stopped
    deploy:
      replicas: 4
    environment:
      PICHOST_DATABASE_URL: ${PICHOST_DATABASE_URL}
      PICHOST_REDIS_URL: ${PICHOST_REDIS_URL}
      PICHOST_AUTH_JWT_SECRET: ${PICHOST_AUTH_JWT_SECRET}
      PICHOST_SERVER_PUBLIC_URL: ${PICHOST_SERVER_PUBLIC_URL}
      PICHOST_STORAGE_DEFAULT_BACKEND: ${PICHOST_STORAGE_DEFAULT_BACKEND:-s3}
      PICHOST_STORAGE_LOCAL_BASE_PATH: /app/storage-local
      PICHOST_STORAGE_RUSTFS_ENDPOINT: ${PICHOST_STORAGE_RUSTFS_ENDPOINT}
      PICHOST_STORAGE_RUSTFS_BUCKET: ${PICHOST_STORAGE_RUSTFS_BUCKET}
      PICHOST_STORAGE_RUSTFS_ACCESS_KEY: ${PICHOST_STORAGE_RUSTFS_ACCESS_KEY}
      PICHOST_STORAGE_RUSTFS_SECRET_KEY: ${PICHOST_STORAGE_RUSTFS_SECRET_KEY}
      PICHOST_STORAGE_RUSTFS_REGION: ${PICHOST_STORAGE_RUSTFS_REGION:-us-east-1}
    secrets:
      - jwt_secret

  # ── Worker (background image processing) ──
  worker:
    build:
      dockerfile: Dockerfile.worker
      context: .
    restart: unless-stopped
    deploy:
      replicas: 4
    environment:
      PICHOST_DATABASE_URL: ${PICHOST_DATABASE_URL}
      PICHOST_REDIS_URL: ${PICHOST_REDIS_URL}
      PICHOST_AUTH_JWT_SECRET: ${PICHOST_AUTH_JWT_SECRET}
      PICHOST_SERVER_PUBLIC_URL: ${PICHOST_SERVER_PUBLIC_URL}
      PICHOST_STORAGE_DEFAULT_BACKEND: ${PICHOST_STORAGE_DEFAULT_BACKEND:-s3}
      PICHOST_STORAGE_LOCAL_BASE_PATH: /app/storage-local
      PICHOST_STORAGE_RUSTFS_ENDPOINT: ${PICHOST_STORAGE_RUSTFS_ENDPOINT}
      PICHOST_STORAGE_RUSTFS_BUCKET: ${PICHOST_STORAGE_RUSTFS_BUCKET}
      PICHOST_STORAGE_RUSTFS_ACCESS_KEY: ${PICHOST_STORAGE_RUSTFS_ACCESS_KEY}
      PICHOST_STORAGE_RUSTFS_SECRET_KEY: ${PICHOST_STORAGE_RUSTFS_SECRET_KEY}
      PICHOST_STORAGE_RUSTFS_REGION: ${PICHOST_STORAGE_RUSTFS_REGION:-us-east-1}
    secrets:
      - jwt_secret

secrets:
  jwt_secret:
    file: ${PICHOST_JWT_SECRET_FILE:-./secrets/jwt_secret.txt}
```

- [ ] **Step 2: Validate compose syntax**

```bash
docker compose -f docker-compose.prod.yml config --no-ansi > /dev/null
```
Expected: no errors (warning about undefined variables is OK).

- [ ] **Step 3: Commit**

```bash
git add docker-compose.prod.yml
git commit -m "feat(ops): add production docker-compose.prod.yml with SSL and external services"
```

---

### Task D3: core — UploadTask.max_retries

**Files:**
- Modify: `pichost-core/src/models.rs:60-71`

- [ ] **Step 1: Add max_retries to UploadTask**

In `pichost-core/src/models.rs`, modify the UploadTask struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadTask {
    pub id: Uuid,
    pub image_id: Uuid,
    pub task_type: String,
    pub payload: Option<serde_json::Value>,
    pub status: String,
    pub error: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
```

- [ ] **Step 2: Build and lint**

```bash
cargo check -p pichost-core
cargo clippy -p pichost-core -- -D warnings
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p pichost-core
```
Expected: all existing tests pass.

- [ ] **Step 4: Commit**

```bash
git add pichost-core/src/models.rs
git commit -m "feat(core): add max_retries field to UploadTask domain model"
```

---

## Final Verification

- [ ] `cargo clippy --workspace -- -D warnings` — zero warnings
- [ ] `cargo test --workspace` — 14 pass, 10 ignored, no regressions
- [ ] `cargo build --workspace` — all 3 crates build
- [ ] `cd web-ui && npm run build` — tsc + vite pass
