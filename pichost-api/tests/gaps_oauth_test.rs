mod common;

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

const JWT_SECRET: &str = "test-jwt-secret-0123456789abcdef0123456789abcdef";

struct OAuthApp {
    router: Router,
    state: Arc<AppState>,
    _tempdir: TempDir,
}

fn db_url() -> String {
    std::env::var("PICHOST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pichost:pichost@localhost:5432/pichost".to_string())
}

fn redis_url() -> String {
    std::env::var("PICHOST_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

/// Full app like `common::test_app()` but with OAuth credentials configured.
async fn oauth_app() -> OAuthApp {
    let tempdir = TempDir::new().expect("tempdir");
    let mut cfg = AppConfig::default();
    cfg.database.url = db_url();
    cfg.redis.url = redis_url();
    cfg.auth.jwt_secret = JWT_SECRET.to_string();
    cfg.server.public_url = "http://localhost:3000".to_string();
    cfg.storage.local_base_path = tempdir.path().to_path_buf();
    cfg.storage.default_backend = "local".to_string();
    cfg.auth.oauth_github_client_id = Some("github-test-id".into());
    cfg.auth.oauth_github_client_secret = Some("github-test-secret".into());
    cfg.auth.oauth_google_client_id = Some("google-test-id".into());
    cfg.auth.oauth_google_client_secret = Some("google-test-secret".into());
    cfg.rate_limit.auth_max = 1_000_000;
    cfg.rate_limit.upload_max = 1_000_000;
    cfg.rate_limit.general_max = 1_000_000;
    cfg.rate_limit.public_max = 1_000_000;

    let config = Arc::new(cfg);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&db_url())
        .await
        .expect("connect to test PostgreSQL");
    db::run_migrations(&pool).await.expect("run migrations");
    let cache_pool = cache::create_pool(&redis_url(), 5);
    let cache = Arc::new(Cache::new(cache_pool));

    let local = Arc::new(LocalStorage::new(
        config.storage.local_base_path.clone(),
        config.server.public_url.clone(),
    )) as Arc<dyn StorageBackend>;
    let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();
    backends.insert("local".into(), local);
    let storage_router =
        Arc::new(StorageRouter::new(backends, config.storage.default_backend.clone()));

    let state = Arc::new(AppState {
        pool,
        cache,
        config,
        router: storage_router,
    });
    let router = configure_app(state.clone());
    OAuthApp {
        router,
        state,
        _tempdir: tempdir,
    }
}

/// Drive a request through the router; returns status, headers and raw body.
async fn request(
    app: &OAuthApp,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = builder
        .body(body.map(|v| Body::from(v.to_string())).unwrap_or_else(Body::empty))
        .expect("build request");
    let resp = app.router.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024)
        .await
        .expect("read body")
        .to_vec();
    (status, headers, bytes)
}

fn json_of(bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(bytes).unwrap_or(Value::Null)
    }
}

/// Insert a user directly and log in; returns the access token.
async fn create_test_user(app: &OAuthApp) -> String {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(b"user123456", &salt)
        .unwrap()
        .to_string();
    let username = format!("oauth_{}", &Uuid::new_v4().simple().to_string()[..10]);
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, email, password_hash, is_admin, storage_quota) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(&username)
    .bind(format!("{username}@example.com"))
    .bind(hash)
    .bind(false)
    .bind(100_i64 * 1024 * 1024)
    .fetch_one(&app.state.pool)
    .await
    .expect("insert user");
    let (_, _, bytes) = request(
        app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(serde_json::json!({"username": username, "password": "user123456"})),
    )
    .await;
    json_of(&bytes)["access_token"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_github_redirect_redirects_to_github() {
    let app = oauth_app().await;
    let (status, headers, _) =
        request(&app, Method::GET, "/api/v1/auth/oauth/github", None, None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let loc = headers
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .expect("location header");
    assert!(loc.starts_with("https://github.com/login/oauth/authorize"));
    assert!(loc.contains("client_id=github-test-id"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_google_redirect_redirects_to_google() {
    let app = oauth_app().await;
    let (status, headers, _) =
        request(&app, Method::GET, "/api/v1/auth/oauth/google", None, None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let loc = headers
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .expect("location header");
    assert!(loc.starts_with("https://accounts.google.com/o/oauth2/v2/auth"));
    assert!(loc.contains("client_id=google-test-id"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_callback_bad_code_rejected() {
    let app = oauth_app().await;
    let uri = "/api/v1/auth/oauth/github/callback?code=badcode&state=x";
    let (status, _, bytes) = request(&app, Method::GET, uri, None, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json_of(&bytes)["error"], "invalid authorization code");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_google_callback_bad_code_rejected() {
    let app = oauth_app().await;
    let uri = "/api/v1/auth/oauth/google/callback?code=badcode&state=x";
    let (status, _, bytes) = request(&app, Method::GET, uri, None, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json_of(&bytes)["error"], "invalid authorization code");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_link_bad_code_rejected() {
    let app = oauth_app().await;
    let token = create_test_user(&app).await;
    let body = serde_json::json!({"provider": "github", "code": "badcode"});
    let (status, _, bytes) =
        request(&app, Method::POST, "/api/v1/users/oauth/link", Some(&token), Some(body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json_of(&bytes)["error"], "invalid authorization code");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_link_google_bad_code_rejected() {
    let app = oauth_app().await;
    let token = create_test_user(&app).await;
    let body = serde_json::json!({"provider": "google", "code": "badcode"});
    let (status, _, bytes) =
        request(&app, Method::POST, "/api/v1/users/oauth/link", Some(&token), Some(body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json_of(&bytes)["error"], "invalid authorization code");
}
