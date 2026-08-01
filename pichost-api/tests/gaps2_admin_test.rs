//! Integration tests closing remaining coverage gaps in admin / users /
//! storage_configs routes. Requires PostgreSQL + Redis (see tests/common/mod.rs).

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::Value;
use serial_test::serial;
use uuid::Uuid;

fn uname(tag: &str) -> String {
    format!("{}_{}", tag, &Uuid::new_v4().simple().to_string()[..8])
}

async fn create_admin(app: &TestApp, tag: &str) -> (String, Uuid) {
    let username = uname(&format!("admin_{tag}"));
    let code = create_invite(app, 3600).await;
    let body = serde_json::json!({
        "username": username,
        "password": "admin123456",
        "invite_code": code,
    });
    let (status, resp) = send_json(app, Method::POST, "/api/v1/auth/register", None, &body).await;
    assert!(status.is_success(), "admin register failed: {status} {resp}");
    let user_id = Uuid::parse_str(resp["user"]["id"].as_str().unwrap()).unwrap();
    make_admin(app, user_id).await;
    let (status, resp) = login(app, &username, "admin123456").await;
    assert!(status.is_success(), "admin login failed: {status} {resp}");
    let token = resp["access_token"].as_str().unwrap().to_string();
    (token, user_id)
}

async fn insert_image(app: &TestApp, user_id: Uuid, backend: &str) -> Uuid {
    let pkey = Uuid::new_v4().simple().to_string();
    let key = format!("users/{}/img-{}.png", user_id, &pkey[..8]);
    let sha = format!("{}{}", pkey, pkey);
    sqlx::query_scalar(
        "INSERT INTO images (user_id, public_key, original_name, storage_key, \
         storage_backend, mime_type, file_size, width, height, sha256, url, status, \
         thumbnail_key, webp_key) \
         VALUES ($1, $2, 'test.png', $3, $4, 'image/png', 100, 1, 1, $5, $6, 'active', $7, $8) \
         RETURNING id",
    )
    .bind(user_id)
    .bind(&pkey[..16])
    .bind(&key)
    .bind(backend)
    .bind(&sha)
    .bind(format!("http://localhost:3000/u/{}", &pkey[..6]))
    .bind(format!("{key}.thumb.png"))
    .bind(format!("{key}.webp"))
    .fetch_one(app.pool())
    .await
    .expect("insert image")
}

async fn del_redis_key(app: &TestApp, key: &str) {
    let mut conn = app.state.cache.get_pool().get().await.unwrap();
    let _: () = deadpool_redis::redis::cmd("DEL")
        .arg(key)
        .query_async::<_, ()>(&mut *conn)
        .await
        .unwrap();
}

const NIL_KEY: &str = "pichost:stats:00000000-0000-0000-0000-000000000000";

fn cleanup_config_files() {
    let dir = std::env::current_dir().unwrap_or_default();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "config.toml"
                || (name.starts_with("config.toml.") && name.ends_with(".bak"))
            {
                let _ = std::fs::remove_file(dir.join(&name));
            }
        }
    }
}

// ── admin.rs gaps ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_stats_db_path_then_cache() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "statsdb2").await;
    del_redis_key(&app, NIL_KEY).await;
    let u1 = insert_user_direct(&app, &uname("quota_a"), "user123456", false).await;
    sqlx::query("UPDATE users SET storage_quota = 12345 WHERE id = $1")
        .bind(u1)
        .execute(app.pool())
        .await
        .unwrap();
    let _ = insert_image(&app, u1, "local").await;
    let (s1, r1) = send_json(
        &app,
        Method::GET,
        "/api/v1/admin/stats",
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(s1, StatusCode::OK, "first stats failed: {r1}");
    assert!(r1["total_users"].as_i64().unwrap() >= 1);
    assert!(r1["total_images"].as_i64().unwrap() >= 1);
    let (s2, r2) = send_json(
        &app,
        Method::GET,
        "/api/v1/admin/stats",
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(s2, StatusCode::OK, "cached stats failed: {r2}");
    assert_eq!(r1["total_users"], r2["total_users"]);
    assert_eq!(r1["total_images"], r2["total_images"]);
    assert_eq!(r1["total_size"], r2["total_size"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_delete_user_removes_storage_files() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "realfiles").await;
    let (_, user_token, uid) = create_user(&app, "realfilesuser").await;
    let (ct, body) = multipart_image("tiny.png", &tiny_png(), &[]);
    let (status, _, _) = send_raw(
        &app,
        Method::POST,
        "/api/v1/images",
        Some(&user_token),
        Some(&ct),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "upload failed: {status}");
    let (storage_key, thumb, webp): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT storage_key, thumbnail_key, webp_key FROM images WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(app.pool())
    .await
    .expect("image row");
    let base = app.state.config.storage.local_base_path.clone();
    for k in [Some(storage_key.as_str()), thumb.as_deref(), webp.as_deref()]
        .into_iter()
        .flatten()
    {
        assert!(base.join(k).exists(), "file missing before delete: {k}");
    }
    let uri = format!("/api/v1/admin/users/{uid}");
    let (status, resp) =
        send_json(&app, Method::DELETE, &uri, Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "delete failed: {resp}");
    for k in [Some(storage_key.as_str()), thumb.as_deref(), webp.as_deref()]
        .into_iter()
        .flatten()
    {
        assert!(!base.join(k).exists(), "file not removed: {k}");
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(uid)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert!(!exists);
    let images: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM images WHERE user_id = $1")
        .bind(uid)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(images, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_list_users_clamps_params() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "clampadmin").await;
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/admin/users?offset=-5&limit=9999",
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "clamped list failed: {resp}");
    assert!(resp["total"].as_i64().unwrap() >= 1);
    assert!(resp["users"].as_array().unwrap().len() <= 200);
}

// ── Config endpoints (serial: touches config.toml in test CWD) ─────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_config_restore_missing_backup_error() {
    cleanup_config_files();
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "restoreerr").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/admin/config/restore",
        Some(&admin_token),
        &serde_json::json!({"backup_file": "config.toml.nonexistent.bak"}),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "got {status}: {resp}");
    cleanup_config_files();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_config_backup_materializes_when_missing() {
    cleanup_config_files();
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "materialize").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/admin/config/backup",
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "backup failed: {resp}");
    let filename = resp["filename"].as_str().unwrap();
    assert!(filename.starts_with("config.toml.") && filename.ends_with(".bak"));
    cleanup_config_files();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_config_test_bad_urls_reports_fail() {
    cleanup_config_files();
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "badurls").await;
    let body = serde_json::json!({
        "database_url": "not-a-valid-url",
        "redis_url": "not-a-valid-url",
    });
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/admin/config/test",
        Some(&admin_token),
        &body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "test failed: {resp}");
    assert!(resp["database"].as_str().unwrap().starts_with("fail:"));
    assert!(resp["redis"].as_str().unwrap().starts_with("fail:"));
    cleanup_config_files();
}

// ── users.rs gaps ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn users_me_stats_cache_prepopulated() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "statsprepop").await;
    app.state
        .cache
        .set_user_stats(&uid, &[("total_images", Some(5)), ("total_size", Some(99))])
        .await
        .unwrap();
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/users/me/stats",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "stats failed: {resp}");
    assert_eq!(resp["total_images"].as_i64().unwrap(), 5);
    assert_eq!(resp["total_size"].as_i64().unwrap(), 99);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn users_me_profile_update_all_fields() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "allfields").await;
    let new_name = uname("renamed");
    let new_email = format!("{}@example.com", new_name);
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        "/api/v1/users/me",
        Some(&token),
        &serde_json::json!({
            "username": new_name,
            "email": new_email,
            "storage_backend": "local",
            "watermark_config": {"enabled": true, "text": "@all"},
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update failed: {resp}");
    assert_eq!(resp["username"].as_str().unwrap(), new_name);
    assert_eq!(resp["email"].as_str().unwrap(), new_email);
    assert_eq!(resp["storage_backend"].as_str().unwrap(), "local");
    assert!(resp["watermark_config"]["enabled"].as_bool().unwrap());
    assert_eq!(resp["watermark_config"]["text"].as_str().unwrap(), "@all");
}
