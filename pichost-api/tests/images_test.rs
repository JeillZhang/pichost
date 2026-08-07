//! Integration tests for image endpoints: multipart + URL upload, gallery
//! list/get/rename/delete/move, batch operations, share links, and public
//! serving routes. Requires PostgreSQL + Redis (Docker compose).
//!
//! The URL-upload success case serves a local HTTP server on 203.0.114.1 —
//! a docker-network gateway address that is not blocked by the SSRF guard.

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::{json, Value};
use uuid::Uuid;

fn png_variant(extra: usize) -> Vec<u8> {
    let mut bytes = tiny_png();
    bytes.extend(std::iter::repeat_n(0u8, extra));
    bytes
}

fn multipart_without_file(fields: &[(&str, &str)]) -> (String, Vec<u8>) {
    let boundary = "----pichosttestboundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

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

async fn bind_public_listener() -> tokio::net::TcpListener {
    match tokio::net::TcpListener::bind("203.0.114.1:0").await {
        Ok(listener) => listener,
        Err(_) => {
            let _ = std::process::Command::new("docker")
                .args([
                    "network",
                    "create",
                    "--subnet=203.0.114.0/24",
                    "pichost-ssrf-net",
                ])
                .status();
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            tokio::net::TcpListener::bind("203.0.114.1:0")
                .await
                .expect("bind 203.0.114.1 (docker network pichost-ssrf-net required)")
        }
    }
}

async fn start_image_server(bytes: Vec<u8>) -> String {
    let listener = bind_public_listener().await;
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        while let Ok((mut sock, _)) = listener.accept().await {
            let bytes = bytes.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&bytes).await;
            });
        }
    });
    format!("http://{}:{}/tiny.png", addr.ip(), addr.port())
}

// ── POST /api/v1/images (multipart upload) ──────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_multipart_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "upok").await;
    let results = upload_png(&app, &token, "test.png", &tiny_png()).await;
    let r = &results[0];
    assert_eq!(r["original_name"], "test.png");
    assert_eq!(r["status"], "active");
    assert_eq!(r["mime_type"], "image/png");
    assert!(r["id"].as_str().is_some());
    assert!(r["public_key"].as_str().is_some());
    let url = r["url"].as_str().unwrap().to_string();
    assert!(url.ends_with(&format!("/u/{}", r["public_key"].as_str().unwrap())));
    assert!(r["markdown"].as_str().unwrap().contains(&url));
    assert!(r["html"].as_str().unwrap().contains(&url));
    assert!(r["bbcode"].as_str().unwrap().contains(&url));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_rejects_non_image() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "upbad").await;
    let (ct, body) = multipart_image("test.txt", b"plain text not an image", &[]);
    let (status, resp) = upload_expect_error(&app, Some(&token), &ct, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"]
        .as_str()
        .unwrap()
        .contains("not a valid image"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_rejects_missing_file_field() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "upnofile").await;
    let (ct, body) = multipart_without_file(&[("name", "value")]);
    let (status, resp) = upload_expect_error(&app, Some(&token), &ct, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("no file field"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_requires_auth() {
    let app = test_app().await;
    let (ct, body) = multipart_image("test.png", &tiny_png(), &[]);
    let (status, _, _) =
        send_raw(&app, Method::POST, "/api/v1/images", None, Some(&ct), body).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_dedup_returns_existing() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "updup").await;
    let first = upload_png(&app, &token, "dup.png", &tiny_png()).await;
    let second = upload_png(&app, &token, "dup.png", &tiny_png()).await;
    assert_eq!(first[0]["id"], second[0]["id"]);
    assert_eq!(first[0]["public_key"], second[0]["public_key"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_invalid_storage_config_ids() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "upcfg").await;
    let (ct, body) = multipart_image(
        "cfg.png",
        &tiny_png(),
        &[("storage_config_ids", "not-a-uuid")],
    );
    let (status, _) = upload_expect_error(&app, Some(&token), &ct, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let fake = Uuid::new_v4().to_string();
    let (ct, body) = multipart_image("cfg.png", &tiny_png(), &[("storage_config_ids", &fake)]);
    let (status, resp) = upload_expect_error(&app, Some(&token), &ct, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"]
        .as_str()
        .unwrap()
        .contains("no matching configs"));
}

// ── POST /api/v1/images/upload-url ──────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn url_upload_rejects_empty_url() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "urlempty").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/upload-url",
        Some(&token),
        &json!({"url": "   "}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("required"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn url_upload_rejects_bad_scheme() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "urlscheme").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/upload-url",
        Some(&token),
        &json!({"url": "file:///etc/passwd"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"]
        .as_str()
        .unwrap()
        .contains("unsupported URL scheme"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn url_upload_blocks_private_ip() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "urlssrf").await;
    for url in [
        "http://127.0.0.1:9/tiny.png",
        "http://169.254.169.254/latest/meta-data",
        "http://192.168.1.1/tiny.png",
    ] {
        let (status, resp) = send_json(
            &app,
            Method::POST,
            "/api/v1/images/upload-url",
            Some(&token),
            &json!({"url": url}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "url: {url}");
        assert!(
            resp["error"].as_str().unwrap().contains("private"),
            "unexpected error for {url}: {resp}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn url_upload_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "urlok").await;
    let server_url = start_image_server(tiny_png()).await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/upload-url",
        Some(&token),
        &json!({"url": server_url}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "url upload failed: {resp}");
    assert_eq!(resp[0]["original_name"], "tiny.png");
    assert_eq!(resp[0]["mime_type"], "image/png");
    assert!(resp[0]["public_key"].as_str().is_some());
}

// ── GET /api/v1/images (list) ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn list_images_empty() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "listempty").await;
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/images",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["items"], json!([]));
    assert_eq!(resp["total"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn list_images_requires_auth() {
    let app = test_app().await;
    let (status, _) = send_json(&app, Method::GET, "/api/v1/images", None, &Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn list_images_pagination() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "listpage").await;
    for (i, extra) in [0usize, 10, 30].into_iter().enumerate() {
        upload_png(&app, &token, &format!("img{i}.png"), &png_variant(extra)).await;
    }
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/images?page=1&per_page=2",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["items"].as_array().unwrap().len(), 2);
    assert_eq!(resp["total"], 3);
    assert_eq!(resp["total_pages"], 2);

    let (_, page2) = send_json(
        &app,
        Method::GET,
        "/api/v1/images?page=2&per_page=2",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(page2["items"].as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn list_images_sort_file_size() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "listsort").await;
    upload_png(&app, &token, "big.png", &png_variant(30)).await;
    upload_png(&app, &token, "small.png", &png_variant(0)).await;
    upload_png(&app, &token, "mid.png", &png_variant(10)).await;

    let (_, asc) = send_json(
        &app,
        Method::GET,
        "/api/v1/images?sort=file_size&order=asc",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(asc["items"][0]["file_size"], 67);
    assert_eq!(asc["items"][2]["file_size"], 97);

    let (_, desc) = send_json(
        &app,
        Method::GET,
        "/api/v1/images?sort=file_size&order=desc",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(desc["items"][0]["file_size"], 97);
    assert_eq!(desc["items"][2]["file_size"], 67);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn list_images_search() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "listsearch").await;
    upload_png(&app, &token, "sunset.png", &png_variant(1)).await;
    upload_png(&app, &token, "mountain.png", &png_variant(2)).await;

    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/images?search=sun",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["items"].as_array().unwrap().len(), 1);
    assert_eq!(resp["items"][0]["original_name"], "sunset.png");

    let (_, none) = send_json(
        &app,
        Method::GET,
        "/api/v1/images?search=zzz",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(none["items"].as_array().unwrap().len(), 0);
    assert_eq!(none["total"], 0);
}

// ── GET /api/v1/images/{id} ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_image_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "getok").await;
    let results = upload_png(&app, &token, "detail.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap().to_string();
    let (status, resp) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert_eq!(resp["id"], json!(id));
    assert_eq!(resp["original_name"], "detail.png");
    assert_eq!(resp["status"], "active");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_image_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "get404").await;
    let id = Uuid::new_v4().to_string();
    let (status, _) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_image_other_user_not_found() {
    let app = test_app().await;
    let (_, token_a, _) = create_user(&app, "owner-a").await;
    let (_, token_b, _) = create_user(&app, "owner-b").await;
    let results = upload_png(&app, &token_a, "secret.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap().to_string();
    let (status, _) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images/{id}"),
        Some(&token_b),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── PATCH /api/v1/images/{id} (rename) ──────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn rename_image_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "renok").await;
    let results = upload_png(&app, &token, "old.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap().to_string();
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &format!("/api/v1/images/{id}"),
        Some(&token),
        &json!({"original_name": "renamed.png"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert_eq!(resp["original_name"], "renamed.png");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn rename_image_rejects_invalid_names() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "renbad").await;
    let results = upload_png(&app, &token, "old.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap().to_string();
    let uri = format!("/api/v1/images/{id}");

    let (status, _) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&token),
        &json!({"original_name": ""}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&token),
        &json!({"original_name": "a".repeat(256)}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&token),
        &json!({"original_name": "bad/name.png"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn rename_image_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "ren404").await;
    let id = Uuid::new_v4().to_string();
    let (status, _) = send_json(
        &app,
        Method::PATCH,
        &format!("/api/v1/images/{id}"),
        Some(&token),
        &json!({"original_name": "x.png"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── DELETE /api/v1/images/{id} ──────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn delete_image_success_removes_file() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "delok").await;
    let results = upload_png(&app, &token, "gone.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap().to_string();
    let pk = results[0]["public_key"].as_str().unwrap().to_string();
    let file = app
        .state
        .config
        .storage
        .local_base_path
        .join(user_id.to_string())
        .join(&pk);
    assert!(file.exists(), "storage file should exist after upload");

    let (status, _) = send_json(
        &app,
        Method::DELETE,
        &format!("/api/v1/images/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!file.exists(), "storage file should be removed on delete");

    let (gstatus, _) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(gstatus, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn delete_image_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "del404").await;
    let id = Uuid::new_v4().to_string();
    let (status, _) = send_json(
        &app,
        Method::DELETE,
        &format!("/api/v1/images/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── POST /api/v1/images/{id}/move ───────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn move_image_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "moveok").await;
    let results = upload_png(&app, &token, "move.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap().to_string();
    let cat = create_category(&app, &token, "vacation").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        &format!("/api/v1/images/{id}/move"),
        Some(&token),
        &json!({"category_id": cat}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");

    let (_, img) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(img["category_id"], json!(cat.to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn move_image_category_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "movecat404").await;
    let results = upload_png(&app, &token, "move.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap().to_string();
    let (status, _) = send_json(
        &app,
        Method::POST,
        &format!("/api/v1/images/{id}/move"),
        Some(&token),
        &json!({"category_id": Uuid::new_v4()}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn move_image_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "moveimg404").await;
    let cat = create_category(&app, &token, "solo").await;
    let id = Uuid::new_v4().to_string();
    let (status, _) = send_json(
        &app,
        Method::POST,
        &format!("/api/v1/images/{id}/move"),
        Some(&token),
        &json!({"category_id": cat}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── POST /api/v1/images/batch-delete ────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn batch_delete_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "bdelok").await;
    let r1 = upload_png(&app, &token, "b1.png", &png_variant(1)).await;
    let r2 = upload_png(&app, &token, "b2.png", &png_variant(2)).await;
    let ids = json!({"ids": [r1[0]["id"], r2[0]["id"]]});
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/batch-delete",
        Some(&token),
        &ids,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert_eq!(resp["deleted"], 2);
    assert_eq!(resp["failed"], 0);

    let (gstatus, _) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images/{}", r1[0]["id"].as_str().unwrap()),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(gstatus, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn batch_delete_rejects_bad_input() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "bdelbad").await;
    let (status, _) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/batch-delete",
        Some(&token),
        &json!({"ids": []}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let too_many: Vec<Value> = (0..101)
        .map(|_| json!(Uuid::new_v4().to_string()))
        .collect();
    let (status, _) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/batch-delete",
        Some(&token),
        &json!({"ids": too_many}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ── POST /api/v1/images/batch-move ──────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn batch_move_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "bmoveok").await;
    let r1 = upload_png(&app, &token, "m1.png", &png_variant(1)).await;
    let r2 = upload_png(&app, &token, "m2.png", &png_variant(2)).await;
    let cat = create_category(&app, &token, "bucket").await;
    let body = json!({"image_ids": [r1[0]["id"], r2[0]["id"]], "category_id": cat});
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/batch-move",
        Some(&token),
        &body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert_eq!(resp["moved"], 2);

    let (_, img) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images/{}", r1[0]["id"].as_str().unwrap()),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(img["category_id"], json!(cat.to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn batch_move_rejects_bad_input() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "bmovebad").await;
    let cat = create_category(&app, &token, "emptybucket").await;
    let (status, _) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/batch-move",
        Some(&token),
        &json!({"image_ids": [], "category_id": cat}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let too_many: Vec<Value> = (0..101)
        .map(|_| json!(Uuid::new_v4().to_string()))
        .collect();
    let (status, _) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/batch-move",
        Some(&token),
        &json!({"image_ids": too_many, "category_id": cat}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ── GET /api/v1/images/{id}/links ───────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_image_links_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "linksok").await;
    let results = upload_png(&app, &token, "link.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap().to_string();
    let (status, resp) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images/{id}/links"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert!(resp["url"].as_str().unwrap().contains("/u/"));
    assert!(resp["markdown"]
        .as_str()
        .unwrap()
        .starts_with("![link.png]("));
    assert!(resp["html"].as_str().unwrap().starts_with("<img src="));
    assert!(resp["bbcode"].as_str().unwrap().starts_with("[img]"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_image_links_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "links404").await;
    let id = Uuid::new_v4().to_string();
    let (status, _) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images/{id}/links"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── Public routes ───────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_get_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "pubok").await;
    let results = upload_png(&app, &token, "pub.png", &tiny_png()).await;
    let pk = results[0]["public_key"].as_str().unwrap();
    let (status, headers, raw) = send_raw(
        &app,
        Method::GET,
        &format!("/u/{pk}"),
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "image/png");
    assert_eq!(
        headers.get("cache-control").unwrap(),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(raw, tiny_png());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_get_not_found() {
    let app = test_app().await;
    let (status, _, _) = send_raw(&app, Method::GET, "/u/zzzzzz", None, None, Vec::new()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_get_hides_non_active() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "pubhide").await;
    let results = upload_png(&app, &token, "hidden.png", &tiny_png()).await;
    let id = Uuid::parse_str(results[0]["id"].as_str().unwrap()).unwrap();
    let pk = results[0]["public_key"].as_str().unwrap().to_string();
    sqlx::query("UPDATE images SET status = 'pending' WHERE id = $1")
        .bind(id)
        .execute(app.pool())
        .await
        .unwrap();
    let (status, _, _) = send_raw(
        &app,
        Method::GET,
        &format!("/u/{pk}"),
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_thumb_not_generated() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "pubthumb").await;
    let results = upload_png(&app, &token, "thumb.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap();
    let (status, _, _) = send_raw(
        &app,
        Method::GET,
        &format!("/u/thumb/{id}"),
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_webp_not_generated() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "pubwebp").await;
    let results = upload_png(&app, &token, "webp.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap();
    let (status, _, _) = send_raw(
        &app,
        Method::GET,
        &format!("/u/webp/{id}"),
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_thumb_alias_not_generated() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "pubalias").await;
    let results = upload_png(&app, &token, "alias.png", &tiny_png()).await;
    let pk = results[0]["public_key"].as_str().unwrap();
    let (status, _, _) = send_raw(
        &app,
        Method::GET,
        &format!("/t/{pk}"),
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── Localized error codes ───────────────────────────────────────────────────

async fn send_json_auth(
    app: &TestApp,
    method: Method,
    uri: &str,
    user: &(String, String, Uuid),
) -> (StatusCode, Value) {
    send_json(app, method, uri, Some(&user.1), &Value::Null).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_get_missing_image_returns_code() {
    let app = test_app().await;
    let user = create_user(&app, "code404").await;
    let resp = send_json_auth(
        &app,
        Method::GET,
        "/api/v1/images/00000000-0000-0000-0000-000000000000",
        &user,
    )
    .await;
    assert_eq!(resp.0, StatusCode::NOT_FOUND);
    assert_eq!(resp.1["code"], "image.not_found");
}
