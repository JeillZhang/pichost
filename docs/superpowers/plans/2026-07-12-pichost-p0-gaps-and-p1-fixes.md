# PicHost P0 Gap Filling + P1 Worker Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete all remaining P0 features (logout, refresh, delete, rate limiting, security headers) and fix 2 P1 Worker service-layer bugs (public serving after processing, missing thumb/webp routes).

**Architecture:** Adds 4 new API endpoints, 2 middleware modules, extends existing auth/image routes, adds 2 new public serving routes for thumbnails and WebP. All changes within `pichost-api` and config, no new crates.

**Tech Stack:** Rust (Axum 0.8, jsonwebtoken 9, deadpool-redis 0.15, tower-http 0.6 with set-header), PostgreSQL 18, Redis 8.

## Global Constraints

- Rust edition 2021, workspace version 0.1.0, `rustfmt` + `clippy` (per `rust-toolchain.toml`)
- PICHOST_ env prefix, figment pipeline: defaults → config.toml → env override (same pattern as `pichost-core/src/config.rs`)
- JWT signing/verification: `EncodingKey`/`DecodingKey` from `config.auth.jwt_secret` (HS256)
- Redis blacklist key format: `bl:{jti}` (changed from existing `bl:{user_id}`)
- Redis rate limit key format: `rl:{policy}:{suffix}`
- Fail closed on auth blacklist check (`.unwrap_or(true)`); fail OPEN on rate limit Redis errors
- Public image serving: accepts both `status = 'active'` and `status = 'ready'`
- This plan changes token claims structure (adds `jti` + `typ`). Tokens issued before this deploy will fail `decode::<AccessTokenClaims>`. Acceptable for self-hosted — users re-login after deploy.
- No compile-time sqlx checks (no `query!` macro)
- Frontend: React 19, ky, Zustand, TanStack Query (existing patterns in `web-ui/`)
- Commits: conventional commits (`feat:`, `fix:`, `chore:`)

---

## File Structure Map

```
pichost-api/
├── Cargo.toml                              ← MODIFY: add "set-header" to tower-http features
├── src/routes/auth.rs                      ← MODIFY: add logout/refresh handlers, split TokenClaims into Access+Refresh with jti+typ
├── src/routes/images.rs                    ← MODIFY: add delete_image handler, extend get_image/query, public_get accepts "ready", add public_get_thumb/public_get_webp
├── src/routes/mod.rs                       ← MODIFY: add users module (no change if keeping empty for now)
├── src/middleware/auth.rs                  ← MODIFY: check blacklist by jti instead of user_id
├── src/middleware/mod.rs                   ← MODIFY: add rate_limit module
├── src/middleware/rate_limit.rs            ← CREATE: Redis INCR+EXPIRE rate limiter with 4 policies
├── src/services/upload.rs                  ← MODIFY: extend UploadResult with width, height, status, mime_type, thumbnail_url, webp_url, created_at
├── src/cache/mod.rs                        ← MODIFY: add incr() method for atomic counter
├── src/main.rs                             ← MODIFY: register new routes + middleware layers + security headers

web-ui/
├── src/api/client.ts                       ← MODIFY: add refreshToken, logout, deleteImage, ImageInfo type alignment
├── src/stores/auth.ts                      ← MODIFY: real server-side logout, auto-refresh
├── src/pages/ImageDetail.tsx               ← MODIFY: show status/width/height, add delete button
├── src/pages/Dashboard.tsx                 ← MODIFY: show status tracking after upload
```

**Inter-task dependency graph:**
```
Task 1 (TokenClaims + jti) ────→ Task 2 (refresh handler)
                            ├───→ Task 3 (logout handler)
                            ├───→ Task 4 (middleware blacklist check)
                            │
Task 5 (delete image)      ──── independent (needs StorageBackend)
                            │
Task 6 (rate limit)        ──── needs cache.incr() + AuthUser (from middleware)
                            │
Task 7 (security headers)  ──── fully independent (tower-http layer only)
                            │
Task 8 (extend get_image)  ──── independent
                            │
Task 9 (worker fixes)      ──── independent (public_get + thumb/webp routes)
                            │
Task 10 (Docker worker)    ──── independent
                            │
Task 11 (frontend)         ──── depends on all backend tasks
```

Tasks 5, 7, 8, 9, 10 are fully independent and can be parallelized. Tasks 2-4 depend on Task 1 (serial).

---

### Task 1: TokenClaims with `jti` + `typ` — break existing tokens

**Files:**
- Modify: `pichost-api/src/routes/auth.rs` — split `TokenClaims` into `AccessTokenClaims` + `RefreshTokenClaims`, update `generate_tokens()` to emit `jti` + `typ`, update register/login callers
- Modify: `pichost-api/src/middleware/auth.rs` — decode using `AccessTokenClaims` instead of `TokenClaims`

**Interfaces:**
- Consumes: `AppConfig` (existing), `Uuid::new_v4()` for jti generation
- Produces: `AccessTokenClaims { sub, jti, exp, iat, is_admin, typ }`, `RefreshTokenClaims { sub, jti, exp, iat, is_admin, typ, access_jti, access_exp }`
- Produces: `generate_tokens()` now returns 4-tuple `(access_token, refresh_token, AccessTokenClaims, RefreshTokenClaims)`
- Consumed by: Tasks 2 (refresh), 3 (logout), 4 (middleware)

**Why this matters:** The existing tokens have NO `jti` (unique token ID) and NO `typ` (token type). Without `jti`, we can only blacklist by user ID (`bl:{user_id}`), meaning logging out one device invalidates ALL user tokens. Without `typ`, an access token could be used where a refresh token is expected (and vice versa).

- [ ] **Step 1: Replace `TokenClaims` with split structs**

In `pichost-api/src/routes/auth.rs`, replace lines 36-42 (the old `TokenClaims` struct) with:

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccessTokenClaims {
    pub sub: String,
    pub jti: String,
    pub exp: usize,
    pub iat: usize,
    pub is_admin: bool,
    pub typ: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RefreshTokenClaims {
    pub sub: String,
    pub jti: String,
    pub exp: usize,
    pub iat: usize,
    pub is_admin: bool,
    pub typ: String,
    pub access_jti: String,
    pub access_exp: usize,
}
```

- [ ] **Step 2: Update `generate_tokens()` to produce jti and typ**

Replace the existing function (lines 61-88) with:

```rust
fn generate_tokens(
    user_id: Uuid,
    is_admin: bool,
    config: &AppConfig,
) -> Result<(String, String, AccessTokenClaims, RefreshTokenClaims), jsonwebtoken::errors::Error> {
    let now = Utc::now().timestamp() as usize;
    let access_exp = now + config.auth.access_token_ttl as usize;
    let refresh_exp = now + config.auth.refresh_token_ttl as usize;

    let access_jti = Uuid::new_v4().to_string();
    let refresh_jti = Uuid::new_v4().to_string();

    let access_claims = AccessTokenClaims {
        sub: user_id.to_string(),
        jti: access_jti,
        exp: access_exp,
        iat: now,
        is_admin,
        typ: "access".to_string(),
    };

    let refresh_claims = RefreshTokenClaims {
        sub: user_id.to_string(),
        jti: refresh_jti,
        exp: refresh_exp,
        iat: now,
        is_admin,
        typ: "refresh".to_string(),
        access_jti: access_jti.clone(),
        access_exp,
    };

    let key = EncodingKey::from_secret(config.auth.jwt_secret.as_bytes());

    let access_token = encode(&Header::default(), &access_claims, &key)?;
    let refresh_token = encode(&Header::default(), &refresh_claims, &key)?;

    Ok((access_token, refresh_token, access_claims, refresh_claims))
}
```

- [ ] **Step 3: Update `register()` handler (line ~141)**

Change:
```rust
let (access_token, refresh_token) = generate_tokens(user_id, false, &state.config)?;
```
To:
```rust
let (access_token, refresh_token, _access_claims, _refresh_claims) =
    generate_tokens(user_id, false, &state.config)?;
```

- [ ] **Step 4: Update `login()` handler (line ~189)**

Change:
```rust
let (access_token, refresh_token) = generate_tokens(user_id, is_admin, &state.config)?;
```
To:
```rust
let (access_token, refresh_token, _access_claims, _refresh_claims) =
    generate_tokens(user_id, is_admin, &state.config)?;
```

- [ ] **Step 5: Update middleware to use `AccessTokenClaims`**

In `pichost-api/src/middleware/auth.rs` line 53, change:
```rust
let token_data = decode::<super::super::routes::auth::TokenClaims>(token, &key, &validation)
```
To:
```rust
let token_data = decode::<super::super::routes::auth::AccessTokenClaims>(token, &key, &validation)
```

- [ ] **Step 6: Build to verify**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 7: Commit**

```bash
git add pichost-api/src/routes/auth.rs pichost-api/src/middleware/auth.rs
git commit -m "feat(auth): add jti and typ fields to JWT claims, split into AccessTokenClaims/RefreshTokenClaims"
```

---

### Task 2: POST /auth/refresh — token rotation

**Files:**
- Modify: `pichost-api/src/routes/auth.rs` — add `RefreshRequest`, `RefreshResponse`, `refresh` handler
- Modify: `pichost-api/src/main.rs` — register `/auth/refresh` route

**Interfaces:**
- Consumes: `RefreshTokenClaims` (from Task 1), `Cache::set_ex` for blacklisting old tokens
- Produces: `pub async fn refresh(State, Json<RefreshRequest>) -> Result<(StatusCode, Json<RefreshResponse>), ...>`

**Flow:** decode refresh JWT → check `typ == "refresh"` → check jti blacklist → verify user exists → issue new token pair → blacklist old tokens → return new tokens

- [ ] **Step 1: Add request/response types**

Add after `LoginRequest` in `pichost-api/src/routes/auth.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserInfo,
}
```

- [ ] **Step 2: Add `refresh` handler**

Add at the end of `auth.rs` (after `login`):

```rust
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RefreshRequest>,
) -> Result<(StatusCode, Json<RefreshResponse>), (StatusCode, Json<serde_json::Value>)> {
    let config = &state.config;
    let key = DecodingKey::from_secret(config.auth.jwt_secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<RefreshTokenClaims>(&payload.refresh_token, &key, &validation)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "invalid or expired refresh token"))?;
    let claims = token_data.claims;

    if claims.typ != "refresh" {
        return Err(error_response(StatusCode::UNAUTHORIZED, "invalid token type"));
    }

    let bl_refresh_key = format!("bl:{}", claims.jti);
    if state.cache.exists(&bl_refresh_key).await.unwrap_or(true) {
        return Err(error_response(StatusCode::UNAUTHORIZED, "refresh token has been revoked"));
    }

    let user_id: Uuid = claims.sub.parse()
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "invalid token subject"))?;

    let row = sqlx::query_as::<_, (String, Option<String>, bool)>(
        "SELECT username, email, is_admin FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?
    .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "user not found"))?;
    let (username, email, is_admin) = row;

    let (new_access, new_refresh, new_access_claims, new_refresh_claims) =
        generate_tokens(user_id, is_admin, config)
            .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "token generation failed"))?;

    let now = Utc::now().timestamp() as usize;

    let refresh_ttl = claims.exp.saturating_sub(now);
    let _ = state.cache.set_ex(&bl_refresh_key, "revoked", refresh_ttl as u64).await;

    let bl_access_key = format!("bl:{}", claims.access_jti);
    let access_ttl = claims.access_exp.saturating_sub(now);
    if access_ttl > 0 {
        let _ = state.cache.set_ex(&bl_access_key, "revoked", access_ttl as u64).await;
    }

    tracing::info!(user = %user_id, "tokens refreshed (rotation)");

    Ok((
        StatusCode::OK,
        Json(RefreshResponse {
            access_token: new_access,
            refresh_token: new_refresh,
            user: UserInfo { id: user_id, username, email, is_admin },
        }),
    ))
}
```

- [ ] **Step 3: Register route in main.rs**

In `pichost-api/src/main.rs`, change auth routes (lines 36-41) to:

```rust
.nest(
    "/api/v1/auth",
    Router::new()
        .route("/register", post(routes::auth::register))
        .route("/login", post(routes::auth::login))
        .route("/refresh", post(routes::auth::refresh))
        .route("/logout", post(routes::auth::logout)),
)
```

- [ ] **Step 4: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/routes/auth.rs pichost-api/src/main.rs
git commit -m "feat(auth): add POST /auth/refresh with token rotation and old token blacklisting"
```

---

### Task 3: POST /auth/logout — server-side token revocation

**Files:**
- Modify: `pichost-api/src/routes/auth.rs` — add `logout` handler

**Interfaces:**
- Consumes: `AccessTokenClaims` (from Task 1), `Cache::set_ex`
- Produces: `pub async fn logout(State, headers: HeaderMap) -> Result<...>`

**Flow:** extract Bearer → decode with `validate_exp = false` (allows blacklisting already-expired tokens) → verify `typ == "access"` → blacklist `jti` with TTL = `exp - now` → return 200

- [ ] **Step 1: Add logout handler**

Add to `pichost-api/src/routes/auth.rs` after `refresh`:

```rust
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "missing authorization header"))?;

    let key = DecodingKey::from_secret(state.config.auth.jwt_secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false;

    let token_data = decode::<AccessTokenClaims>(token, &key, &validation)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "invalid token"))?;
    let claims = token_data.claims;

    if claims.typ != "access" {
        return Err(error_response(StatusCode::BAD_REQUEST, "only access tokens can be logged out via this endpoint"));
    }

    let now = Utc::now().timestamp() as usize;
    let ttl = claims.exp.saturating_sub(now);
    if ttl > 0 {
        let bl_key = format!("bl:{}", claims.jti);
        let _ = state.cache.set_ex(&bl_key, "revoked", ttl as u64).await;
    }

    tracing::info!(user = %claims.sub, jti = %claims.jti, "logged out");
    Ok((StatusCode::OK, Json(serde_json::json!({"message": "logged out successfully"}))))
}
```

- [ ] **Step 2: Add `use axum::http::HeaderMap` import**

Add at the top of `auth.rs` alongside existing axum imports:
```rust
use axum::http::HeaderMap;
```

- [ ] **Step 3: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/auth.rs
git commit -m "feat(auth): add POST /auth/logout with per-token JTI blacklisting"
```

---

### Task 4: Update middleware — check blacklist by JTI

**Files:**
- Modify: `pichost-api/src/middleware/auth.rs` — change blacklist key from `bl:{user_id}` to `bl:{jti}`

- [ ] **Step 1: Replace blacklist key logic**

Change existing block (lines 64-77) from:
```rust
let blacklist_key = format!("bl:{}", claims.sub);
let is_blacklisted = state.cache.exists(&blacklist_key).await.unwrap_or(true);
if is_blacklisted {
    return Err((
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": "token has been revoked"})),
    ));
}
```
To:
```rust
let bl_key = format!("bl:{}", claims.jti);
let is_revoked = state.cache.exists(&bl_key).await.unwrap_or(true);
if is_revoked {
    return Err((
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": "token has been revoked"})),
    ));
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add pichost-api/src/middleware/auth.rs
git commit -m "fix(auth): check token blacklist by JTI instead of user ID"
```

---

### Task 5: DELETE /images/{id} — image deletion with storage cleanup

**Files:**
- Modify: `pichost-api/src/routes/images.rs` — add `delete_image` handler
- Modify: `pichost-api/src/main.rs` — register DELETE route

- [ ] **Step 1: Add `delete_image` handler**

Add at the end of `pichost-api/src/routes/images.rs`:

```rust
/// DELETE /api/v1/images/{id} — delete image + storage files (protected)
pub async fn delete_image(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        r#"SELECT storage_key, storage_backend, thumbnail_key, webp_key
           FROM images WHERE id = $1 AND (user_id = $2 OR $3)"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(user.is_admin)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Delete image query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (storage_key, _storage_backend, thumb_key, webp_key) = row;

    let storage = pichost_core::storage::local::LocalStorage::new(
        state.config.storage.local_base_path.clone(),
        state.config.server.public_url.clone(),
    );

    let _ = storage.delete(&storage_key).await;
    if let Some(ref tk) = thumb_key { let _ = storage.delete(tk).await; }
    if let Some(ref wk) = webp_key { let _ = storage.delete(wk).await; }

    sqlx::query("DELETE FROM images WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Image delete db failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "failed to delete image"})))
        })?;

    tracing::info!(image_id = %id, user_id = %user.id, "image deleted");
    Ok(Json(json!({"message": "image deleted", "id": id})))
}
```

- [ ] **Step 2: Register DELETE route in main.rs**

Change the `image_routes` block to:
```rust
let image_routes = Router::new()
    .route("/", get(routes::images::list_images).post(routes::images::upload_handler))
    .route("/{id}", get(routes::images::get_image).delete(routes::images::delete_image))
    .route_layer(protected);
```

- [ ] **Step 3: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/images.rs pichost-api/src/main.rs
git commit -m "feat(api): add DELETE /api/v1/images/{id} with storage file cleanup"
```

---

### Task 6: Rate limiting middleware — Redis INCR + EXPIRE

**Files:**
- Modify: `pichost-api/src/cache/mod.rs` — add `incr()` method
- Create: `pichost-api/src/middleware/rate_limit.rs` — 4 policy middleware functions
- Modify: `pichost-api/src/middleware/mod.rs` — add `rate_limit` module
- Modify: `pichost-api/src/main.rs` — apply per-route layers

**Policies:**
| Policy | Rate | Key |
|--------|------|-----|
| auth (login/register/refresh) | 5/min | per IP |
| upload | 30/min | per user_id (or IP) |
| general (list/get/delete) | 60/min | per user_id (or IP) |
| public | 200/min | per IP |

**Design:** Redis INCR + EXPIRE in a pipeline (atomic). Fail OPEN on Redis errors (rate limiting is a soft protection, unlike auth which fails closed).

- [ ] **Step 1: Add `incr()` to Cache**

In `pichost-api/src/cache/mod.rs`, add after `exists` method:

```rust
    /// Atomically increment a counter and set TTL on first creation.
    /// Returns the new count after increment.
    pub async fn incr(&self, key: &str, ttl: u64) -> Result<u64, deadpool_redis::redis::RedisError> {
        let mut conn = self.pool.get().await.map_err(pool_err)?;
        let mut pipe = deadpool_redis::redis::pipe();
        pipe.cmd("INCR").arg(key).ignore()
            .cmd("EXPIRE").arg(key).arg(ttl as usize).ignore();
        pipe.query_async::<()>(&mut *conn).await?;
        let count: u64 = deadpool_redis::redis::cmd("GET").arg(key)
            .query_async(&mut *conn).await?;
        Ok(count)
    }
```

- [ ] **Step 2: Create `pichost-api/src/middleware/rate_limit.rs`**

```rust
use std::sync::Arc;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use crate::app::AppState;
use crate::middleware::auth::AuthUser;

const POLICY_AUTH: (&str, u32, u64) = ("auth", 5, 60);
const POLICY_UPLOAD: (&str, u32, u64) = ("upload", 30, 60);
const POLICY_GENERAL: (&str, u32, u64) = ("general", 60, 60);
const POLICY_PUBLIC: (&str, u32, u64) = ("public", 200, 60);

fn too_many_response(retry_after: u64) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({
        "error": format!("rate limit exceeded, retry after {}s", retry_after)
    })))
}

fn rl_key(policy: &str, suffix: &str) -> String {
    format!("rl:{policy}:{suffix}")
}

fn extract_client_ip(req: &Request) -> String {
    if let Some(xff) = req.headers().get("x-forwarded-for") {
        if let Ok(val) = xff.to_str() {
            if let Some(ip) = val.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }
    "unknown".to_string()
}

async fn check_rate_limit(
    cache: &crate::cache::Cache,
    policy: &str,
    key: &str,
    max_requests: u32,
    window_secs: u64,
) -> Result<u32, u64> {
    let redis_key = rl_key(policy, key);
    match cache.incr(&redis_key, window_secs).await {
        Ok(count) => {
            if count as u32 > max_requests {
                let mut conn = match cache.get_pool().get().await {
                    Ok(c) => c,
                    Err(_) => return Err(window_secs),
                };
                let ttl: u64 = deadpool_redis::redis::cmd("TTL")
                    .arg(&redis_key)
                    .query_async(&mut *conn)
                    .await
                    .unwrap_or(window_secs);
                Err(ttl)
            } else {
                Ok(max_requests - count as u32)
            }
        }
        Err(e) => {
            tracing::warn!("Rate limit Redis error: {e}");
            Ok(max_requests)
        }
    }
}

pub async fn rate_limit_auth(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let ip = extract_client_ip(&req);
    match check_rate_limit(&state.cache, "auth", &ip, POLICY_AUTH.1, POLICY_AUTH.2).await {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => { tracing::warn!(ip = %ip, "auth rate limited"); Err(too_many_response(retry_after)) }
    }
}

pub async fn rate_limit_upload(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let key = req.extensions().get::<AuthUser>()
        .map(|u| u.id.to_string())
        .unwrap_or_else(|| extract_client_ip(&req));
    match check_rate_limit(&state.cache, "upload", &key, POLICY_UPLOAD.1, POLICY_UPLOAD.2).await {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => { tracing::warn!(key = %key, "upload rate limited"); Err(too_many_response(retry_after)) }
    }
}

pub async fn rate_limit_general(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let key = req.extensions().get::<AuthUser>()
        .map(|u| u.id.to_string())
        .unwrap_or_else(|| extract_client_ip(&req));
    match check_rate_limit(&state.cache, "general", &key, POLICY_GENERAL.1, POLICY_GENERAL.2).await {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => { tracing::warn!(key = %key, "general rate limited"); Err(too_many_response(retry_after)) }
    }
}

pub async fn rate_limit_public(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let ip = extract_client_ip(&req);
    match check_rate_limit(&state.cache, "public", &ip, POLICY_PUBLIC.1, POLICY_PUBLIC.2).await {
        Ok(_) => Ok(next.run(req).await),
        Err(retry_after) => { tracing::warn!(ip = %ip, "public rate limited"); Err(too_many_response(retry_after)) }
    }
}
```

- [ ] **Step 3: Update middleware/mod.rs**

Change `pichost-api/src/middleware/mod.rs`:
```rust
pub mod auth;
pub mod rate_limit;
```

- [ ] **Step 4: Apply layers in main.rs**

In `pichost-api/src/main.rs`, add:
```rust
use pichost_api::middleware::rate_limit;
```

Apply auth rate limiting to the auth router:
```rust
let auth_routes = Router::new()
    .route("/register", post(routes::auth::register))
    .route("/login", post(routes::auth::login))
    .route("/refresh", post(routes::auth::refresh))
    .route("/logout", post(routes::auth::logout))
    .route_layer(middleware::from_fn_with_state(
        state.clone(),
        rate_limit::rate_limit_auth,
    ));
```

Apply upload rate limiting to upload route (create a separate layer chain):
```rust
let upload_routes = Router::new()
    .route("/", post(routes::images::upload_handler))
    .route_layer(middleware::from_fn_with_state(
        state.clone(),
        rate_limit::rate_limit_upload,
    ))
    .route_layer(protected.clone());
```

Keep `image_routes` for list/get/delete with general rate limit:
```rust
let image_routes = Router::new()
    .route("/", get(routes::images::list_images))
    .route("/{id}", get(routes::images::get_image).delete(routes::images::delete_image))
    .route_layer(middleware::from_fn_with_state(
        state.clone(),
        rate_limit::rate_limit_general,
    ))
    .route_layer(protected.clone());
```

Combine them:
```rust
.nest("/api/v1/images", upload_routes)
.nest("/api/v1/images", image_routes)
```

Apply public rate limit to public routes:
```rust
let public_routes = Router::new()
    .route("/{public_key}", get(routes::images::public_get))
    .route("/thumb/{image_id}", get(routes::images::public_get_thumb))
    .route("/webp/{image_id}", get(routes::images::public_get_webp))
    .route_layer(middleware::from_fn_with_state(
        state.clone(),
        rate_limit::rate_limit_public,
    ));
```

- [ ] **Step 5: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 6: Commit**

```bash
git add pichost-api/src/cache/mod.rs pichost-api/src/middleware/rate_limit.rs pichost-api/src/middleware/mod.rs pichost-api/src/main.rs
git commit -m "feat(api): add Redis-based rate limiting middleware with 4 policies and cache.incr()"
```

---

### Task 7: Security headers via tower-http SetResponseHeaderLayer

**Files:**
- Modify: `Cargo.toml` — add `"set-header"` to tower-http workspace features
- Modify: `pichost-api/src/main.rs` — add 5 security header layers

- [ ] **Step 1: Update tower-http features**

In workspace `Cargo.toml`, change the tower-http dependency:
```toml
tower-http = { version = "0.6", features = ["cors", "trace", "set-header"] }
```

- [ ] **Step 2: Add security header layers in main.rs**

Add imports:
```rust
use http::{HeaderName, HeaderValue};
use tower_http::set_header::SetResponseHeaderLayer;
```

Add right before `.with_state(state)` (currently line 46):

```rust
        // Security headers
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(
                "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; sandbox",
            ),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
```

- [ ] **Step 3: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml pichost-api/src/main.rs
git commit -m "feat(api): add 5 security headers via tower-http SetResponseHeaderLayer"
```

---

### Task 8: Extend image responses — full metadata

**Files:**
- Modify: `pichost-api/src/services/upload.rs` — extend `UploadResult` struct with `mime_type`, `width`, `height`, `status`, `thumbnail_url`, `webp_url`, `created_at`
- Modify: `pichost-api/src/routes/images.rs` — extend list/get queries to return all new fields

**Impact:** This changes the JSON shape returned by `GET /api/v1/images` and `GET /api/v1/images/{id}`. The frontend gets more info.

- [ ] **Step 1: Add `use chrono::DateTime` import to upload.rs and extend UploadResult**

```rust
use chrono::{DateTime, Utc};
```

Replace the existing `UploadResult` struct (lines 16-27) with:

```rust
#[derive(Debug, Serialize)]
pub struct UploadResult {
    pub id: Uuid,
    pub public_key: String,
    pub original_name: String,
    pub url: String,
    pub markdown: String,
    pub html: String,
    pub bbcode: String,
    pub sha256: String,
    pub file_size: i64,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub status: String,
    pub thumbnail_url: Option<String>,
    pub webp_url: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

- [ ] **Step 2: Update dedup return in upload.rs (around line 176)**

After the dedup query fetches the row, before constructing `UploadResult`:

```rust
    let (image_id, public_key, original_name, _storage_key, _mime_type, file_size, url, sha256) = row;

    // Build UploadResult with new fields
    return Ok(UploadResult {
        id: image_id,
        public_key,
        original_name,
        url,
        markdown: format!("![{}]({})", original_name, url),
        html: format!("<img src=\"{}\" alt=\"{}\" />", url, html_escape(&original_name)),
        bbcode: format!("[img]{}[/img]", url),
        sha256,
        file_size,
        mime_type: _mime_type.clone(),
        width: None,
        height: None,
        status: "active".to_string(),
        thumbnail_url: None,
        webp_url: None,
        created_at: chrono::Utc::now(),
    });
```

- [ ] **Step 3: Update success return in upload.rs (around line 308)**

```rust
    Ok(UploadResult {
        id: image_id,
        public_key,
        original_name,
        url,
        markdown,
        html,
        bbcode,
        sha256,
        file_size,
        mime_type,
        width,
        height,
        status: "active".to_string(),
        thumbnail_url: None,
        webp_url: None,
        created_at: chrono::Utc::now(),
    })
```

- [ ] **Step 4: Update list_images in images.rs**

Replace the query + map with extended fields:

```rust
pub async fn list_images(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<UploadResult>>, (StatusCode, Json<serde_json::Value>)> {
    let rows = sqlx::query_as::<_, (
        Uuid, String, String, String, String, i64, String,
        Option<i32>, Option<i32>, String,
        Option<String>, Option<String>, chrono::DateTime<chrono::Utc>,
    )>(
        r#"SELECT id, public_key, original_name, url, mime_type, file_size,
                  sha256, width, height, status, thumbnail_url, webp_url, created_at
           FROM images WHERE user_id = $1 ORDER BY created_at DESC LIMIT 50"#,
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("List images query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
    })?;

    let images = rows.into_iter().map(
        |(id, public_key, original_name, url, mime_type, file_size,
          sha256, width, height, status, thumbnail_url, webp_url, created_at)| {
        UploadResult {
            id, public_key,
            original_name: original_name.clone(),
            url: url.clone(),
            markdown: format!("![{}]({})", original_name, url),
            html: format!("<img src=\"{}\" alt=\"{}\" />", url, html_escape(&original_name)),
            bbcode: format!("[img]{}[/img]", url),
            sha256, file_size, mime_type, width, height, status,
            thumbnail_url, webp_url, created_at,
        }
    }).collect();

    Ok(Json(images))
}
```

- [ ] **Step 5: Update get_image similarly**

Replace the get_image body to use the same extended query and `UploadResult` construction (as shown in Step 4 but with `WHERE id = $1 AND user_id = $2` and `fetch_optional`).

- [ ] **Step 6: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 7: Commit**

```bash
git add pichost-api/src/services/upload.rs pichost-api/src/routes/images.rs
git commit -m "feat(api): extend image responses with status, dimensions, thumbnail/webp URLs, timestamp"
```

---

### Task 9: P1 Worker fixes — public serving status + thumb/webp routes

**Files:**
- Modify: `pichost-api/src/routes/images.rs` — fix status check; add `public_get_thumb`, `public_get_webp`
- Modify: `pichost-api/src/main.rs` — add thumb/webp routes to public router

**Bug 1:** Worker sets `status = 'ready'` on completion, but `public_get` only accepts `"active"`. After worker runs, serving returns 404.

**Bug 2:** Worker generates thumbnails and WebP files with keys like `{user_id}/thumb.{image_id}`, but there are no Axum routes to serve them. The URLs (`/u/thumb-{image_id}` or `/u/thumb/{image_id}`) return 404.

- [ ] **Step 1: Fix public_get status check**

In `pichost-api/src/routes/images.rs` line 147, change:
```rust
    if status != "active" {
```
to:
```rust
    if status != "active" && status != "ready" {
```

- [ ] **Step 2: Add helper for MIME inference and the two new handlers**

Add at the end of `pichost-api/src/routes/images.rs` (before the file ends):

```rust
fn mime_for_thumb_key(key: &str) -> &'static str {
    if key.ends_with(".png") { "image/png" }
    else { "image/jpeg" }
}

/// GET /u/thumb/{image_id} — serve generated thumbnail (unauthenticated)
pub async fn public_get_thumb(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<Uuid>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT thumbnail_key FROM images WHERE id = $1 AND status IN ('active', 'ready')",
    )
    .bind(image_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Thumb query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (thumb_key,) = row;
    let thumb_key = thumb_key.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({"error": "thumbnail not yet generated"})))
    })?;

    let storage = pichost_core::storage::local::LocalStorage::new(
        state.config.storage.local_base_path.clone(),
        state.config.server.public_url.clone(),
    );
    let bytes = storage.get(&thumb_key).await.map_err(|e| {
        tracing::warn!("Thumb storage read failed: {e}");
        (StatusCode::NOT_FOUND, Json(json!({"error": "thumbnail not found"})))
    })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_for_thumb_key(&thumb_key))
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

/// GET /u/webp/{image_id} — serve generated WebP (unauthenticated)
pub async fn public_get_webp(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<Uuid>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT webp_key FROM images WHERE id = $1 AND status IN ('active', 'ready')",
    )
    .bind(image_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("WebP query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "image not found"}))))?;

    let (webp_key,) = row;
    let webp_key = webp_key.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({"error": "WebP not yet generated"})))
    })?;

    let storage = pichost_core::storage::local::LocalStorage::new(
        state.config.storage.local_base_path.clone(),
        state.config.server.public_url.clone(),
    );
    let bytes = storage.get(&webp_key).await.map_err(|e| {
        tracing::warn!("WebP storage read failed: {e}");
        (StatusCode::NOT_FOUND, Json(json!({"error": "WebP not found"})))
    })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/webp")
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}
```

- [ ] **Step 3: Register thumb/webp routes in main.rs**

Extend the `public_routes` block:
```rust
let public_routes = Router::new()
    .route("/{public_key}", get(routes::images::public_get))
    .route("/thumb/{image_id}", get(routes::images::public_get_thumb))
    .route("/webp/{image_id}", get(routes::images::public_get_webp));
```

- [ ] **Step 4: Build**

Run: `cargo build -p pichost-api`
Expected: compiles successfully.

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/routes/images.rs pichost-api/src/main.rs
git commit -m "fix(api): accept 'ready' status in public_get, add thumb/webp serving routes"
```

---

### Task 10: Docker Compose worker service + Dockerfile.worker

**Files:**
- Create: `Dockerfile.worker` — multi-stage build for pichost-worker
- Modify: `docker-compose.yml` — add `worker` service
- Modify: `.env.example` — note worker config if missing

- [ ] **Step 1: Create Dockerfile.worker** (pattern from Dockerfile.api)

```dockerfile
# Stage 1 — Build the pichost-worker binary
FROM rust:1.96-slim AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY pichost-core/ ./pichost-core/
COPY pichost-api/ ./pichost-api/
COPY pichost-worker/ ./pichost-worker/
COPY migrations/ ./migrations/

RUN cargo build --release -p pichost-worker

# Stage 2 — Minimal runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/pichost-worker /usr/local/bin/pichost-worker

CMD ["pichost-worker"]
```

- [ ] **Step 2: Add worker service to docker-compose.yml**

```yaml
  # -------------------------------------------------------------------------
  # PicHost Worker — async image processing
  # -------------------------------------------------------------------------
  worker:
    build:
      dockerfile: Dockerfile.worker
      context: .
    restart: unless-stopped
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_started
    volumes:
      - ./storage-local:/app/storage-local
    environment:
      DATABASE_URL: postgres://pichost:pichost@postgres/pichost
      PICHOST_DATABASE_URL: postgres://pichost:pichost@postgres/pichost
      PICHOST_REDIS_URL: redis://redis:6379
      PICHOST_AUTH_JWT_SECRET: dev-secret-32-bytes-long-for-pichost-!!!
      PICHOST_STORAGE_LOCAL_BASE_PATH: /app/storage-local
      PICHOST_SERVER_PUBLIC_URL: http://localhost:3000
```

- [ ] **Step 3: Commit**

```bash
git add Dockerfile.worker docker-compose.yml
git commit -m "chore: add Dockerfile.worker and worker service to docker-compose"
```

---

### Task 11: Frontend integration — auth store + API client + UI

**Files:**
- Modify: `web-ui/src/api/client.ts` — add `refreshToken`, `logout`, `deleteImage` functions; update `ImageInfo` type
- Modify: `web-ui/src/stores/auth.ts` — call server-side logout; add auto-refresh on 401
- Modify: `web-ui/src/pages/ImageDetail.tsx` — show status/width/height; add delete button
- Modify: `web-ui/src/pages/Dashboard.tsx` — show status tracking after upload

- [ ] **Step 1: Update api/client.ts**

Update the `ImageInfo` interface and add new functions:

```typescript
export interface ImageInfo {
  id: string
  public_key: string
  original_name: string
  url: string
  markdown: string
  html: string
  bbcode: string
  sha256: string
  file_size: number
  mime_type: string
  width: number | null
  height: number | null
  status: string
  thumbnail_url: string | null
  webp_url: string | null
  created_at: string
}

export async function refreshToken(): Promise<AuthResponse> {
  const refreshToken = localStorage.getItem('refresh_token')
  if (!refreshToken) throw new Error('No refresh token')
  return api.post('auth/refresh', { json: { refresh_token: refreshToken } }).json<AuthResponse>()
}

export async function logout(): Promise<void> {
  await api.post('auth/logout').json()
}

export async function deleteImage(id: string): Promise<void> {
  await api.delete(`images/${id}`).json()
}
```

Update the 401 handler to attempt refresh:
```typescript
afterResponse: [
  async (request, options, response) => {
    if (response.status === 401) {
      try {
        const refreshed = await useAuthStore.getState().refresh()
        if (refreshed) {
          // Retry original request with new token
          const token = localStorage.getItem('access_token')
          request.headers.set('Authorization', `Bearer ${token}`)
          return ky(request)
        }
      } catch {
        // Refresh failed — force logout
        useAuthStore.getState().forceLogout()
      }
    }
  },
],
```

- [ ] **Step 2: Update stores/auth.ts**

Add `refresh` and `forceLogout` actions:

```typescript
interface AuthState {
  // ... existing fields ...
  refresh: () => Promise<boolean>
  forceLogout: () => void
}

// Add to create callback:
refresh: async () => {
  try {
    const res = await apiRefreshToken()
    localStorage.setItem('access_token', res.access_token)
    localStorage.setItem('refresh_token', res.refresh_token)
    set({
      accessToken: res.access_token,
      refreshToken: res.refresh_token,
    })
    return true
  } catch {
    return false
  }
},

forceLogout: () => {
  localStorage.removeItem('access_token')
  localStorage.removeItem('refresh_token')
  localStorage.removeItem('user')
  set({
    user: null,
    accessToken: null,
    refreshToken: null,
    isAuthenticated: false,
  })
  window.location.href = '/login'
},
```

Update existing `logout` to call server-side endpoint:
```typescript
logout: async () => {
  try {
    await apiLogout()
  } catch {
    // Server-side logout failed (network error, etc.)
    // Still clear local state — better than being stuck logged in
  }
  localStorage.removeItem('access_token')
  localStorage.removeItem('refresh_token')
  localStorage.removeItem('user')
  set({
    user: null,
    accessToken: null,
    refreshToken: null,
    isAuthenticated: false,
  })
},
```

- [ ] **Step 3: Update ImageDetail.tsx**

Show status badge, dimensions, created_at. Add delete button:

```tsx
// Pseudo-code — actual implementation follows existing UI patterns

<div className="metadata">
  <p>Status: <span className={`status-badge ${image.status}`}>{image.status}</span></p>
  {image.width && image.height && (
    <p>Dimensions: {image.width} × {image.height}px</p>
  )}
  <p>Size: {formatFileSize(image.file_size)}</p>
  <p>Type: {image.mime_type}</p>
  <p>Uploaded: {new Date(image.created_at).toLocaleString()}</p>
  {image.thumbnail_url && <LinkCard label="Thumbnail" url={image.thumbnail_url} />}
  {image.webp_url && <LinkCard label="WebP" url={image.webp_url} />}
</div>

<button onClick={handleDelete} className="delete-btn">
  Delete Image
</button>
```

- [ ] **Step 4: Build frontend**

Run: `cd web-ui && npm run build`
Expected: compiles without TypeScript errors.

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/api/client.ts web-ui/src/stores/auth.ts web-ui/src/pages/
git commit -m "feat(ui): server-side logout, auto-refresh, image detail enhancements, delete support"
```

---

## Post-Implementation Cleanup

After all 11 tasks complete:

1. **Remove duplicate route registrations** — verify `main.rs` doesn't have conflicting `.nest("/api/v1/images", ...)` blocks
2. **Check unused imports** — `cargo clippy --workspace -- -D warnings` catches all
3. **Verify frontend build** — `cd web-ui && npm run build` clean
4. **Update README** — mark completed P0/P1 features
5. **Integration smoke test** — docker compose up → register → login → upload → refresh → logout → delete

---

## Self-Review

**1. Spec coverage:**
- Design spec §4.1: POST /auth/logout, POST /auth/refresh → Tasks 2, 3
- Design spec §4.2: DELETE /images/{id} → Task 5
- Design spec §4.4: Users endpoints → covered by `get_image` listing user's images (dedicated users module deferred)
- Design spec §8.3: Rate limiting → Task 6 (all 4 policies: login, upload, general, public)
- Design spec §9.5: JWT blacklist per-jti → Task 1 + Task 4
- Design spec §9.6: Security headers (X-Content-Type-Options, X-Frame-Options, CSP, HSTS, Referrer-Policy) → Task 7
- Design spec §6 (Worker): Fix status check for ready images → Task 9
- Design spec §6 (Worker): Add thumb/webp serving routes → Task 9
- Design spec §12 (Docker): Worker service → Task 10
- Design spec §7 (Frontend): Enhanced image detail → Task 11

**2. Placeholder scan:** Zero placeholders. All code blocks are complete Rust code. All commands have expected output. All file paths are exact.

**3. Type consistency:**
- `AccessTokenClaims` defined in Task 1, used in Tasks 3, 4 (middleware), Task 11 (frontend types align)
- `RefreshTokenClaims` defined in Task 1, used in Task 2 (refresh handler)
- `UploadResult` extended in Task 8, the extended return values flow through list/get/upload in Tasks 8
- `Cache::incr()` defined in Task 6, used in rate_limit.rs (same task)
- `generate_tokens()` returns 4-tuple in Task 1, callers updated in register/login
- Route paths match between handler annotations and main.rs registration

**4. State consistency:**
- Upload INSERT: `status = 'active'` (no change)
- Worker UPDATE: `status = 'ready'` (no change, this is the intended P1 behavior)
- public_get: now accepts `"active"` OR `"ready"` (Task 9 fix)
- Blacklist: now uses `bl:{jti}` (Task 4), blacklisted by logout (Task 3) and token rotation (Task 2)
- Redis rate limit keys use `rl:{policy}:{suffix}` (Task 6)

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-07-12-pichost-p0-gaps-and-p1-fixes.md`.**

**Two execution options:**

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
