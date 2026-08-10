//! Coverage-gap integration tests for auth endpoints: invite-code states
//! (used/unknown/expired), refresh edge cases (invalid sub, deleted user,
//! wrong typ, rotation), and logout edge cases (missing header, garbage,
//! refresh token). Requires PostgreSQL + Redis.
//!
//! OAuth redirect/callback/link gaps are covered by `gaps_oauth_test.rs`.

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use jsonwebtoken::{encode, EncodingKey, Header};
use pichost_api::routes::auth::RefreshTokenClaims;
use serde_json::Value;
use uuid::Uuid;

fn unique_username(tag: &str) -> String {
    format!("{}_{}", tag, &Uuid::new_v4().simple().to_string()[..10])
}

fn register_body(username: &str, code: &str) -> Value {
    serde_json::json!({
        "username": username,
        "password": "user123456",
        "invite_code": code,
    })
}

fn refresh_body(token: &str) -> Value {
    serde_json::json!({ "refresh_token": token })
}

fn mint_refresh_with_typ(sub: &str, typ: &str) -> String {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = RefreshTokenClaims {
        sub: sub.to_string(),
        jti: Uuid::new_v4().to_string(),
        exp: now + 3600,
        iat: now,
        is_admin: false,
        typ: typ.to_string(),
        access_jti: Uuid::new_v4().to_string(),
        access_exp: now + 900,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TEST_JWT_SECRET.as_bytes()),
    )
    .expect("mint refresh token")
}

/// Mint a valid refresh token (HS256, TEST_JWT_SECRET) for any subject.
fn mint_refresh(sub: &str) -> String {
    mint_refresh_with_typ(sub, "refresh")
}

async fn login_get_refresh(app: &TestApp, username: &str) -> String {
    let (status, resp) = send_json(
        app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        &serde_json::json!({"username": username, "password": "user123456"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login failed: {resp}");
    resp["refresh_token"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn register_rejects_used_invite() {
    let app = test_app().await;
    let _ = create_user(&app, "usedinv").await;
    let code = create_invite(&app, 3600).await;

    let first = unique_username("used1");
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        None,
        &register_body(&first, &code),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "first register failed: {resp}");

    let second = unique_username("used2");
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        None,
        &register_body(&second, &code),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "got {status}: {resp}");
    assert_eq!(resp["error"], "invite code has already been used");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn register_rejects_unknown_invite() {
    let app = test_app().await;
    let _ = create_user(&app, "unkinv").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        None,
        &register_body(&unique_username("unk"), "no-such-code-0000000000"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "got {status}: {resp}");
    assert_eq!(resp["error"], "invalid invite code");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn register_rejects_expired_invite() {
    let app = test_app().await;
    let _ = create_user(&app, "expinv").await;
    let code = create_invite(&app, 3600).await;
    let key = format!("pichost:invite:{code}");
    let past = (chrono::Utc::now().timestamp() - 100).to_string();
    let mut conn = app.redis_pool().get().await.expect("redis conn");
    deadpool_redis::redis::cmd("HSET")
        .arg(&key)
        .arg("expires_at")
        .arg(&past)
        .query_async::<_, ()>(&mut *conn)
        .await
        .expect("hset expires_at");

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/register",
        None,
        &register_body(&unique_username("exp"), &code),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "got {status}: {resp}");
    assert_eq!(resp["error"], "invite code has expired");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn refresh_rejects_invalid_subject() {
    let app = test_app().await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        &refresh_body(&mint_refresh("not-a-uuid")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "got {status}: {resp}");
    assert_eq!(resp["error"], "invalid token subject");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn refresh_rejects_deleted_user() {
    let app = test_app().await;
    let (username, _, user_id) = create_user(&app, "delusr").await;
    let refresh = login_get_refresh(&app, &username).await;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(app.pool())
        .await
        .expect("delete user");

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        &refresh_body(&refresh),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "got {status}: {resp}");
    assert_eq!(resp["error"], "user not found");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn refresh_rejects_access_token() {
    let app = test_app().await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        &refresh_body(&mint_refresh_with_typ("some-user", "access")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "got {status}: {resp}");
    assert_eq!(resp["error"], "invalid token type");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn refresh_rejects_rotated_token() {
    let app = test_app().await;
    let (username, _, _) = create_user(&app, "rotref").await;
    let old_refresh = login_get_refresh(&app, &username).await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        &refresh_body(&old_refresh),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "first refresh failed: {resp}");

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        &refresh_body(&old_refresh),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "got {status}: {resp}");
    assert_eq!(resp["error"], "refresh token has been revoked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn logout_requires_authorization() {
    let app = test_app().await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        None,
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "got {status}: {resp}");
    assert_eq!(resp["error"], "missing authorization header");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn logout_rejects_garbage_token() {
    let app = test_app().await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        Some("garbage.token.here"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "got {status}: {resp}");
    assert_eq!(resp["error"], "invalid token");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn logout_rejects_refresh_token() {
    let app = test_app().await;
    let (username, _, _) = create_user(&app, "logoutref").await;
    let refresh = login_get_refresh(&app, &username).await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        Some(&refresh),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "got {status}: {resp}");
    assert!(
        resp["error"]
            .as_str()
            .unwrap()
            .contains("only access tokens"),
        "unexpected error: {resp}"
    );
}
