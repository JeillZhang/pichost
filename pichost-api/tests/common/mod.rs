//! Shared integration-test harness.
//!
//! Requires PostgreSQL + Redis reachable at the URLs below (override via
//! `PICHOST_DATABASE_URL` / `PICHOST_REDIS_URL` env vars). Tests build a real
//! `AppState` and drive the production router with `tower::ServiceExt::oneshot`.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use pichost_api::app::{configure_app, AppState};
use pichost_api::cache::{self, Cache};
use pichost_api::db;
use pichost_core::config::AppConfig;
use pichost_core::storage::local::LocalStorage;
use pichost_core::storage::StorageBackend;
use pichost_core::StorageRouter;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

/// Base64-encoded 32-byte key (bytes 0..=31) for AES-GCM token encryption.
pub const TEST_ENC_KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
pub const TEST_JWT_SECRET: &str = "test-jwt-secret-0123456789abcdef0123456789abcdef";
/// Default quota applied by the harness (100 MiB) so quota tests are meaningful.
pub const TEST_QUOTA: i64 = 100 * 1024 * 1024;

fn test_db_url() -> String {
    std::env::var("PICHOST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pichost:pichost@localhost:5432/pichost".to_string())
}

fn test_redis_url() -> String {
    std::env::var("PICHOST_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

/// Build an `AppConfig` suited for tests: raised rate limits, a fixed JWT
/// secret, a token-encryption key, and a tempdir-backed local storage path.
pub fn test_config(tempdir: &TempDir) -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.database.url = test_db_url();
    cfg.redis.url = test_redis_url();
    cfg.auth.jwt_secret = TEST_JWT_SECRET.to_string();
    cfg.token_encryption_key = Some(TEST_ENC_KEY.to_string());
    cfg.server.public_url = "http://localhost:3000".to_string();
    cfg.storage.local_base_path = tempdir.path().to_path_buf();
    cfg.storage.default_backend = "local".to_string();
    // Raise rate limits far above anything a test suite could hit.
    cfg.rate_limit.auth_max = 1_000_000;
    cfg.rate_limit.upload_max = 1_000_000;
    cfg.rate_limit.general_max = 1_000_000;
    cfg.rate_limit.public_max = 1_000_000;
    cfg
}

/// A ready-to-drive test app: the production router plus the shared state.
pub struct TestApp {
    pub router: Router,
    pub state: Arc<AppState>,
    /// Keeps the local-storage tempdir alive.
    _tempdir: TempDir,
}

impl TestApp {
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.state.pool
    }
}

/// Create the pool and run migrations once per process (idempotent).
/// Shared via OnceCell so many parallel tests reuse a single PG pool
/// instead of exhausting the server's max_connections.
/// Create a dedicated pool + run migrations for this test. Each test owns
/// its own pool (created inside its own runtime) so it cannot be poisoned by
/// another test's runtime shutdown. max_connections is kept small so many
/// parallel tests fit under the server's connection limit.
async fn init_pool() -> sqlx::PgPool {
    let pool = db::create_pg_pool(&test_db_url(), 5)
        .await
        .expect("failed to connect to test PostgreSQL (is it running?)");
    db::run_pg_migrations(&pool)
        .await
        .expect("failed to run migrations");
    pool
}

/// Build a test app from a fully-prepared `AppConfig` (custom rate limits,
/// OAuth credentials, etc.). The local-storage path is overridden with a
/// fresh tempdir kept alive by the returned `TestApp`.
pub async fn test_app_with_config(mut config: AppConfig) -> TestApp {
    let tempdir = TempDir::new().expect("tempdir");
    config.storage.local_base_path = tempdir.path().to_path_buf();
    let config = Arc::new(config);

    let pool = init_pool().await;
    let cache_pool = cache::create_pool(&test_redis_url(), 5);
    let cache = Arc::new(Cache::new(cache_pool));

    let local = Arc::new(LocalStorage::new(
        config.storage.local_base_path.clone(),
        config.server.public_url.clone(),
    )) as Arc<dyn StorageBackend>;
    let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
    backends.insert("local".into(), local);
    let storage_router = Arc::new(StorageRouter::new(
        backends,
        config.storage.default_backend.clone(),
    ));

    let state = Arc::new(AppState {
        pool,
        cache,
        config,
        router: storage_router,
    });
    let router = configure_app(state.clone());
    TestApp {
        router,
        state,
        _tempdir: tempdir,
    }
}

/// Build a full test app backed by the real PG + Redis and a fresh local
/// storage dir. Each call gets its own tempdir, PG pool, and Redis pool.
pub async fn test_app() -> TestApp {
    let tempdir = TempDir::new().expect("tempdir");
    let config = test_config(&tempdir);
    test_app_with_config(config).await
}

/// Register a fresh user through the real API. The first user in an empty
/// table becomes admin; to be deterministic the caller picks `is_admin`
/// by directly updating the row afterwards. Returns (username, password).
pub async fn register_user(app: &TestApp, username: &str, password: &str) -> (StatusCode, Value) {
    let body = serde_json::json!({
        "username": username,
        "password": password,
        "email": format!("{}@example.com", username),
    });
    send_json(app, Method::POST, "/api/v1/auth/register", None, &body).await
}

/// Directly promote a user to admin (bypasses registration order races).
pub async fn make_admin(app: &TestApp, user_id: Uuid) {
    sqlx::query("UPDATE users SET is_admin = true WHERE id = $1")
        .bind(user_id)
        .execute(app.pool())
        .await
        .expect("promote user");
}

/// Login and return the parsed auth response (access/refresh tokens + user).
pub async fn login(app: &TestApp, username: &str, password: &str) -> (StatusCode, Value) {
    let body = serde_json::json!({ "username": username, "password": password });
    send_json(app, Method::POST, "/api/v1/auth/login", None, &body).await
}

/// Create an admin user end-to-end: register → promote → login.
/// Returns (access_token, user_id).
pub async fn create_admin(app: &TestApp, tag: &str) -> (String, Uuid) {
    let username = short_username("admin", tag);
    let password = "admin123456";
    let (status, resp) = register_user(app, &username, password).await;
    assert!(
        status.is_success(),
        "admin register failed: {status} {resp}"
    );
    let user_id = Uuid::parse_str(resp["user"]["id"].as_str().unwrap()).unwrap();
    make_admin(app, user_id).await;
    let (status, resp) = login(app, &username, password).await;
    assert!(status.is_success(), "admin login failed: {status} {resp}");
    let token = resp["access_token"].as_str().unwrap().to_string();
    (token, user_id)
}

/// Create a regular (non-admin) user. If the users table already contains
/// users, registration requires an invite code — so we insert the user
/// directly and log in through the API. Returns (username, token, user_id).
pub async fn create_user(app: &TestApp, tag: &str) -> (String, String, Uuid) {
    let username = short_username("user", tag);
    let password = "user123456";
    let (status, resp) = register_user(app, &username, password).await;
    let user_id = if status.is_success() {
        let uid = Uuid::parse_str(resp["user"]["id"].as_str().unwrap()).unwrap();
        // Ensure the user is not an admin even if the table happened to be empty.
        sqlx::query("UPDATE users SET is_admin = false WHERE id = $1")
            .bind(uid)
            .execute(app.pool())
            .await
            .expect("demote user");
        uid
    } else {
        // Table already has users — insert directly and log in.
        let uid = insert_user_direct(app, &username, password, false).await;
        let (lstatus, lresp) = login(app, &username, password).await;
        assert!(lstatus.is_success(), "user login failed: {lstatus} {lresp}");
        uid
    };
    let (_, resp) = login(app, &username, password).await;
    let token = resp["access_token"].as_str().unwrap().to_string();
    (username, token, user_id)
}

/// Insert a user row directly with a real Argon2 hash, then return the id.
pub async fn insert_user_direct(
    app: &TestApp,
    username: &str,
    password: &str,
    is_admin: bool,
) -> Uuid {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string();
    sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, is_admin, storage_quota) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(username)
    .bind(format!("{}@example.com", username))
    .bind(hash)
    .bind(is_admin)
    .bind(TEST_QUOTA)
    .fetch_one(app.pool())
    .await
    .expect("insert user")
}

/// Run a JSON request against the router and return the response.
pub async fn send_json(
    app: &TestApp,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: &Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    builder = builder.header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = builder
        .body(Body::from(body.to_string()))
        .expect("build request");
    let resp = app.router.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024)
        .await
        .expect("read body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string()))
    };
    (status, value)
}

/// Send a request with a raw body (e.g. multipart) and return the response.
pub async fn send_raw(
    app: &TestApp,
    method: Method,
    uri: &str,
    token: Option<&str>,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(ct) = content_type {
        builder = builder.header(header::CONTENT_TYPE, ct);
    }
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = builder.body(Body::from(body)).expect("build request");
    let resp = app.router.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024)
        .await
        .expect("read body")
        .to_vec();
    (status, headers, bytes)
}

/// Build a minimal valid PNG byte buffer (1×1).
pub fn tiny_png() -> Vec<u8> {
    // 1x1 transparent PNG, hand-rolled.
    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR len + type
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, // bit depth/color + CRC
        0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, // IDAT len + type
        0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, // zlib data
        0x0D, 0x0A, 0x2D, 0xB4, // IDAT CRC
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82, // IEND
    ];
    png.to_vec()
}

/// Build a multipart body for an image upload.
pub fn multipart_image(
    file_name: &str,
    bytes: &[u8],
    extra_fields: &[(&str, &str)],
) -> (String, Vec<u8>) {
    let boundary = "----pichosttestboundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();
    for (name, value) in extra_fields {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let ct = format!("multipart/form-data; boundary={boundary}");
    (ct, body)
}

/// Register + login a normal user via the full API (returns token).
pub async fn auth_token(app: &TestApp, tag: &str) -> String {
    let (_, token, _) = create_user(app, tag).await;
    token
}

/// Build a short unique username within the DB's varchar(64) limit.
fn short_username(kind: &str, tag: &str) -> String {
    let tag: String = tag
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    let uid: String = Uuid::new_v4().simple().to_string();
    format!("{}_{}_{}", kind, tag, &uid[..8])
}

/// Create a valid invite code via the cache (returns the code string).
pub async fn create_invite(app: &TestApp, ttl_secs: u64) -> String {
    app.state
        .cache
        .create_invite_code(&Uuid::nil(), ttl_secs)
        .await
        .expect("create invite code")
}
