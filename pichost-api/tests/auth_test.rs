//! Integration tests for auth endpoints (register/login/refresh/logout),
//! including the first-user-becomes-admin path and invite-code gating.
//!
//! Requires PostgreSQL + Redis (Docker: `docker compose up postgres redis`).

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use common::*;
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn register_and_login_roundtrip() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "roundtrip").await;
    assert!(!token.is_empty());

    // GET /users/me should return the profile
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/users/me",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["id"].as_str().unwrap(), user_id.to_string().as_str());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn login_with_wrong_password_fails() {
    let app = test_app().await;
    let _ = create_user(&app, "wrongpass").await;
    // Attempt login with a wrong password for a non-existent user.
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        &serde_json::json!({"username": "definitely_not_there", "password": "nope1234"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn login_rejects_malformed_json_body() {
    let app = test_app().await;
    let (status, _, bytes) = send_raw(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some("application/json"),
        b"{\"bad json".to_vec(),
    )
    .await;
    // Malformed JSON syntax is a JsonSyntaxError, whose status() is 400;
    // JsonBody maps every Json rejection to validation.body_invalid.
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let v: Value = serde_json::from_slice(&bytes).expect("error body is JSON");
    assert_eq!(v["code"], "validation.body_invalid");
    assert!(v["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn register_rejects_short_password() {
    let app = test_app().await;
    let (status, resp) = register_user(&app, "shortpw", "123").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("at least 6"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn refresh_rotates_tokens() {
    let app = test_app().await;
    let (username, _, _) = create_user(&app, "refresh").await;
    let (lstatus, lresp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        &serde_json::json!({"username": username, "password": "user123456"}),
    )
    .await;
    assert_eq!(lstatus, StatusCode::OK);
    let refresh = lresp["refresh_token"].as_str().unwrap().to_string();
    assert!(!refresh.is_empty());

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        &serde_json::json!({"refresh_token": refresh}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "refresh failed: {resp}");
    assert!(resp["access_token"].as_str().is_some());
    assert!(resp["refresh_token"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn refresh_rejects_invalid_token() {
    let app = test_app().await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        &serde_json::json!({"refresh_token": "not-a-jwt"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn logout_blacklists_token() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "logout").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "logout failed: {resp}");

    // Token should now be revoked → protected endpoint returns 401.
    let (status, _) = send_json(
        &app,
        Method::GET,
        "/api/v1/users/me",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn protected_route_requires_auth() {
    let app = test_app().await;
    let (status, resp) = send_json(&app, Method::GET, "/api/v1/users/me", None, &Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn protected_route_rejects_garbage_token() {
    let app = test_app().await;
    let (status, _) = send_json(
        &app,
        Method::GET,
        "/api/v1/users/me",
        Some("garbage.token.here"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn register_duplicate_username_conflicts() {
    let app = test_app().await;
    let (username, _, _) = create_user(&app, "dup").await;

    // A non-first registration needs a valid invite code to reach the
    // username-uniqueness insert, which then 409s.
    let code = create_invite(&app, 3600).await;
    let body = serde_json::json!({
        "username": username,
        "password": "user123456",
        "invite_code": code,
    });
    let (status, resp) = send_json(&app, Method::POST, "/api/v1/auth/register", None, &body).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "expected conflict, got {status}: {resp}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn health_endpoint_returns_healthy() {
    let app = test_app().await;
    let (status, resp) = send_json(&app, Method::GET, "/api/health", None, &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["status"].as_str().unwrap(), "healthy");
    assert_eq!(
        resp["components"]["postgres"]["status"].as_str().unwrap(),
        "ok"
    );
    assert_eq!(
        resp["components"]["redis"]["status"].as_str().unwrap(),
        "ok"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn metrics_endpoint_returns_text() {
    let app = test_app().await;
    // Warm up the metrics middleware so the counter family registers.
    let _ = send_raw(&app, Method::GET, "/api/health", None, None, Vec::new()).await;
    let (status, _headers, body) =
        send_raw(&app, Method::GET, "/metrics", None, None, Vec::new()).await;
    assert_eq!(status, StatusCode::OK);
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("pichost_http_requests_total"),
        "metrics missing counter: {text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn security_headers_present() {
    let app = test_app().await;
    let (_, headers, _) = send_raw(&app, Method::GET, "/api/health", None, None, Vec::new()).await;
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(
        headers.get("referrer-policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_login_failure_returns_code_and_localized_message() {
    let app = test_app().await;
    let (status, body) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        &serde_json::json!({"username": "nobody", "password": "x"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "auth.invalid_credentials");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_login_failure_zh_negotiation() {
    let app = test_app().await;
    let resp = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("accept-language", "zh-CN")
                .body(Body::from(r#"{"username":"nobody","password":"x"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"], "用户名或密码错误");
    assert_eq!(body["code"], "auth.invalid_credentials");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_endpoint_forbidden_code() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "forbidden").await; // non-admin
    let (status, body) = send_json(
        &app,
        Method::GET,
        "/api/v1/admin/stats",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "auth.admin_required");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_rate_limit_returns_code() {
    let app = test_app_with_rate_limits(2).await;
    let body = serde_json::json!({"username": "x", "password": "y"});
    for _ in 0..3 {
        send_json(&app, Method::POST, "/api/v1/auth/login", None, &body).await;
    }
    let (status, resp) = send_json(&app, Method::POST, "/api/v1/auth/login", None, &body).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(resp["code"], "rate_limited");
}

/// Test app with a lowered auth rate limit so the limiter trips quickly.
async fn test_app_with_rate_limits(auth_max: u32) -> TestApp {
    let tempdir = tempfile::TempDir::new().expect("tempdir");
    let mut cfg = common::test_config(&tempdir);
    cfg.rate_limit.auth_max = auth_max;
    common::test_app_with_config(cfg).await
}

fn test_cache() -> pichost_api::cache::Cache {
    let url =
        std::env::var("PICHOST_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    pichost_api::cache::Cache::new(pichost_api::cache::create_pool(&url, 5))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Redis"]
async fn redis_blacklist_revoke_blocks_jti() {
    use pichost_core::state::Blacklist;
    let cache = test_cache();
    let bl = pichost_api::middleware::auth::RedisBlacklist::new(cache);
    let jti = format!("jti-{}", uuid::Uuid::new_v4());
    assert!(!bl.check(&jti).await.unwrap());
    bl.revoke(&jti, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    assert!(bl.check(&jti).await.unwrap());
}
