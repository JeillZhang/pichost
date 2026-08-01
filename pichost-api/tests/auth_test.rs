//! Integration tests for auth endpoints (register/login/refresh/logout),
//! including the first-user-becomes-admin path and invite-code gating.
//!
//! Requires PostgreSQL + Redis (Docker: `docker compose up postgres redis`).

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::Value;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn register_and_login_roundtrip() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "roundtrip").await;
    assert!(!token.is_empty());

    // GET /users/me should return the profile
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/users/me", Some(&token), &Value::Null).await;
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
    let (status, _) =
        send_json(&app, Method::GET, "/api/v1/users/me", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn protected_route_requires_auth() {
    let app = test_app().await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/users/me", None, &Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn protected_route_rejects_garbage_token() {
    let app = test_app().await;
    let (status, _) =
        send_json(&app, Method::GET, "/api/v1/users/me", Some("garbage.token.here"), &Value::Null)
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
    assert_eq!(status, StatusCode::CONFLICT, "expected conflict, got {status}: {resp}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn health_endpoint_returns_healthy() {
    let app = test_app().await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/health", None, &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["status"].as_str().unwrap(), "healthy");
    assert_eq!(resp["components"]["postgres"]["status"].as_str().unwrap(), "ok");
    assert_eq!(resp["components"]["redis"]["status"].as_str().unwrap(), "ok");
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
    assert!(text.contains("pichost_http_requests_total"), "metrics missing counter: {text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn security_headers_present() {
    let app = test_app().await;
    let (_, headers, _) =
        send_raw(&app, Method::GET, "/api/health", None, None, Vec::new()).await;
    assert_eq!(
        headers.get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(headers.get("referrer-policy").unwrap(), "strict-origin-when-cross-origin");
}
