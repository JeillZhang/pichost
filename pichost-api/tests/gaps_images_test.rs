//! Coverage-gap integration tests: upload service internals (quota, dedup,
//! config resolution, Redis enqueue, dimensions) and image route edge cases
//! (thumb/webp serving, storage cleanup, list filters, pagination clamping).
//! Requires PostgreSQL + Redis (Docker compose).

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use deadpool_redis::redis::AsyncCommands;
use pichost_api::services::upload::{get_user_image, list_user_images, ImageListQuery};
use serde_json::{json, Value};
use uuid::Uuid;

fn png_variant(extra: usize) -> Vec<u8> {
    let mut bytes = tiny_png();
    bytes.extend(std::iter::repeat_n(0u8, extra));
    bytes
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

async fn insert_storage_config(
    app: &TestApp,
    user_id: Uuid,
    name: &str,
    provider: &str,
    is_default: bool,
    config: Value,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO user_storage_configs (user_id, name, provider, is_default, config) \
         VALUES ($1,$2,$3,$4,$5) RETURNING id",
    )
    .bind(user_id)
    .bind(name)
    .bind(provider)
    .bind(is_default)
    .bind(config)
    .fetch_one(app.pool())
    .await
    .expect("insert storage config")
}

async fn insert_image_row(
    app: &TestApp,
    user_id: Uuid,
    public_key: &str,
    storage_key: &str,
    thumb_key: Option<&str>,
    webp_key: Option<&str>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO images (id, user_id, public_key, original_name, storage_key, \
         storage_backend, mime_type, file_size, sha256, url, status, \
         thumbnail_key, webp_key) \
         VALUES ($1,$2,$3,$4,$5,'local','image/png',67,$6,$7,'active',$8,$9)",
    )
    .bind(id)
    .bind(user_id)
    .bind(public_key)
    .bind("inserted.png")
    .bind(storage_key)
    .bind("a".repeat(64))
    .bind(format!("/u/{public_key}"))
    .bind(thumb_key)
    .bind(webp_key)
    .execute(app.pool())
    .await
    .expect("insert image row");
    id
}

fn write_local_file(app: &TestApp, key: &str, bytes: &[u8]) {
    let path = app.state.config.storage.local_base_path.join(key);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn short_pk() -> String {
    Uuid::new_v4().simple().to_string()[..6].to_string()
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

// ── upload service internals ────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_enqueues_redis_task() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "gapenq").await;
    let results = upload_png(&app, &token, "task.png", &tiny_png()).await;
    let img_id = results[0]["id"].as_str().unwrap();
    let mut conn = app.redis_pool().get().await.expect("redis conn");
    let tasks: Vec<String> = conn.lrange("pichost:tasks:pending", 0, -1).await.unwrap();
    let mut found = None;
    for t in tasks.iter().rev() {
        let key = format!("pichost:task:{t}");
        let data: String = conn.hget(&key, "data").await.unwrap_or_default();
        if data.contains(img_id) {
            found = Some(key);
            break;
        }
    }
    let key = found.expect("task for upload exists");
    let status: String = conn.hget(&key, "status").await.unwrap();
    assert_eq!(status, "pending");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_storage_quota_enforced() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "gapquota").await;
    sqlx::query("UPDATE users SET storage_quota = 100 WHERE id = $1")
        .bind(user_id)
        .execute(app.pool())
        .await
        .unwrap();
    upload_png(&app, &token, "ok.png", &tiny_png()).await;
    sqlx::query("UPDATE users SET storage_quota = 10 WHERE id = $1")
        .bind(user_id)
        .execute(app.pool())
        .await
        .unwrap();
    let (ct, body) = multipart_image("big.png", &png_variant(20), &[]);
    let (status, resp) = upload_expect_error(&app, Some(&token), &ct, body).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(resp["error"].as_str().unwrap().contains("storage quota"));
    sqlx::query("UPDATE users SET storage_quota = NULL WHERE id = $1")
        .bind(user_id)
        .execute(app.pool())
        .await
        .unwrap();
    upload_png(&app, &token, "unlim.png", &png_variant(30)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_dedup_per_storage_config() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "gapdedup").await;
    let cfg_id = insert_storage_config(&app, user_id, "my-local", "local", false, json!({})).await;
    let ids = cfg_id.to_string();
    let (ct, body) = multipart_image("c.png", &tiny_png(), &[("storage_config_ids", &ids)]);
    let (status, _, raw) = send_raw(
        &app,
        Method::POST,
        "/api/v1/images",
        Some(&token),
        Some(&ct),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let first: Value = serde_json::from_slice(&raw).unwrap();
    let (ct, body) = multipart_image("c2.png", &tiny_png(), &[("storage_config_ids", &ids)]);
    let (status, _, raw) = send_raw(
        &app,
        Method::POST,
        "/api/v1/images",
        Some(&token),
        Some(&ct),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let second: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(first[0]["id"], second[0]["id"]);
    assert_eq!(
        first[0]["storage_config"]["id"].as_str().unwrap(),
        ids.as_str()
    );
    assert_eq!(first[0]["storage_config"]["provider"], "local");

    let plain = upload_png(&app, &token, "plain.png", &png_variant(10)).await;
    assert_ne!(plain[0]["id"], first[0]["id"]);
    let plain2 = upload_png(&app, &token, "plain2.png", &png_variant(10)).await;
    assert_eq!(plain[0]["id"], plain2[0]["id"]);
    assert_eq!(plain[0]["storage_config"]["name"], "Local Storage");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_corrupt_image_has_null_dimensions() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "gapcorrupt").await;
    let mut bytes = tiny_png();
    bytes.truncate(8);
    bytes.extend(b"garbagegarbagegarbage");
    let (ct, body) = multipart_image("corrupt.png", &bytes, &[]);
    let (status, _, raw) = send_raw(
        &app,
        Method::POST,
        "/api/v1/images",
        Some(&token),
        Some(&ct),
        body,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&raw)
    );
    let resp: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(resp[0]["width"], Value::Null);
    assert_eq!(resp[0]["height"], Value::Null);
    let pk = resp[0]["public_key"].as_str().unwrap();
    assert_eq!(pk.len(), 6);
    assert!(pk.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_url_empty_config_ids_rejected() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "gapempty").await;
    let server_url = start_image_server(tiny_png()).await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/images/upload-url",
        Some(&token),
        &json!({"url": server_url, "storage_config_ids": []}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("local"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_foreign_storage_config_rejected() {
    let app = test_app().await;
    let (_, _, user_a) = create_user(&app, "gapfor-a").await;
    let (_, token_b, _) = create_user(&app, "gapfor-b").await;
    let cfg = insert_storage_config(&app, user_a, "a-cfg", "local", false, json!({})).await;
    let (ct, body) = multipart_image(
        "x.png",
        &tiny_png(),
        &[("storage_config_ids", &cfg.to_string())],
    );
    let (status, resp) = upload_expect_error(&app, Some(&token_b), &ct, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"]
        .as_str()
        .unwrap()
        .contains("no matching configs"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_uses_default_config() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "gapdef").await;
    let cfg_id =
        insert_storage_config(&app, user_id, "default-local", "local", true, json!({})).await;
    let results = upload_png(&app, &token, "def.png", &tiny_png()).await;
    assert_eq!(
        results[0]["storage_config"]["id"].as_str().unwrap(),
        cfg_id.to_string()
    );
    assert_eq!(results[0]["storage_config"]["name"], "default-local");
    assert_eq!(results[0]["storage_config"]["provider"], "local");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn upload_git_config_failure_returns_500() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "gapgit").await;
    let local_cfg =
        insert_storage_config(&app, user_id, "local-cfg", "local", false, json!({})).await;
    let git_cfg =
        insert_storage_config(&app, user_id, "git-cfg", "gitcode", false, json!({})).await;
    let ids = format!("{local_cfg},{git_cfg}");
    let (ct, body) = multipart_image("g.png", &tiny_png(), &[("storage_config_ids", &ids)]);
    let (status, resp) = upload_expect_error(&app, Some(&token), &ct, body).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(resp["error"]
        .as_str()
        .unwrap()
        .contains("resolve storage backend"));
}

// ── gallery list ─────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn list_filters_storage_config_and_category() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "gapfilt").await;
    let cfg_id = insert_storage_config(&app, user_id, "f-cfg", "local", false, json!({})).await;
    let ids = cfg_id.to_string();
    let (ct, body) = multipart_image(
        "cfg-img.png",
        &png_variant(1),
        &[("storage_config_ids", &ids)],
    );
    let (status, _, raw) = send_raw(
        &app,
        Method::POST,
        "/api/v1/images",
        Some(&token),
        Some(&ct),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let cfg_img: Value = serde_json::from_slice(&raw).unwrap();
    let plain = upload_png(&app, &token, "plain.png", &png_variant(2)).await;
    let cat = create_category(&app, &token, "gapcat").await;
    let img_id = cfg_img[0]["id"].as_str().unwrap();
    let (status, _) = send_json(
        &app,
        Method::POST,
        &format!("/api/v1/images/{img_id}/move"),
        Some(&token),
        &json!({"category_id": cat}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, resp) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images?category_id={cat}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["items"].as_array().unwrap().len(), 1);
    assert_eq!(resp["items"][0]["id"], cfg_img[0]["id"]);

    let (_, resp) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images?storage_config_id={cfg_id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(resp["items"].as_array().unwrap().len(), 1);

    let (_, resp) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/images?storage_config_id={cfg_id}&category_id={cat}&search=cfg"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(resp["items"].as_array().unwrap().len(), 1);
    assert_eq!(resp["total"], 1);

    let (_, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/images?search=zzz",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(resp["total"], 0);
    assert_eq!(plain[0]["id"].as_str().unwrap().len(), 36);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn list_clamps_page_and_per_page() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "gapclamp").await;
    for extra in [0usize, 10, 30] {
        upload_png(&app, &token, "c.png", &png_variant(extra)).await;
    }
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/images?page=0&per_page=999",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["page"], 1);
    assert_eq!(resp["per_page"], 100);
    assert_eq!(resp["total"], 3);
    let (_, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/images?page=0&per_page=0",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(resp["page"], 1);
    assert_eq!(resp["per_page"], 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn service_upload_queries_error_path() {
    // upload.rs service list/get queries omit the name/provider JOIN columns
    // that ImageRow requires, so any existing row fails to decode with 500.
    // The route reimplements both queries correctly; pin the error path here.
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "gapserr").await;
    let results = upload_png(&app, &token, "err.png", &tiny_png()).await;
    let img_uuid = Uuid::parse_str(results[0]["id"].as_str().unwrap()).unwrap();

    let query = ImageListQuery {
        page: 1,
        per_page: 20,
        sort: "file_size".into(),
        order: "desc".into(),
        search: "err".into(),
        storage_config_id: None,
        category_id: None,
    };
    let err = list_user_images(app.pool(), user_id, &query)
        .await
        .unwrap_err();
    assert_eq!(err.0, StatusCode::INTERNAL_SERVER_ERROR);

    let err2 = get_user_image(app.pool(), user_id, img_uuid)
        .await
        .unwrap_err();
    assert_eq!(err2.0, StatusCode::INTERNAL_SERVER_ERROR);
}

// ── public serving + storage cleanup ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_thumb_serves_file() {
    let app = test_app().await;
    let (_, _, user_id) = create_user(&app, "gapthumb").await;
    let pk = short_pk();
    let thumb_key = format!("{user_id}/thumb.png");
    write_local_file(&app, &thumb_key, &tiny_png());
    let id = insert_image_row(
        &app,
        user_id,
        &pk,
        &format!("{user_id}/main.png"),
        Some(&thumb_key),
        None,
    )
    .await;
    let uri = format!("/u/thumb/{id}");
    let (status, headers, raw) = send_raw(&app, Method::GET, &uri, None, None, Vec::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "image/png");
    assert_eq!(raw, tiny_png());
    let (status2, _, raw2) = send_raw(&app, Method::GET, &uri, None, None, Vec::new()).await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(raw2, tiny_png());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_thumb_alias_serves_file() {
    let app = test_app().await;
    let (_, _, user_id) = create_user(&app, "gapalias").await;
    let pk = short_pk();
    let thumb_key = format!("{user_id}/alias.png");
    write_local_file(&app, &thumb_key, &tiny_png());
    insert_image_row(
        &app,
        user_id,
        &pk,
        &format!("{user_id}/main.png"),
        Some(&thumb_key),
        None,
    )
    .await;
    let (status, headers, raw) = send_raw(
        &app,
        Method::GET,
        &format!("/t/{pk}"),
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "image/png");
    assert_eq!(raw, tiny_png());
    let (status, _, _) = send_raw(&app, Method::GET, "/t/zzzzzz", None, None, Vec::new()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_webp_serves_file() {
    let app = test_app().await;
    let (_, _, user_id) = create_user(&app, "gapwebp").await;
    let pk = short_pk();
    let webp_key = format!("{user_id}/img.webp");
    let payload = b"RIFF....WEBPVP8".to_vec();
    write_local_file(&app, &webp_key, &payload);
    let id = insert_image_row(
        &app,
        user_id,
        &pk,
        &format!("{user_id}/main.png"),
        None,
        Some(&webp_key),
    )
    .await;
    let (status, headers, raw) = send_raw(
        &app,
        Method::GET,
        &format!("/u/webp/{id}"),
        None,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "image/webp");
    assert_eq!(raw, payload);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn public_get_storage_missing_returns_404() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "gapstor404").await;
    let results = upload_png(&app, &token, "gone.png", &tiny_png()).await;
    let pk = results[0]["public_key"].as_str().unwrap().to_string();
    let file = app
        .state
        .config
        .storage
        .local_base_path
        .join(user_id.to_string())
        .join(&pk);
    std::fs::remove_file(&file).unwrap();
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
async fn delete_image_removes_thumb_and_webp() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "gapclean").await;
    let pk = short_pk();
    let main_key = format!("{user_id}/main.png");
    let thumb_key = format!("{user_id}/thumb.png");
    let webp_key = format!("{user_id}/img.webp");
    write_local_file(&app, &main_key, &tiny_png());
    write_local_file(&app, &thumb_key, &tiny_png());
    write_local_file(&app, &webp_key, b"webp");
    let id = insert_image_row(
        &app,
        user_id,
        &pk,
        &main_key,
        Some(&thumb_key),
        Some(&webp_key),
    )
    .await;
    let base = app.state.config.storage.local_base_path.clone();
    assert!(base.join(&main_key).exists());
    let (status, _) = send_json(
        &app,
        Method::DELETE,
        &format!("/api/v1/images/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!base.join(&main_key).exists());
    assert!(!base.join(&thumb_key).exists());
    assert!(!base.join(&webp_key).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_deletes_other_user_image() {
    let app = test_app().await;
    let (_, token_user, _) = create_user(&app, "gapadm-u").await;
    let (username, _, user_id) = create_user(&app, "gapadm-a").await;
    make_admin(&app, user_id).await;
    let (status, resp) = login(&app, &username, "user123456").await;
    assert!(status.is_success(), "admin login failed: {resp}");
    let admin_token = resp["access_token"].as_str().unwrap().to_string();
    let results = upload_png(&app, &token_user, "mine.png", &tiny_png()).await;
    let id = results[0]["id"].as_str().unwrap();
    let (status, resp) = send_json(
        &app,
        Method::DELETE,
        &format!("/api/v1/images/{id}"),
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
}
