//! End-to-end test of SQLite lite mode (T26).
//!
//! Assembles a real lite-mode app — SQLite pool + migrations + the SQLite
//! trait-object state implementations + the embedded worker — and drives the
//! production router with `tower::ServiceExt::oneshot` through the full flow:
//! register → login → upload → list → public serve → thumbnail (produced by
//! the embedded worker). Runs in the DEFAULT suite: no PostgreSQL, no Redis.

mod common;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use common::*;
use serde_json::{json, Value};
use std::time::Instant;
use tower::ServiceExt;

/// Assemble a lite-mode app and return (router, tempdir). The tempdir keeps
/// the SQLite file and the local-storage directory alive for the test body.
async fn lite_app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_url = format!("sqlite://{}/e2e.db", dir.path().display());
    let pool = pichost_core::db::create_sqlite_pool(&db_url, 5)
        .await
        .expect("sqlite pool");
    pichost_core::db::run_sqlite_migrations(&pool)
        .await
        .expect("sqlite migrations");

    let config = test_config(&dir);
    let state = pichost_api::app::build_lite_app_state(config, pool).await;
    let router = pichost_api::app::configure_app(state.clone());
    (router, dir)
}

/// JSON request helper (mirrors `common::send_json` but takes the raw router).
async fn send_json(
    router: &Router,
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
    let resp = router.clone().oneshot(req).await.expect("oneshot");
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

/// Raw-body request helper (multipart uploads).
async fn send_raw(
    router: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    content_type: &str,
    body: Vec<u8>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    builder = builder.header(header::CONTENT_TYPE, content_type);
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = builder.body(Body::from(body)).expect("build request");
    let resp = router.clone().oneshot(req).await.expect("oneshot");
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_lite_mode_full_flow() {
    let (router, _dir) = lite_app().await;

    // a. Register the first user — auto-admin, no invite code needed.
    let (status, resp) = send_json(
        &router,
        Method::POST,
        "/api/v1/auth/register",
        None,
        &json!({"username": "admin", "password": "admin123456"}),
    )
    .await;
    assert!(status.is_success(), "register failed: {status} {resp}");
    assert_eq!(resp["user"]["is_admin"], json!(true), "first user is admin");

    // b. Login → access token.
    let (status, resp) = send_json(
        &router,
        Method::POST,
        "/api/v1/auth/login",
        None,
        &json!({"username": "admin", "password": "admin123456"}),
    )
    .await;
    assert!(status.is_success(), "login failed: {status} {resp}");
    let token = resp["access_token"]
        .as_str()
        .expect("access token")
        .to_string();

    // c. Upload a tiny PNG via multipart → 201 + UploadResult array.
    let (ct, body) = multipart_image("e2e.png", &tiny_png(), &[]);
    let (status, resp) = send_raw(
        &router,
        Method::POST,
        "/api/v1/images",
        Some(&token),
        &ct,
        body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "upload failed: {resp}");
    let image = &resp[0];
    let id = image["id"].as_str().expect("image id").to_string();
    let public_key = image["public_key"]
        .as_str()
        .expect("public key")
        .to_string();

    // d. GET /images → the list contains the uploaded image.
    let (status, resp) = send_json(
        &router,
        Method::GET,
        "/api/v1/images",
        Some(&token),
        &json!(null),
    )
    .await;
    assert!(status.is_success(), "list failed: {status} {resp}");
    let items = resp["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "list should contain exactly one image");
    assert_eq!(items[0]["public_key"], json!(public_key));

    // e. GET /u/{public_key} → 200 (public serve, unauthenticated).
    let (status, resp) = send_json(
        &router,
        Method::GET,
        &format!("/u/{public_key}"),
        None,
        &json!(null),
    )
    .await;
    assert!(status.is_success(), "public serve failed: {status} {resp}");

    // f. The embedded worker (same process, no Redis) generates the
    //    thumbnail. Poll the image detail for thumbnail_url (≤5s).
    let uri = format!("/api/v1/images/{id}");
    let start = Instant::now();
    let mut detail: Option<Value> = None;
    for _ in 0..50 {
        let (status, resp) =
            send_json(&router, Method::GET, &uri, Some(&token), &json!(null)).await;
        assert!(status.is_success(), "detail failed: {status} {resp}");
        if resp["thumbnail_url"].is_string() {
            detail = Some(resp);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let elapsed = start.elapsed();
    let detail = detail.expect("thumbnail_url appears within 5s (embedded worker)");
    let url = detail["thumbnail_url"].as_str().expect("thumbnail url");
    assert!(
        url.contains(&format!("/u/thumb/{id}")),
        "unexpected url: {url}"
    );
    assert_eq!(detail["status"], json!("ready"));
    tracing::info!("thumbnail ready after {elapsed:?}");
}
