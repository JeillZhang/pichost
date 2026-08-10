//! Coverage-gap integration tests round 2: upload max-size rejection,
//! direct service-level multipart extraction, and route edge cases for
//! cross-user batch-delete / category ownership. Requires PG + Redis.

mod common;

use axum::body::Body;
use axum::extract::{FromRequest, Multipart};
use axum::http::{header, Method, Request, StatusCode};
use axum::Json;
use common::*;
use pichost_api::services::upload::extract_file_from_multipart;
use serde_json::{json, Value};
use uuid::Uuid;

async fn upload_png(app: &TestApp, token: &str, name: &str, bytes: &[u8]) -> Value {
    let (ct, body) = multipart_image(name, bytes, &[]);
    let (status, _, raw) = send_raw(
        app,
        Method::POST,
        "/api/v1/images",
        Some(token),
        Some(&ct),
        body,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "upload failed: {}",
        String::from_utf8_lossy(&raw)
    );
    serde_json::from_slice::<Value>(&raw).expect("upload response is JSON")
}

async fn upload_expect_error(
    app: &TestApp,
    token: Option<&str>,
    ct: &str,
    body: Vec<u8>,
) -> (StatusCode, Value) {
    let (status, _, raw) =
        send_raw(app, Method::POST, "/api/v1/images", token, Some(ct), body).await;
    let value = if raw.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&raw).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn create_category(app: &TestApp, token: &str, name: &str) -> Uuid {
    let (status, resp) = send_json(
        app,
        Method::POST,
        "/api/v1/categories",
        Some(token),
        &json!({"name": name}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create category failed: {resp}"
    );
    Uuid::parse_str(resp["id"].as_str().unwrap()).unwrap()
}

async fn extract_via_request(
    ct: &str,
    body: Vec<u8>,
) -> Result<(Vec<u8>, String), (StatusCode, Json<Value>)> {
    let req = Request::builder()
        .method(Method::POST)
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(body))
        .unwrap();
    let multipart = Multipart::from_request(req, &()).await.unwrap();
    extract_file_from_multipart(multipart).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_exceeds_max_size_rejected() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "g2size").await;
    let mut big = tiny_png();
    big.extend(std::iter::repeat_n(0u8, 10_500_000));
    let (ct, body) = multipart_image("big.png", &big, &[]);
    let (status, resp) = upload_expect_error(&app, Some(&token), &ct, body).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(resp["error"]
        .as_str()
        .unwrap()
        .contains("maximum allowed size"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn service_extract_file_from_multipart() {
    let (ct, body) = multipart_image("direct.png", &tiny_png(), &[]);
    let (bytes, name) = extract_via_request(&ct, body).await.unwrap();
    assert_eq!(bytes, tiny_png());
    assert_eq!(name, "direct.png");

    let boundary = "----gaps2boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"x\"\r\n\r\nvalue\r\n--{boundary}--\r\n"
    );
    let ct = format!("multipart/form-data; boundary={boundary}");
    let err = extract_via_request(&ct, body.into_bytes())
        .await
        .unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST);
    assert!(err.1 .0["error"]
        .as_str()
        .unwrap()
        .contains("no file field"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn batch_delete_foreign_image_succeeds() {
    let app = test_app().await;
    let (_, token_b, _) = create_user(&app, "g2bd-u").await;
    let (username, _, user_id) = create_user(&app, "g2bd-a").await;
    make_admin(&app, user_id).await;
    let (status, resp) = login(&app, &username, "user123456").await;
    assert!(status.is_success(), "admin login failed: {resp}");
    let admin_token = resp["access_token"].as_str().unwrap().to_string();
    let results = upload_png(&app, &token_b, "foreign.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap();
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/batch-delete",
        Some(&admin_token),
        &json!({"ids": [id]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert_eq!(resp["deleted"], 1);
    assert_eq!(resp["failed"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn move_image_foreign_category_404() {
    let app = test_app().await;
    let (_, token_a, _) = create_user(&app, "g2mv-a").await;
    let (_, token_b, _) = create_user(&app, "g2mv-b").await;
    let cat = create_category(&app, &token_b, "foreign").await;
    let results = upload_png(&app, &token_a, "m.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap();
    let (status, _) = send_json(
        &app,
        Method::POST,
        &format!("/api/v1/images/{id}/move"),
        Some(&token_a),
        &json!({"category_id": cat}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn batch_move_foreign_category_404() {
    let app = test_app().await;
    let (_, token_a, _) = create_user(&app, "g2bm-a").await;
    let (_, token_b, _) = create_user(&app, "g2bm-b").await;
    let cat = create_category(&app, &token_b, "bucket").await;
    let results = upload_png(&app, &token_a, "m1.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap();
    let (status, _) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/batch-move",
        Some(&token_a),
        &json!({"image_ids": [id], "category_id": cat}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
