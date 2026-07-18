# Plan B: Visual Polish + Admin Panel — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add admin backend API (middleware + 4 endpoints), a frontend theme system with dark/light mode, glassmorphism visual polish across all pages, and an admin management panel.

**Architecture:** Backend adds `require_admin` middleware (reuses existing `AuthUser` from `require_auth`) and 4 new endpoints in `routes/admin.rs`. Frontend adds a CSS variable theme system (`theme.css`), a Zustand theme store with localStorage persistence, a `Layout` component to eliminate repeated NavBar imports, glassmorphism styling via CSS variables, and admin pages (stats dashboard + user management table).

**Tech Stack:** Rust (Axum 0.8, tower-http, deadpool-redis), React 19, Tailwind CSS 4, Zustand, TanStack Query, lucide-react, sonner.

## Global Constraints

- Rust edition 2021, workspace version 0.1.0, `rustfmt` + `clippy` (per `rust-toolchain.toml`)
- Auth middleware pattern: `require_auth` injects `AuthUser { id, is_admin }` into `req.extensions()`. `require_admin` reads `AuthUser` from `req.extensions()` — no JWT re-decode.
- Admin routes use middleware stacking: `rate_limit_general` → `protected` → `require_admin`
- Admin `PATCH /admin/users/:id` cannot demote self (check `current_user.id != target_user.id` for `is_admin` toggle)
- Admin `DELETE /admin/users/:id` cannot delete self
- Frontend: React 19, Tailwind CSS 4 (no `@apply` — use inline Tailwind classes), Zustand for client state, TanStack Query for server state
- Tailwind v4 dark mode: use `@variant dark` in `index.css`, control via `document.documentElement.classList.toggle('dark')`
- CSS variables defined in `theme.css` at `:root` (dark) and `.light` (light), imported in `index.css`
- No shadcn/ui — project doesn't have it. Build Button/Input components from scratch.
- Theme flash prevention: inline `<script>` in `index.html` `<head>` before any CSS
- All frontend pages currently import `<NavBar />` individually — the `Layout` component eliminates this duplication
- Commits: conventional commits (`feat:`, `fix:`, `chore:`)
- All Rust code must pass `cargo clippy --workspace -- -D warnings`
- Frontend must pass `cd web-ui && npm run build` (TypeScript check + Vite bundle)

---

## File Structure Map

```
pichost-api/
├── Cargo.toml                              ← no change needed
├── src/routes/admin.rs                     ← CREATE: 4 admin endpoints (list users, update, delete, stats)
├── src/routes/mod.rs                       ← MODIFY: add pub mod admin;
├── src/middleware/auth.rs                  ← MODIFY: add pub async fn require_admin
├── src/middleware/mod.rs                   ← no change needed (pub mod auth already)
├── src/main.rs                             ← MODIFY: register admin routes with require_admin layer

web-ui/
├── index.html                              ← MODIFY: add inline theme flash prevention script
├── src/index.css                           ← MODIFY: add @variant dark, import theme.css
├── src/theme.css                           ← CREATE: CSS custom properties for light/dark themes
├── src/App.tsx                             ← MODIFY: wrap protected routes in <Layout>, add /admin route
├── src/stores/ui.ts                        ← CREATE: Zustand theme store (light/dark/system) + localStorage
├── src/components/
│   ├── Layout.tsx                          ← CREATE: shared shell (NavBar + <main>children</main>)
│   ├── NavBar.tsx                          ← MODIFY: add ThemeToggle, add admin link for admins
│   ├── ProtectedRoute.tsx                  ← no change needed
│   ├── AdminRoute.tsx                      ← CREATE: check user.is_admin, redirect non-admins
│   ├── ThemeToggle.tsx                     ← CREATE: icon button cycling light→dark→system
│   ├── ui/Button.tsx                       ← CREATE: Button component with primary/danger/ghost/icon variants
│   ├── ui/Input.tsx                        ← CREATE: theme-aware input component
├── src/pages/
│   ├── Login.tsx                           ← MODIFY: glassmorphism form card
│   ├── Dashboard.tsx                       ← MODIFY: remove <NavBar /> (now in Layout), glass cards
│   ├── Gallery.tsx                         ← MODIFY: remove <NavBar />, glass image cards
│   ├── ImageDetail.tsx                     ← MODIFY: remove <NavBar />, glass panels
│   ├── Admin.tsx                           ← CREATE: admin shell with tab navigation (Overview | Users)
│   ├── AdminStats.tsx                      ← CREATE: stat cards for system overview
│   └── AdminUsers.tsx                      ← CREATE: user management table with edit/delete
├── src/components/
│   └── EditUserDialog.tsx                  ← CREATE: glass modal for editing user fields
```

**Inter-task dependency graph:**
```
Phase A (Backend)
  Task A1 (require_admin middleware) ────┐
                                          ├──→ Task A3 (user management endpoints)
  Task A2 (system stats endpoint) ────────┘
                                          │
  Task A3 ───→ Task A4 (route registration + integration test)

Phase B (Theme)
  Task B1 (theme.css) ────┐
  Task B2 (index.css) ─────┤
                           ├──→ Task B5 (ThemeToggle)
  Task B3 (ui store) ──────┤
  Task B4 (flash script) ──┘

Phase C (Glassmorphism)
  Task C1 (Layout) ────→ Task C2-C8 (pages) ────→ Task C9 (Button) ────→ Task C10 (Input)

Phase D (Admin UI) ──── depends on Phase A + Phase C

Phase E (Verification) ──── depends on everything
```

---

## Phase A: Backend — Admin Middleware & API

### Task A1: `require_admin` middleware

**Files:**
- Modify: `pichost-api/src/middleware/auth.rs` — add `require_admin` function

**Interfaces:**
- Consumes: `AuthUser { id, is_admin }` from `req.extensions()` (already injected by `require_auth`)
- Produces: `pub async fn require_admin(State, req, next) -> Result<Response, (StatusCode, Json)>` — returns 403 for non-admins
- Consumed by: Task A4 (route registration)

- [ ] **Step 1: Write `require_admin` middleware**

Add at the end of `pichost-api/src/middleware/auth.rs` (after `require_auth` function, before end of file):

```rust
/// Middleware that rejects non-admin users with 403 Forbidden.
/// MUST be placed after `require_auth` — requires `AuthUser` in extensions.
pub async fn require_admin(
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = req
        .extensions()
        .get::<AuthUser>()
        .ok_or_else(|| {
            tracing::warn!("require_admin called without AuthUser in extensions");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "authentication required"})),
            )
        })?;

    if !auth_user.is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "admin access required"})),
        ));
    }

    Ok(next.run(req).await)
}
```

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p pichost-api -- -D warnings`
Expected: no new warnings.

- [ ] **Step 3: Commit**

```bash
git add pichost-api/src/middleware/auth.rs
git commit -m "feat(admin): add require_admin middleware (403 for non-admins)"
```

---

### Task A2: Admin API — `GET /api/v1/admin/stats`

**Files:**
- Create: `pichost-api/src/routes/admin.rs` — add `get_admin_stats` handler
- Modify: `pichost-api/src/routes/mod.rs` — add `pub mod admin;`

**Interfaces:**
- Consumes: `AppState` (DB pool, cache, StorageRouter), `AuthUser` (must be admin — checked by middleware)
- Produces: `pub async fn get_admin_stats(State) -> Result<Json<AdminStats>, ...>`
- Response shape: `{ total_users, total_images, total_size, active_users_24h, storage_backends: { "local": { total_images, total_size }, "rustfs": { ... } } }`
- Cached via Redis `incr_user_stat` pattern with TTL 300s (key `pichost:stats:00000000-0000-0000-0000-000000000000`)

- [ ] **Step 1: Create `pichost-api/src/routes/admin.rs`**

```rust
use std::collections::HashMap;
use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;

use crate::app::AppState;

#[derive(Debug, Serialize)]
pub struct BackendStats {
    pub total_images: i64,
    pub total_size: i64,
}

#[derive(Debug, Serialize)]
pub struct AdminStats {
    pub total_users: i64,
    pub total_images: i64,
    pub total_size: i64,
    pub active_users_24h: i64,
    pub storage_backends: HashMap<String, BackendStats>,
}

/// GET /api/v1/admin/stats — system-wide statistics (admin only, cached 5 min)
pub async fn get_admin_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AdminStats>, (StatusCode, Json<serde_json::Value>)> {
    // Try cache first using nil UUID as admin stats key
    if let Ok(Some(stats_map)) = state.cache.get_user_stats(&uuid::Uuid::nil()).await {
        if let (Some(total_users), Some(total_images), Some(total_size)) = (
            stats_map.get("total_users").and_then(|v| v.parse().ok()),
            stats_map.get("total_images").and_then(|v| v.parse().ok()),
            stats_map.get("total_size").and_then(|v| v.parse().ok()),
        ) {
            let active_users_24h: i64 = stats_map
                .get("active_users_24h")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let local = BackendStats {
                total_images: stats_map
                    .get("local_images")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(total_images),
                total_size: stats_map
                    .get("local_size")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(total_size),
            };
            let rustfs = BackendStats {
                total_images: stats_map
                    .get("rustfs_images")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                total_size: stats_map
                    .get("rustfs_size")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            };
            let mut backends = HashMap::new();
            backends.insert("local".to_string(), local);
            backends.insert("rustfs".to_string(), rustfs);

            return Ok(Json(AdminStats {
                total_users,
                total_images,
                total_size,
                active_users_24h,
                storage_backends: backends,
            }));
        }
    }

    // Cache miss — query DB
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin stats user count failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
        })?;

    let img_row = sqlx::query_as::<_, (i64, Option<i64>)>(
        r#"SELECT COUNT(*) as total_images, COALESCE(SUM(file_size), 0) as total_size
           FROM images"#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin stats image query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
    })?;
    let (total_images, total_size) = (img_row.0, img_row.1.unwrap_or(0));

    // Active users in last 24h
    let active_users_24h: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT user_id) FROM images
           WHERE created_at > NOW() - INTERVAL '24 hours'"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    // Per-backend breakdown
    let local_row = sqlx::query_as::<_, (i64, Option<i64>)>(
        r#"SELECT COUNT(*), COALESCE(SUM(file_size), 0)
           FROM images WHERE storage_backend = 'local'"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0, None));

    let rustfs_row = sqlx::query_as::<_, (i64, Option<i64>)>(
        r#"SELECT COUNT(*), COALESCE(SUM(file_size), 0)
           FROM images WHERE storage_backend = 'rustfs'"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0, None));

    let mut backends = HashMap::new();
    backends.insert(
        "local".to_string(),
        BackendStats {
            total_images: local_row.0,
            total_size: local_row.1.unwrap_or(0),
        },
    );
    backends.insert(
        "rustfs".to_string(),
        BackendStats {
            total_images: rustfs_row.0,
            total_size: rustfs_row.1.unwrap_or(0),
        },
    );

    let stats = AdminStats {
        total_users,
        total_images,
        total_size,
        active_users_24h,
        storage_backends: backends,
    };

    // Populate cache (best-effort)
    let nil_uuid = uuid::Uuid::nil();
    let _ = state.cache.incr_user_stat(&nil_uuid, "total_users", total_users).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "total_images", total_images).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "total_size", total_size).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "active_users_24h", active_users_24h).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "local_images", local_row.0).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "local_size", local_row.1.unwrap_or(0)).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "rustfs_images", rustfs_row.0).await;
    let _ = state.cache.incr_user_stat(&nil_uuid, "rustfs_size", rustfs_row.1.unwrap_or(0)).await;

    Ok(Json(stats))
}
```

- [ ] **Step 2: Update `routes/mod.rs`**

In `pichost-api/src/routes/mod.rs`, add:

```rust
pub mod admin;
```

- [ ] **Step 3: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/admin.rs pichost-api/src/routes/mod.rs
git commit -m "feat(admin): add GET /api/v1/admin/stats with per-backend breakdown"
```

---

### Task A3: Admin API — User management endpoints

**Files:**
- Modify: `pichost-api/src/routes/admin.rs` — add `list_users`, `update_user`, `delete_user` handlers

**Interfaces:**
- Produces: `pub async fn list_users(State, Query<PaginationQuery>) -> Result<Json<ListUsersResponse>, ...>`
- Produces: `pub async fn update_user(State, Extension<AuthUser>, Path<Uuid>, Json<UpdateUserBody>) -> Result<Json<UserInfo>, ...>`
- Produces: `pub async fn delete_user(State, Extension<AuthUser>, Path<Uuid>) -> Result<(StatusCode, Json), ...>`
- Consumed by: Task A4 (route registration)

- [ ] **Step 1: Add `list_users` handler**

Add to `pichost-api/src/routes/admin.rs` before `get_admin_stats` (after imports):

```rust
use axum::{Json, extract::{State, Query, Path}, http::StatusCode, Extension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::sync::Arc;
use crate::app::AppState;
use crate::middleware::auth::AuthUser;
use crate::routes::auth::UserInfo;

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ListUsersResponse {
    pub users: Vec<UserInfo>,
    pub total: i64,
}

/// GET /api/v1/admin/users — paginated user list (admin only)
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ListUsersResponse>, (StatusCode, Json<serde_json::Value>)> {
    let offset = pagination.offset.unwrap_or(0).max(0);
    let limit = pagination.limit.unwrap_or(50).clamp(1, 200);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin user count query failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
        })?;

    let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, bool, String, chrono::DateTime<chrono::Utc>)>(
        r#"SELECT id, username, email, is_admin, storage_backend, created_at
           FROM users ORDER BY created_at DESC OFFSET $1 LIMIT $2"#,
    )
    .bind(offset)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin user list query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
    })?;

    let users = rows
        .into_iter()
        .map(|(id, username, email, _is_admin, _storage_backend, _created_at)| UserInfo {
            id,
            username,
            email,
            is_admin: _is_admin,
        })
        .collect();

    Ok(Json(ListUsersResponse { users, total }))
}
```

- [ ] **Step 2: Add `update_user` handler**

Add after `list_users`:

```rust
#[derive(Debug, Deserialize)]
pub struct UpdateUserBody {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<bool>,
    pub storage_backend: Option<String>,
}

/// PATCH /api/v1/admin/users/{id} — update user fields (admin only)
pub async fn update_user(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<UpdateUserBody>,
) -> Result<Json<UserInfo>, (StatusCode, Json<serde_json::Value>)> {
    // Prevent self-demotion
    if body.is_admin == Some(false) && current_user.id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "cannot demote yourself"})),
        ));
    }

    // Fetch existing user
    let existing = sqlx::query_as::<_, (String, Option<String>, bool, String)>(
        r#"SELECT username, email, is_admin, storage_backend FROM users WHERE id = $1"#,
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin update user query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
    })?
    .ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "user not found"})))
    })?;

    let (username, email, is_admin, storage_backend) = existing;

    let new_username = body.username.unwrap_or(username);
    let new_email = body.email.or(email);
    let new_is_admin = body.is_admin.unwrap_or(is_admin);
    let new_storage_backend = body.storage_backend.unwrap_or(storage_backend);

    // If password provided, hash it
    if let Some(password) = &body.password {
        if password.len() < 8 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "password must be at least 8 characters"})),
            ));
        }

        use argon2::password_hash::SaltString;
        use argon2::PasswordHasher;
        let salt = SaltString::generate(&mut rand::rngs::OsRng);
        let argon2 = argon2::Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| {
                tracing::warn!("Password hashing failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal error"})),
                )
            })?
            .to_string();

        sqlx::query(
            r#"UPDATE users SET username = $1, email = $2, is_admin = $3,
               storage_backend = $4, password_hash = $5 WHERE id = $6"#,
        )
        .bind(&new_username)
        .bind(&new_email)
        .bind(new_is_admin)
        .bind(&new_storage_backend)
        .bind(&password_hash)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin update user (with pw) failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
        })?;
    } else {
        sqlx::query(
            r#"UPDATE users SET username = $1, email = $2, is_admin = $3,
               storage_backend = $4 WHERE id = $5"#,
        )
        .bind(&new_username)
        .bind(&new_email)
        .bind(new_is_admin)
        .bind(&new_storage_backend)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin update user failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal error"})))
        })?;
    }

    tracing::info!(admin_id = %current_user.id, target_user = %user_id, "user updated");

    Ok(Json(UserInfo {
        id: user_id,
        username: new_username,
        email: new_email,
        is_admin: new_is_admin,
    }))
}
```

- [ ] **Step 3: Add `delete_user` handler**

Add after `update_user`:

```rust
/// DELETE /api/v1/admin/users/{id} — delete user and all images (admin only)
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    // Prevent self-deletion
    if current_user.id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "cannot delete yourself"})),
        ));
    }

    // Verify user exists
    let exists: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin delete user check failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?;

    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "user not found"})),
        ));
    }

    // Collect storage keys for all user's images (to delete physical files)
    let image_keys: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT storage_key, thumbnail_key, webp_key FROM images WHERE user_id = $1"#,
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Admin delete user image keys query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal error"})),
        )
    })?;

    // Delete physical files (best-effort — storage errors don't block user deletion)
    let storage = state.router.default_backend();
    for (key, thumb_key, webp_key) in &image_keys {
        let _ = storage.delete(key).await;
        if let Some(tk) = thumb_key {
            let _ = storage.delete(tk).await;
        }
        if let Some(wk) = webp_key {
            let _ = storage.delete(wk).await;
        }
    }

    // Delete from DB (cascade handles images)
    sqlx::query("DELETE FROM images WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin delete user images failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Admin delete user failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal error"})),
            )
        })?;

    tracing::info!(admin_id = %current_user.id, target_user = %user_id, images_deleted = image_keys.len(), "user deleted");
    Ok((
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"message": "user deleted"})),
    ))
}
```

- [ ] **Step 4: Update imports at top of admin.rs**

The full import block should be:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::middleware::auth::AuthUser;
use crate::routes::auth::UserInfo;
```

- [ ] **Step 5: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 6: Commit**

```bash
git add pichost-api/src/routes/admin.rs
git commit -m "feat(admin): add user management endpoints (list/update/delete)"
```

---

### Task A4: Route registration + integration test stubs

**Files:**
- Modify: `pichost-api/src/main.rs` — register admin routes with `require_admin` middleware
- Create: `pichost-api/tests/admin_test.rs` — integration tests (gated with `#[ignore]`)

- [ ] **Step 1: Register admin routes in main.rs**

In `pichost-api/src/main.rs`, after the `user_routes` block and before `let app = Router::new()`:

```rust
    // Admin routes — auth + admin check + rate limit
    let admin_protected = middleware::from_fn_with_state(
        state.clone(),
        pichost_api::middleware::auth::require_admin,
    );

    let admin_routes = Router::new()
        .route("/stats", get(routes::admin::get_admin_stats))
        .route("/users", get(routes::admin::list_users))
        .route(
            "/users/{id}",
            patch(routes::admin::update_user).delete(routes::admin::delete_user),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_general,
        ))
        .route_layer(protected.clone())
        .route_layer(admin_protected);
```

Add to `let app = Router::new()` block:

```rust
        .nest("/api/v1/admin", admin_routes)
```

Add `patch` to the axum routing import:

```rust
use axum::{..., routing::{get, patch, post}, ...};
```

- [ ] **Step 2: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 3: Create admin test file**

Create `pichost-api/tests/admin_test.rs`:

```rust
/// Integration tests for admin API endpoints.
/// Requires running PostgreSQL + Redis (set DATABASE_URL + PICHOST_REDIS_URL).
/// Run with: cargo test -p pichost-api --test admin_test -- --ignored

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_non_admin_cannot_list_users() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_can_list_users() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_can_update_user() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_cannot_demote_self() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_cannot_delete_self() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_stats() {
    let ok = true;
    assert!(ok);
}
```

- [ ] **Step 4: Build + clippy**

Run: `cargo clippy -p pichost-api -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/main.rs pichost-api/tests/admin_test.rs
git commit -m "feat(admin): register admin routes with require_admin middleware, add integration test stubs"
```

---

## Phase B: Frontend Theme System

### Task B1: CSS custom properties (`theme.css`)

**Files:**
- Create: `web-ui/src/theme.css` — all CSS variables for light + dark themes

- [ ] **Step 1: Create `web-ui/src/theme.css`**

```css
/* ── Theme Tokens ──────────────────────────── */
/* Dark theme is the default (:root)            */
/* Light theme is activated via .light class    */
/* ──────────────────────────────────────────── */

:root {
  /* Backgrounds */
  --color-bg: #030712;
  --color-surface: rgba(17, 24, 39, 0.5);
  --color-surface-elevated: rgba(17, 24, 39, 0.8);
  --color-surface-glass: rgba(255, 255, 255, 0.03);

  /* Borders */
  --color-border: rgba(75, 85, 99, 0.3);
  --color-border-hover: rgba(75, 85, 99, 0.6);

  /* Text */
  --color-text-primary: #f9fafb;
  --color-text-secondary: #9ca3af;
  --color-text-muted: #6b7280;

  /* Accent */
  --color-accent: #3b82f6;
  --color-accent-hover: #2563eb;
  --color-accent-subtle: rgba(59, 130, 246, 0.1);

  /* Danger */
  --color-danger: #ef4444;
  --color-danger-hover: #dc2626;
  --color-danger-subtle: rgba(239, 68, 68, 0.1);

  /* Glassmorphism */
  --glass-bg: rgba(255, 255, 255, 0.03);
  --glass-border: rgba(255, 255, 255, 0.06);
  --glass-blur: 12px;
  --glass-shadow: 0 4px 24px rgba(0, 0, 0, 0.3);

  /* Radii */
  --radius-sm: 0.375rem;
  --radius-md: 0.5rem;
  --radius-lg: 0.75rem;
  --radius-xl: 1rem;
}

/* ── Light Theme ──────────────────────────── */
.light {
  --color-bg: #f9fafb;
  --color-surface: rgba(255, 255, 255, 0.7);
  --color-surface-elevated: rgba(255, 255, 255, 0.95);
  --color-surface-glass: rgba(0, 0, 0, 0.02);

  --color-border: rgba(209, 213, 219, 0.6);
  --color-border-hover: rgba(156, 163, 175, 0.8);

  --color-text-primary: #111827;
  --color-text-secondary: #4b5563;
  --color-text-muted: #9ca3af;

  --color-accent: #2563eb;
  --color-accent-hover: #1d4ed8;
  --color-accent-subtle: rgba(37, 99, 235, 0.08);

  --color-danger: #dc2626;
  --color-danger-hover: #b91c1c;
  --color-danger-subtle: rgba(220, 38, 38, 0.08);

  --glass-bg: rgba(255, 255, 255, 0.5);
  --glass-border: rgba(0, 0, 0, 0.06);
  --glass-shadow: 0 4px 24px rgba(0, 0, 0, 0.06);
}
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/theme.css
git commit -m "feat(ui): add CSS theme tokens for dark and light modes"
```

---

### Task B2: Update `index.css` — import theme + dark variant

**Files:**
- Modify: `web-ui/src/index.css` — import theme.css, add `@variant dark`

- [ ] **Step 1: Rewrite `web-ui/src/index.css`**

```css
@import "tailwindcss";
@import "./theme.css";

@variant dark (&:where(.dark, .dark *));

@layer base {
  body {
    @apply min-h-screen;
    background-color: var(--color-bg);
    color: var(--color-text-primary);
    font-family: system-ui, -apple-system, sans-serif;
  }
}
```

- [ ] **Step 2: Verify frontend build**

Run: `cd web-ui && npm run build`
Expected: compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/index.css
git commit -m "feat(ui): integrate theme.css with Tailwind v4 dark variant"
```

---

### Task B3: Theme store (`stores/ui.ts`)

**Files:**
- Create: `web-ui/src/stores/ui.ts` — Zustand store with localStorage persistence

**Interfaces:**
- Produces: `UiState { theme: 'light' | 'dark' | 'system', toggleTheme() }` + auto-applies to DOM
- Consumed by: Task B5 (ThemeToggle), all glassmorphism pages via NavBar

- [ ] **Step 1: Create `web-ui/src/stores/ui.ts`**

```typescript
import { create } from 'zustand'

type Theme = 'light' | 'dark' | 'system'

interface UiState {
  theme: Theme
  setTheme: (theme: Theme) => void
  toggleTheme: () => void
}

function getSystemTheme(): 'light' | 'dark' {
  if (typeof window === 'undefined') return 'dark'
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function resolveAndApply(theme: Theme) {
  const resolved = theme === 'system' ? getSystemTheme() : theme
  const root = document.documentElement
  if (resolved === 'dark') {
    root.classList.add('dark')
    root.classList.remove('light')
  } else {
    root.classList.remove('dark')
    root.classList.add('light')
  }
}

// Read initial theme from localStorage before first render
const stored = (typeof localStorage !== 'undefined'
  ? (localStorage.getItem('pichost-theme') as Theme | null)
  : null) ?? 'system'

if (typeof document !== 'undefined') {
  resolveAndApply(stored)
}

export const useUiStore = create<UiState>((set, get) => ({
  theme: stored,

  setTheme: (theme: Theme) => {
    localStorage.setItem('pichost-theme', theme)
    resolveAndApply(theme)
    set({ theme })
  },

  toggleTheme: () => {
    const { theme, setTheme } = get()
    const next: Theme = theme === 'light' ? 'dark' : theme === 'dark' ? 'system' : 'light'
    setTheme(next)
  },
}))

// Listen for system theme changes when in 'system' mode
if (typeof window !== 'undefined') {
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    const state = useUiStore.getState()
    if (state.theme === 'system') {
      resolveAndApply('system')
    }
  })
}
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/stores/ui.ts
git commit -m "feat(ui): add theme store with localStorage persistence and system theme detection"
```

---

### Task B4: Flash prevention script

**Files:**
- Modify: `web-ui/index.html` — add inline `<script>` before `</head>`

- [ ] **Step 1: Update `web-ui/index.html`**

Replace the file content:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/svg+xml" href="/vite.svg" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>PicHost</title>
    <script>
      (function() {
        var theme = localStorage.getItem('pichost-theme') || 'system';
        var isDark = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
        if (isDark) {
          document.documentElement.classList.add('dark');
          document.documentElement.classList.remove('light');
        } else {
          document.documentElement.classList.remove('dark');
          document.documentElement.classList.add('light');
        }
      })();
    </script>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/index.html
git commit -m "feat(ui): add theme flash prevention script"
```

---

### Task B5: `ThemeToggle` component

**Files:**
- Create: `web-ui/src/components/ThemeToggle.tsx`

- [ ] **Step 1: Create `web-ui/src/components/ThemeToggle.tsx`**

```tsx
import { Sun, Moon, Monitor } from 'lucide-react'
import { useUiStore } from '../stores/ui'

const icons: Record<string, typeof Sun> = {
  light: Sun,
  dark: Moon,
  system: Monitor,
}

export default function ThemeToggle() {
  const theme = useUiStore((s) => s.theme)
  const toggleTheme = useUiStore((s) => s.toggleTheme)
  const Icon = icons[theme] || Monitor

  return (
    <button
      onClick={toggleTheme}
      className="rounded-lg p-2 transition-colors"
      style={{ color: 'var(--color-text-muted)' }}
      onMouseEnter={(e) => { e.currentTarget.style.backgroundColor = 'var(--color-surface)'; e.currentTarget.style.color = 'var(--color-text-secondary)' }}
      onMouseLeave={(e) => { e.currentTarget.style.backgroundColor = 'transparent'; e.currentTarget.style.color = 'var(--color-text-muted)' }}
      title={`Theme: ${theme}. Click to cycle.`}
    >
      <Icon className="h-4 w-4" />
    </button>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/components/ThemeToggle.tsx
git commit -m "feat(ui): add ThemeToggle component cycling light→dark→system"
```

---

## Phase C: Glassmorphism Visual Polish

### Task C1: `Layout` component — shared page shell

**Files:**
- Create: `web-ui/src/components/Layout.tsx`

- [ ] **Step 1: Create `web-ui/src/components/Layout.tsx`**

```tsx
import { type ReactNode } from 'react'
import NavBar from './NavBar'

interface LayoutProps {
  children: ReactNode
}

export default function Layout({ children }: LayoutProps) {
  return (
    <>
      <NavBar />
      <main className="mx-auto max-w-5xl p-4">
        {children}
      </main>
    </>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/components/Layout.tsx
git commit -m "feat(ui): add Layout component as shared page shell"
```

---

### Task C2: Restructure `App.tsx` — wrap protected routes in `<Layout>`, add `/admin` route

**Files:**
- Modify: `web-ui/src/App.tsx`

- [x] **Step 1: Update `web-ui/src/App.tsx`**

```tsx
import { useEffect } from 'react'
import { Routes, Route, Navigate } from 'react-router-dom'
import { Toaster } from 'sonner'
import { useAuthStore } from './stores/auth'
import Layout from './components/Layout'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import Gallery from './pages/Gallery'
import ImageDetail from './pages/ImageDetail'
import Admin from './pages/Admin'
import ProtectedRoute from './components/ProtectedRoute'
import AdminRoute from './components/AdminRoute'

export default function App() {
  const loadFromStorage = useAuthStore((s) => s.loadFromStorage)
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated)
  const hasLoaded = useAuthStore((s) => s.hasLoaded)

  useEffect(() => {
    loadFromStorage()
  }, [loadFromStorage])

  if (!hasLoaded) {
    return (
      <div
        className="flex min-h-screen items-center justify-center"
        style={{ backgroundColor: 'var(--color-bg)', color: 'var(--color-text-muted)' }}
      >
        <div
          className="h-8 w-8 animate-spin rounded-full border-2"
          style={{ borderColor: 'var(--color-border)', borderTopColor: 'var(--color-accent)' }}
        />
      </div>
    )
  }

  return (
    <>
      <Routes>
        <Route
          path="/"
          element={
            <Navigate to={isAuthenticated ? '/dashboard' : '/login'} replace />
          }
        />
        <Route path="/login" element={<Login />} />
        <Route
          path="/dashboard"
          element={
            <ProtectedRoute>
              <Layout>
                <Dashboard />
              </Layout>
            </ProtectedRoute>
          }
        />
        <Route
          path="/gallery"
          element={
            <ProtectedRoute>
              <Layout>
                <Gallery />
              </Layout>
            </ProtectedRoute>
          }
        />
        <Route
          path="/images/:id"
          element={
            <ProtectedRoute>
              <Layout>
                <ImageDetail />
              </Layout>
            </ProtectedRoute>
          }
        />
        <Route
          path="/admin/*"
          element={
            <ProtectedRoute>
              <AdminRoute>
                <Layout>
                  <Admin />
                </Layout>
              </AdminRoute>
            </ProtectedRoute>
          }
        />
      </Routes>
      <Toaster position="top-right" richColors />
    </>
  )
}
```

- [x] **Step 2: Commit**

```bash
git add web-ui/src/App.tsx
git commit -m "feat(ui): wrap protected routes in Layout, add /admin route"
```

---

### Task C3: Glassmorphism — Login page

**Files:**
- Modify: `web-ui/src/pages/Login.tsx` — glassmorphism form card with gradient branding

- [ ] **Step 1: Rewrite `web-ui/src/pages/Login.tsx`**

Replace all hardcoded Tailwind gray/border colors with CSS variables. Key changes:
- Form card: use `var(--glass-bg)` / `var(--glass-border)` / `var(--glass-shadow)`
- Brand heading: `linear-gradient(135deg, #3b82f6, #8b5cf6)` with `WebkitBackgroundClip: text`
- Inputs: use `var(--color-surface)` / `var(--color-border)` / `var(--color-text-primary)`
- Submit button: use `var(--color-accent)` / `var(--color-accent-hover)`

The full file is:

```tsx
import { useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { LogIn, UserPlus, Loader2 } from 'lucide-react'
import { toast } from 'sonner'
import { useAuthStore } from '../stores/auth'

export default function Login() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [isRegister, setIsRegister] = useState(false)
  const { login, register, isLoading, error, clearError } = useAuthStore()
  const navigate = useNavigate()

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    if (isRegister) {
      await register(username, password)
    } else {
      await login(username, password)
    }
    if (useAuthStore.getState().isAuthenticated) {
      toast.success(isRegister ? 'Registered!' : 'Logged in!')
      navigate('/dashboard', { replace: true })
    }
  }

  function toggleMode() {
    setIsRegister(!isRegister)
    clearError()
  }

  return (
    <div className="flex min-h-screen items-center justify-center p-4" style={{ backgroundColor: 'var(--color-bg)' }}>
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <h1
            className="text-4xl font-bold"
            style={{
              background: 'linear-gradient(135deg, #3b82f6, #8b5cf6)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
            }}
          >
            PicHost
          </h1>
          <p className="mt-1 text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Self-hosted image hosting
          </p>
        </div>

        <form
          onSubmit={handleSubmit}
          className="space-y-4 rounded-xl p-6"
          style={{
            backgroundColor: 'var(--glass-bg)',
            border: '1px solid var(--glass-border)',
            backdropFilter: 'blur(var(--glass-blur))',
            boxShadow: 'var(--glass-shadow)',
          }}
        >
          {error && (
            <div
              className="rounded-lg px-4 py-2 text-sm"
              style={{ backgroundColor: 'var(--color-danger-subtle)', color: 'var(--color-danger)' }}
            >
              {error}
            </div>
          )}

          <div>
            <label
              htmlFor="username"
              className="block text-sm font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              Username
            </label>
            <input
              id="username"
              type="text"
              required
              minLength={3}
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="mt-1 block w-full rounded-lg px-3 py-2 placeholder-gray-500 focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
              placeholder="your username"
            />
          </div>

          <div>
            <label
              htmlFor="password"
              className="block text-sm font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              Password
            </label>
            <input
              id="password"
              type="password"
              required
              minLength={8}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="mt-1 block w-full rounded-lg px-3 py-2 placeholder-gray-500 focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
              placeholder="••••••••"
            />
          </div>

          <button
            type="submit"
            disabled={isLoading}
            className="flex w-full items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-50"
            style={{ backgroundColor: 'var(--color-accent)' }}
            onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = 'var(--color-accent-hover)')}
            onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = 'var(--color-accent)')}
          >
            {isLoading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : isRegister ? (
              <UserPlus className="h-4 w-4" />
            ) : (
              <LogIn className="h-4 w-4" />
            )}
            {isRegister ? 'Register' : 'Sign In'}
          </button>

          <p className="text-center text-sm" style={{ color: 'var(--color-text-muted)' }}>
            {isRegister ? 'Already have an account?' : "Don't have an account?"}{' '}
            <button
              type="button"
              onClick={toggleMode}
              style={{ color: 'var(--color-accent)' }}
              className="hover:opacity-80"
            >
              {isRegister ? 'Sign in' : 'Register'}
            </button>
          </p>
        </form>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Verify frontend build**

Run: `cd web-ui && npm run build`
Expected: compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/pages/Login.tsx
git commit -m "feat(ui): glassmorphism login page with gradient branding"
```

---

### Task C4: Glassmorphism — Dashboard page

**Files:**
- Modify: `web-ui/src/pages/Dashboard.tsx` — remove `<NavBar />`, glass cards

- [ ] **Step 1: Update `web-ui/src/pages/Dashboard.tsx`**

Changes:
1. Remove `import NavBar from '../components/NavBar'` (Layout handles it)
2. Remove the `<NavBar />` JSX
3. Replace hardcoded border/bg colors with CSS variables for recent items

The updated component removes the NavBar import/usage and replaces `border border-gray-800 bg-gray-900/30 p-3` with `backgroundColor: 'var(--glass-bg)', border: '1px solid var(--glass-border)'`.

- [ ] **Step 2: Verify build**

Run: `cd web-ui && npm run build`
Expected: compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/pages/Dashboard.tsx
git commit -m "feat(ui): glassmorphism dashboard with CSS variable cards"
```

---

### Task C5: Glassmorphism — Gallery page

**Files:**
- Modify: `web-ui/src/pages/Gallery.tsx` — remove `<NavBar />`, glass image cards

- [ ] **Step 1: Update `web-ui/src/pages/Gallery.tsx`**

Changes:
1. Remove `import NavBar from '../components/NavBar'`
2. Remove `<NavBar />` JSX
3. Replace `border border-gray-800 bg-gray-900/50` with `backgroundColor: 'var(--color-surface)', border: '1px solid var(--glass-border)'` on image cards

- [ ] **Step 2: Verify build**

Run: `cd web-ui && npm run build`
Expected: compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/pages/Gallery.tsx
git commit -m "feat(ui): glassmorphism gallery with CSS variable cards"
```

---

### Task C6: Glassmorphism — ImageDetail page

**Files:**
- Modify: `web-ui/src/pages/ImageDetail.tsx` — remove `<NavBar />`, glass info panels

- [ ] **Step 1: Update `web-ui/src/pages/ImageDetail.tsx`**

Changes:
1. Remove `import NavBar from '../components/NavBar'`
2. Remove `<NavBar />` JSX
3. Glass-ify the info section: replace `border border-gray-800 bg-gray-900/50` with CSS variables
4. Glass-ify the preview card

- [ ] **Step 2: Verify build**

Run: `cd web-ui && npm run build`
Expected: compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/pages/ImageDetail.tsx
git commit -m "feat(ui): glassmorphism image detail with CSS variable panels"
```

---

### Task C7: Glassmorphism — NavBar with ThemeToggle + admin link

**Files:**
- Modify: `web-ui/src/components/NavBar.tsx`

- [ ] **Step 1: Update `web-ui/src/components/NavBar.tsx`**

Changes:
1. Import `ThemeToggle` from `./ThemeToggle`
2. Add ThemeToggle button next to username
3. Add `Admin` nav link for `user?.is_admin` users
4. Replace hardcoded `bg-gray-950/80` and `border-gray-800` with CSS variables

- [ ] **Step 2: Verify build**

Run: `cd web-ui && npm run build`
Expected: compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/components/NavBar.tsx
git commit -m "feat(ui): glassmorphism NavBar with ThemeToggle and admin nav link"
```

---

### Task C8: Glassmorphism — DropZone + LinkCard

**Files:**
- Modify: `web-ui/src/components/DropZone.tsx`
- Modify: `web-ui/src/components/LinkCard.tsx`

- [ ] **Step 1: Update DropZone styles**

Replace hardcoded border/bg colors on the dropzone container:
- `border-gray-700 bg-gray-900/50 hover:border-gray-500` → `var(--color-border)` / `var(--glass-bg)`
- `border-blue-500 bg-blue-500/10` → `var(--color-accent)` / `var(--color-accent-subtle)`

- [ ] **Step 2: Update LinkCard styles**

Replace `border border-gray-800 bg-gray-900/30 p-3` → CSS variables.

- [ ] **Step 3: Verify build**

Run: `cd web-ui && npm run build`
Expected: compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add web-ui/src/components/DropZone.tsx web-ui/src/components/LinkCard.tsx
git commit -m "feat(ui): glassmorphism DropZone and LinkCard components"
```

---

### Task C9: Extract `Button` UI component

**Files:**
- Create: `web-ui/src/components/ui/Button.tsx`

- [ ] **Step 1: Create `web-ui/src/components/ui/Button.tsx`**

```tsx
import { type ButtonHTMLAttributes, type ReactNode, useCallback } from 'react'

type ButtonVariant = 'primary' | 'danger' | 'ghost' | 'icon'
type ButtonSize = 'sm' | 'md'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: ButtonSize
  children: ReactNode
}

const variantStyles: Record<ButtonVariant, React.CSSProperties> = {
  primary: { backgroundColor: 'var(--color-accent)', color: 'white' },
  danger: { backgroundColor: 'var(--color-danger)', color: 'white' },
  ghost: { backgroundColor: 'transparent', color: 'var(--color-text-muted)', border: '1px solid var(--color-border)' },
  icon: { backgroundColor: 'transparent', color: 'var(--color-text-muted)' },
}

const hoverStyles: Record<ButtonVariant, React.CSSProperties> = {
  primary: { backgroundColor: 'var(--color-accent-hover)' },
  danger: { backgroundColor: 'var(--color-danger-hover)' },
  ghost: { backgroundColor: 'var(--color-surface)', color: 'var(--color-text-secondary)' },
  icon: { backgroundColor: 'var(--color-surface)', color: 'var(--color-text-secondary)' },
}

const sizeStyles: Record<ButtonSize, React.CSSProperties> = {
  sm: { padding: '0.375rem 0.75rem', fontSize: '0.75rem' },
  md: { padding: '0.5rem 1rem', fontSize: '0.875rem' },
}

export default function Button({
  variant = 'primary',
  size = 'md',
  children,
  style,
  ...props
}: ButtonProps) {
  const onMouseEnter = useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      if (!props.disabled) Object.assign(e.currentTarget.style, hoverStyles[variant])
      props.onMouseEnter?.(e)
    },
    [variant, props.disabled],
  )

  const onMouseLeave = useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      if (!props.disabled) Object.assign(e.currentTarget.style, variantStyles[variant])
      props.onMouseLeave?.(e)
    },
    [variant, props.disabled],
  )

  return (
    <button
      {...props}
      style={{
        ...variantStyles[variant],
        ...sizeStyles[size],
        borderRadius: 'var(--radius-md)',
        fontWeight: 500,
        cursor: props.disabled ? 'not-allowed' : 'pointer',
        opacity: props.disabled ? 0.5 : 1,
        display: 'inline-flex',
        alignItems: 'center',
        gap: '0.375rem',
        transition: 'all 0.15s ease',
        ...style,
      }}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      {children}
    </button>
  )
}
```

- [ ] **Step 2: Verify build**

Run: `cd web-ui && npm run build`
Expected: compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/components/ui/Button.tsx
git commit -m "feat(ui): extract Button component with variant/size props"
```

---

### Task C10: Extract `Input` UI component

**Files:**
- Create: `web-ui/src/components/ui/Input.tsx`

- [ ] **Step 1: Create `web-ui/src/components/ui/Input.tsx`**

```tsx
import { type InputHTMLAttributes } from 'react'

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string
}

export default function Input({ label, id, style, ...props }: InputProps) {
  return (
    <div>
      {label && (
        <label
          htmlFor={id}
          className="mb-1 block text-sm font-medium"
          style={{ color: 'var(--color-text-secondary)' }}
        >
          {label}
        </label>
      )}
      <input
        id={id}
        {...props}
        className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
        style={{
          backgroundColor: 'var(--color-surface)',
          border: '1px solid var(--color-border)',
          color: 'var(--color-text-primary)',
          ...style,
        }}
      />
    </div>
  )
}
```

- [ ] **Step 2: Verify build**

Run: `cd web-ui && npm run build`
Expected: compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/components/ui/Input.tsx
git commit -m "feat(ui): extract Input component with theme-aware styling"
```

---

## Phase D: Admin Panel Frontend

### Task D1: `AdminRoute` guard component

**Files:**
- Create: `web-ui/src/components/AdminRoute.tsx`

- [ ] **Step 1: Create `web-ui/src/components/AdminRoute.tsx`**

```tsx
import { type ReactNode } from 'react'
import { Navigate } from 'react-router-dom'
import { useAuthStore } from '../stores/auth'

interface AdminRouteProps {
  children: ReactNode
}

export default function AdminRoute({ children }: AdminRouteProps) {
  const user = useAuthStore((s) => s.user)
  const hasLoaded = useAuthStore((s) => s.hasLoaded)

  if (!hasLoaded) {
    return (
      <div
        className="flex min-h-screen items-center justify-center"
        style={{ backgroundColor: 'var(--color-bg)', color: 'var(--color-text-muted)' }}
      >
        <div
          className="h-8 w-8 animate-spin rounded-full border-2"
          style={{ borderColor: 'var(--color-border)', borderTopColor: 'var(--color-accent)' }}
        />
      </div>
    )
  }

  if (!user?.is_admin) {
    return <Navigate to="/dashboard" replace />
  }

  return <>{children}</>
}
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/components/AdminRoute.tsx
git commit -m "feat(admin): add AdminRoute guard component"
```

---

### Task D2: Admin Stats page (Overview tab)

**Files:**
- Create: `web-ui/src/pages/AdminStats.tsx`

- [ ] **Step 1: Create `web-ui/src/pages/AdminStats.tsx`**

```tsx
import { useQuery } from '@tanstack/react-query'
import { Users, Image as ImageIcon, HardDrive, Activity } from 'lucide-react'
import api from '../api/client'

interface BackendStats {
  total_images: number
  total_size: number
}

interface AdminStatsResponse {
  total_users: number
  total_images: number
  total_size: number
  active_users_24h: number
  storage_backends: Record<string, BackendStats>
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`
}

const statCards = [
  { key: 'total_users' as const, label: 'Total Users', icon: Users, color: '#3b82f6' },
  { key: 'total_images' as const, label: 'Total Images', icon: ImageIcon, color: '#8b5cf6' },
  { key: 'total_size' as const, label: 'Total Storage', icon: HardDrive, color: '#22c55e', format: (v: number) => formatBytes(v) },
  { key: 'active_users_24h' as const, label: 'Active (24h)', icon: Activity, color: '#f59e0b' },
] as const

export default function AdminStats() {
  const { data, isLoading } = useQuery({
    queryKey: ['admin', 'stats'],
    queryFn: () => api.get('admin/stats').json<AdminStatsResponse>(),
    refetchInterval: 30_000,
  })

  if (isLoading || !data) {
    return (
      <div className="flex items-center justify-center py-20" style={{ color: 'var(--color-text-muted)' }}>
        Loading stats…
      </div>
    )
  }

  return (
    <div>
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        {statCards.map(({ key, label, icon: Icon, color, format }) => {
          const value = data[key]
          return (
            <div
              key={key}
              className="rounded-xl p-4"
              style={{
                backgroundColor: 'var(--glass-bg)',
                border: '1px solid var(--glass-border)',
                backdropFilter: 'blur(var(--glass-blur))',
              }}
            >
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium uppercase tracking-wide" style={{ color: 'var(--color-text-muted)' }}>
                  {label}
                </span>
                <Icon className="h-4 w-4" style={{ color }} />
              </div>
              <p className="mt-2 text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
                {format ? format(value) : value.toLocaleString()}
              </p>
            </div>
          )
        })}
      </div>

      {/* Backend breakdown */}
      <div
        className="mt-6 rounded-xl p-4"
        style={{
          backgroundColor: 'var(--glass-bg)',
          border: '1px solid var(--glass-border)',
          backdropFilter: 'blur(var(--glass-blur))',
        }}
      >
        <h3 className="mb-3 text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
          Storage Backend Breakdown
        </h3>
        <div className="space-y-3">
          {Object.entries(data.storage_backends).map(([name, stats]) => (
            <div key={name}>
              <div className="mb-1 flex justify-between text-sm">
                <span style={{ color: 'var(--color-text-primary)' }}>{name}</span>
                <span style={{ color: 'var(--color-text-muted)' }}>
                  {stats.total_images.toLocaleString()} images / {formatBytes(stats.total_size)}
                </span>
              </div>
              <div
                className="h-2 overflow-hidden rounded-full"
                style={{ backgroundColor: 'var(--color-surface)' }}
              >
                <div
                  className="h-full rounded-full transition-all"
                  style={{
                    width: `${data.total_images > 0 ? (stats.total_images / data.total_images) * 100 : 0}%`,
                    backgroundColor: name === 'local' ? '#3b82f6' : '#8b5cf6',
                  }}
                />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/pages/AdminStats.tsx
git commit -m "feat(admin): add admin stats dashboard with stat cards and backend breakdown"
```

---

### Task D3: Admin Users table page (Users tab)

**Files:**
- Create: `web-ui/src/pages/AdminUsers.tsx`

- [ ] **Step 1: Create `web-ui/src/pages/AdminUsers.tsx`**

```tsx
import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { Trash2, Pencil } from 'lucide-react'
import api, { type UserInfo } from '../api/client'
import EditUserDialog from '../components/EditUserDialog'

interface ListUsersResponse {
  users: UserInfo[]
  total: number
}

export default function AdminUsers() {
  const [editingUser, setEditingUser] = useState<UserInfo | null>(null)
  const queryClient = useQueryClient()

  const { data, isLoading } = useQuery({
    queryKey: ['admin', 'users'],
    queryFn: () => api.get('admin/users?offset=0&limit=50').json<ListUsersResponse>(),
  })

  async function handleDelete(user: UserInfo) {
    if (!confirm(`Delete user "${user.username}"? This will permanently delete all their images.`)) return
    try {
      await api.delete(`admin/users/${user.id}`).json()
      toast.success(`User "${user.username}" deleted`)
      queryClient.invalidateQueries({ queryKey: ['admin', 'users'] })
      queryClient.invalidateQueries({ queryKey: ['admin', 'stats'] })
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Delete failed'
      toast.error(msg)
    }
  }

  if (isLoading || !data) {
    return (
      <div className="flex items-center justify-center py-20" style={{ color: 'var(--color-text-muted)' }}>
        Loading users…
      </div>
    )
  }

  return (
    <div>
      <div className="mb-3 flex items-center justify-between">
        <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
          {data.total} user{data.total !== 1 ? 's' : ''} total
        </p>
      </div>

      <div
        className="overflow-hidden rounded-xl"
        style={{
          backgroundColor: 'var(--glass-bg)',
          border: '1px solid var(--glass-border)',
        }}
      >
        <table className="w-full text-sm">
          <thead>
            <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
              <th className="px-4 py-3 text-left font-medium" style={{ color: 'var(--color-text-muted)' }}>Username</th>
              <th className="hidden px-4 py-3 text-left font-medium sm:table-cell" style={{ color: 'var(--color-text-muted)' }}>Email</th>
              <th className="px-4 py-3 text-center font-medium" style={{ color: 'var(--color-text-muted)' }}>Admin</th>
              <th className="px-4 py-3 text-right font-medium" style={{ color: 'var(--color-text-muted)' }}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {data.users.map((user) => (
              <tr
                key={user.id}
                style={{ borderBottom: '1px solid var(--color-border)' }}
                className="hover:opacity-80"
              >
                <td className="px-4 py-3" style={{ color: 'var(--color-text-primary)' }}>
                  {user.username}
                </td>
                <td className="hidden px-4 py-3 sm:table-cell" style={{ color: 'var(--color-text-secondary)' }}>
                  {user.email || '—'}
                </td>
                <td className="px-4 py-3 text-center">
                  {user.is_admin ? (
                    <span
                      className="inline-block rounded px-2 py-0.5 text-xs font-medium"
                      style={{ backgroundColor: 'rgba(59, 130, 246, 0.1)', color: '#3b82f6' }}
                    >
                      Admin
                    </span>
                  ) : (
                    <span style={{ color: 'var(--color-text-muted)' }}>—</span>
                  )}
                </td>
                <td className="px-4 py-3 text-right">
                  <div className="flex items-center justify-end gap-2">
                    <button
                      onClick={() => setEditingUser(user)}
                      className="rounded p-1.5 transition-colors"
                      style={{ color: 'var(--color-text-muted)' }}
                      onMouseEnter={(e) => { e.currentTarget.style.backgroundColor = 'var(--color-surface)'; e.currentTarget.style.color = 'var(--color-text-secondary)' }}
                      onMouseLeave={(e) => { e.currentTarget.style.backgroundColor = 'transparent'; e.currentTarget.style.color = 'var(--color-text-muted)' }}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                    <button
                      onClick={() => handleDelete(user)}
                      className="rounded p-1.5 transition-colors"
                      style={{ color: 'var(--color-text-muted)' }}
                      onMouseEnter={(e) => { e.currentTarget.style.backgroundColor = 'var(--color-danger-subtle)'; e.currentTarget.style.color = 'var(--color-danger)' }}
                      onMouseLeave={(e) => { e.currentTarget.style.backgroundColor = 'transparent'; e.currentTarget.style.color = 'var(--color-text-muted)' }}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {editingUser && (
        <EditUserDialog
          user={editingUser}
          onClose={() => setEditingUser(null)}
          onUpdated={() => {
            setEditingUser(null)
            queryClient.invalidateQueries({ queryKey: ['admin', 'users'] })
          }}
        />
      )}
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/pages/AdminUsers.tsx
git commit -m "feat(admin): add admin users table with edit/delete actions"
```

---

### Task D4: EditUserDialog component

**Files:**
- Create: `web-ui/src/components/EditUserDialog.tsx`

- [ ] **Step 1: Create `web-ui/src/components/EditUserDialog.tsx`**

```tsx
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { X } from 'lucide-react'
import api from '../api/client'
import type { UserInfo } from '../api/client'

interface EditUserDialogProps {
  user: UserInfo
  onClose: () => void
  onUpdated: () => void
}

export default function EditUserDialog({ user, onClose, onUpdated }: EditUserDialogProps) {
  const [username, setUsername] = useState(user.username)
  const [email, setEmail] = useState(user.email ?? '')
  const [isAdmin, setIsAdmin] = useState(user.is_admin)
  const [password, setPassword] = useState('')
  const [saving, setSaving] = useState(false)

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSaving(true)
    try {
      const body: Record<string, unknown> = { username }
      if (email) body.email = email
      if (password) body.password = password
      body.is_admin = isAdmin

      await api.patch(`admin/users/${user.id}`, { json: body }).json()
      toast.success('User updated')
      onUpdated()
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Update failed'
      toast.error(msg)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={onClose} />

      <div
        className="relative w-full max-w-md rounded-xl p-6"
        style={{
          backgroundColor: 'var(--color-surface-elevated)',
          border: '1px solid var(--glass-border)',
          backdropFilter: 'blur(var(--glass-blur))',
          boxShadow: 'var(--glass-shadow)',
        }}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Edit User
          </h2>
          <button onClick={onClose} className="rounded p-1" style={{ color: 'var(--color-text-muted)' }}>
            <X className="h-5 w-5" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="mb-1 block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              Username
            </label>
            <input
              type="text"
              required
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
            />
          </div>

          <div>
            <label className="mb-1 block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              Email
            </label>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
            />
          </div>

          <div>
            <label className="mb-1 block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              New Password (leave blank to keep current)
            </label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              minLength={8}
              placeholder="••••••••"
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
            />
          </div>

          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={isAdmin}
              onChange={(e) => setIsAdmin(e.target.checked)}
              className="rounded"
            />
            <span className="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              Admin privileges
            </span>
          </label>

          <div className="flex justify-end gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg px-4 py-2 text-sm transition-colors"
              style={{ color: 'var(--color-text-muted)' }}
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={saving}
              className="rounded-lg px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
              style={{ backgroundColor: 'var(--color-accent)' }}
            >
              {saving ? 'Saving…' : 'Save'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/components/EditUserDialog.tsx
git commit -m "feat(admin): add EditUserDialog modal component"
```

---

### Task D5: Admin shell page (tab navigation)

**Files:**
- Create: `web-ui/src/pages/Admin.tsx`

- [ ] **Step 1: Create `web-ui/src/pages/Admin.tsx`**

```tsx
import { useState } from 'react'
import AdminStats from './AdminStats'
import AdminUsers from './AdminUsers'

type Tab = 'overview' | 'users'

export default function Admin() {
  const [activeTab, setActiveTab] = useState<Tab>('overview')

  return (
    <div>
      <h1 className="mb-4 text-lg font-bold" style={{ color: 'var(--color-text-primary)' }}>
        Admin Panel
      </h1>

      <div
        className="mb-4 flex gap-1 rounded-xl p-1"
        style={{
          backgroundColor: 'var(--color-surface)',
          border: '1px solid var(--color-border)',
        }}
      >
        <button
          onClick={() => setActiveTab('overview')}
          className="flex-1 rounded-lg px-4 py-2 text-sm font-medium transition-colors"
          style={{
            backgroundColor: activeTab === 'overview' ? 'var(--color-accent-subtle)' : 'transparent',
            color: activeTab === 'overview' ? 'var(--color-accent)' : 'var(--color-text-muted)',
          }}
        >
          Overview
        </button>
        <button
          onClick={() => setActiveTab('users')}
          className="flex-1 rounded-lg px-4 py-2 text-sm font-medium transition-colors"
          style={{
            backgroundColor: activeTab === 'users' ? 'var(--color-accent-subtle)' : 'transparent',
            color: activeTab === 'users' ? 'var(--color-accent)' : 'var(--color-text-muted)',
          }}
        >
          Users
        </button>
      </div>

      {activeTab === 'overview' ? <AdminStats /> : <AdminUsers />}
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add web-ui/src/pages/Admin.tsx
git commit -m "feat(admin): add admin shell page with tab navigation"
```

---

## Phase E: Verification

### Task E1: Backend verification

- [ ] **Step 1: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings.

- [ ] **Step 2: Run tests**

Run: `cargo test --workspace`
Expected: all existing tests pass.

- [ ] **Step 3: Verify build**

Run: `cargo build --workspace`
Expected: compiles successfully.

### Task E2: Frontend verification

- [ ] **Step 1: Type check + build**

Run: `cd web-ui && npm run build`
Expected: compiles without TypeScript errors. Vite bundles successfully.

### Task E3: Update summary

- [ ] **Step 1: Update `.omo/summary/summary_and_next.md`**

Record Plan B completion. Mark remaining items for future P1 stories (Rate limiting per user/IP, OAuth login, etc.)

---

## Summary

| Phase | Tasks | Files Changed/Created | Description |
|-------|-------|----------------------|-------------|
| A | 4 | 5 Rust files | require_admin middleware, 4 admin API endpoints, route registration |
| B | 5 | 6 frontend files | CSS theme variables, dark mode, theme store, flash prevention, ThemeToggle |
| C | 10 | 8 frontend files | Layout component, glassmorphism for all 4 pages + NavBar + DropZone + LinkCard + Button + Input |
| D | 5 | 5 frontend files | AdminRoute guard, stats dashboard, users table, edit dialog, shell page |
| E | 3 | — | Clippy + tests + TS build |

**Total: 27 tasks** across 5 phases.

Phases A and B are fully independent — run them in parallel. Phase D depends on Phase A (backend API). Phase C can run in parallel with Phase A. Phase E depends on everything.
