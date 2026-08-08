//! Integration tests filling coverage gaps in admin / users / storage_configs /
//! auth routes. Requires PostgreSQL + Redis (see tests/common/mod.rs).

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

async fn insert_config(app: &TestApp, user_id: Uuid, name: &str, provider: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO user_storage_configs (user_id, name, provider, config) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user_id)
    .bind(name)
    .bind(provider)
    .bind(serde_json::json!({
        "token_encrypted": "enc",
        "repo": "owner/repo",
        "branch": "main",
    }))
    .fetch_one(app.pool())
    .await
    .expect("insert storage config")
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

// ── auth.rs gaps ───────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn auth_invite_code_used_rejected() {
    let app = test_app().await;
    let code = create_invite(&app, 3600).await;
    let body = serde_json::json!({
        "username": uname("used1"),
        "password": "user123456",
        "invite_code": code,
    });
    let (status, resp) = send_json(&app, Method::POST, "/api/v1/auth/register", None, &body).await;
    assert_eq!(status, StatusCode::CREATED, "first register failed: {status} {resp}");
    let body2 = serde_json::json!({
        "username": uname("used2"),
        "password": "user123456",
        "invite_code": code,
    });
    let (status, resp) = send_json(&app, Method::POST, "/api/v1/auth/register", None, &body2).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].as_str().unwrap().contains("already been used"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn auth_invite_code_expired_rejected() {
    let app = test_app().await;
    let code = create_invite(&app, 60).await;
    let mut conn = app.state.cache.get_pool().get().await.unwrap();
    let _: () = deadpool_redis::redis::cmd("HSET")
        .arg(format!("pichost:invite:{code}"))
        .arg("expires_at")
        .arg(1)
        .query_async::<_, ()>(&mut *conn)
        .await
        .unwrap();
    let body = serde_json::json!({
        "username": uname("expired"),
        "password": "user123456",
        "invite_code": code,
    });
    let (status, resp) = send_json(&app, Method::POST, "/api/v1/auth/register", None, &body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].as_str().unwrap().contains("expired"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn auth_refresh_deleted_user_unauthorized() {
    let app = test_app().await;
    let (username, _, uid) = create_user(&app, "refdel").await;
    let (status, resp) = login(&app, &username, "user123456").await;
    assert_eq!(status, StatusCode::OK);
    let refresh = resp["refresh_token"].as_str().unwrap().to_string();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(uid)
        .execute(app.pool())
        .await
        .unwrap();
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        &serde_json::json!({"refresh_token": refresh}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "expected 401, got {status}: {resp}");
    assert!(resp["error"].as_str().unwrap().contains("user not found"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn auth_logout_missing_header_unauthorized() {
    let app = test_app().await;
    let (status, resp) =
        send_json(&app, Method::POST, "/api/v1/auth/logout", None, &Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "expected 401, got {status}: {resp}");
    assert!(resp["error"].as_str().unwrap().contains("missing authorization header"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn auth_logout_invalid_token_unauthorized() {
    let app = test_app().await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        Some("garbage.token.here"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "expected 401, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn auth_logout_rejects_refresh_token() {
    let app = test_app().await;
    let (username, _, _) = create_user(&app, "logoutrej").await;
    let (status, resp) = login(&app, &username, "user123456").await;
    assert_eq!(status, StatusCode::OK);
    let refresh = resp["refresh_token"].as_str().unwrap().to_string();
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        Some(&refresh),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].as_str().unwrap().contains("only access tokens"));
}

// ── users.rs gaps ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn users_me_stats_served_from_cache() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "statscache").await;
    let (s1, r1) = send_json(&app, Method::GET, "/api/v1/users/me/stats", Some(&token), &Value::Null).await;
    assert_eq!(s1, StatusCode::OK);
    let (s2, r2) = send_json(&app, Method::GET, "/api/v1/users/me/stats", Some(&token), &Value::Null).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(r1["total_images"], r2["total_images"]);
    assert_eq!(r1["total_size"], r2["total_size"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn users_me_stats_db_path_with_images() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "statsdb").await;
    let (ct, body) = multipart_image("tiny.png", &tiny_png(), &[]);
    let (status, _, _) = send_raw(
        &app,
        Method::POST,
        "/api/v1/images",
        Some(&token),
        Some(&ct),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "upload failed: {status}");
    del_redis_key(&app, &format!("pichost:stats:{uid}")).await;
    let (status, resp) = send_json(&app, Method::GET, "/api/v1/users/me/stats", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "stats failed: {resp}");
    assert!(resp["total_images"].as_i64().unwrap() >= 1);
    assert!(resp["total_size"].as_i64().unwrap() > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn users_me_profile_not_found_after_delete() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "profdel").await;
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(uid)
        .execute(app.pool())
        .await
        .unwrap();
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/users/me", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn users_me_password_not_found_after_delete() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "pwdel").await;
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(uid)
        .execute(app.pool())
        .await
        .unwrap();
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/me/password",
        Some(&token),
        &serde_json::json!({"current_password": "user123456", "new_password": "newpass12345"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn users_me_email_conflict() {
    let app = test_app().await;
    let (_, token_a, _) = create_user(&app, "emailconf_a").await;
    let (uname_b, _, _) = create_user(&app, "emailconf_b").await;
    let b_email = format!("{}@example.com", uname_b);
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        "/api/v1/users/me",
        Some(&token_a),
        &serde_json::json!({"email": b_email}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "expected 409, got {status}: {resp}");
    assert!(resp["error"].as_str().unwrap().contains("email already taken"));
}

// ── storage_configs.rs gaps ────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_limit_reached() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "sc_limit").await;
    for i in 0..5 {
        insert_config(&app, uid, &format!("cfg{i}"), "github").await;
    }
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/me/storage-configs",
        Some(&token),
        &serde_json::json!({"name": "sixth", "provider": "github", "token": "t", "repo": "o/r"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert_eq!(resp["code"], "storage_config.limit");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_name_duplicate() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "sc_dup").await;
    insert_config(&app, uid, "dup", "github").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/me/storage-configs",
        Some(&token),
        &serde_json::json!({"name": "dup", "provider": "github", "token": "t", "repo": "o/r"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "expected 409, got {status}: {resp}");
    assert_eq!(resp["code"], "storage_config.name_exists");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_list_with_rows() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "sc_list").await;
    insert_config(&app, uid, "cfg1", "github").await;
    insert_config(&app, uid, "cfg2", "gitcode").await;
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/users/me/storage-configs",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list failed: {resp}");
    let arr = resp.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert!(arr.iter().any(|c| c["name"] == "cfg1"));
    assert!(arr.iter().any(|c| c["name"] == "cfg2"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_get_success() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "sc_get").await;
    let id = insert_config(&app, uid, "cfg1", "github").await;
    let uri = format!("/api/v1/users/me/storage-configs/{id}");
    let (status, resp) = send_json(&app, Method::GET, &uri, Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "get failed: {resp}");
    assert_eq!(resp["name"].as_str().unwrap(), "cfg1");
    assert_eq!(resp["provider"].as_str().unwrap(), "github");
    assert_eq!(resp["repo"].as_str().unwrap(), "owner/repo");
    assert_eq!(resp["branch"].as_str().unwrap(), "main");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_update_success() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "sc_patch").await;
    let id = insert_config(&app, uid, "cfg1", "github").await;
    let uri = format!("/api/v1/users/me/storage-configs/{id}");
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&token),
        &serde_json::json!({"name": "renamed", "repo": "other/repo", "branch": "dev"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update failed: {resp}");
    assert_eq!(resp["name"].as_str().unwrap(), "renamed");
    assert_eq!(resp["repo"].as_str().unwrap(), "other/repo");
    assert_eq!(resp["branch"].as_str().unwrap(), "dev");
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&token),
        &serde_json::json!({"token": "ghp_token_abcdefgh123456"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "token update failed: {resp}");
    assert_ne!(resp["token_masked"].as_str().unwrap(), "****");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_delete_success() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "sc_del").await;
    let id = insert_config(&app, uid, "cfg1", "github").await;
    let uri = format!("/api/v1/users/me/storage-configs/{id}");
    let (status, _) = send_json(&app, Method::DELETE, &uri, Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send_json(&app, Method::GET, &uri, Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_delete_referenced_image_rejected() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "sc_refdel").await;
    let id = insert_config(&app, uid, "cfg1", "github").await;
    let pkey = Uuid::new_v4().simple().to_string();
    sqlx::query(
        "INSERT INTO images (user_id, public_key, original_name, storage_key, \
         storage_backend, mime_type, file_size, sha256, url, status, storage_config_id) \
         VALUES ($1, $2, 'ref.png', $3, 'local', 'image/png', 100, $4, $5, 'active', $6)",
    )
    .bind(uid)
    .bind(&pkey[..16])
    .bind(format!("users/{uid}/ref.png"))
    .bind("a".repeat(64))
    .bind(format!("http://localhost:3000/u/{}", &pkey[..6]))
    .bind(id)
    .execute(app.pool())
    .await
    .expect("insert referencing image");
    let uri = format!("/api/v1/users/me/storage-configs/{id}");
    let (status, resp) = send_json(&app, Method::DELETE, &uri, Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::CONFLICT, "expected 409, got {status}: {resp}");
    assert_eq!(resp["code"], "storage_config.in_use");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_set_default_unsets_others() {
    let app = test_app().await;
    let (_, token, uid) = create_user(&app, "sc_default").await;
    let a = insert_config(&app, uid, "a", "github").await;
    let b = insert_config(&app, uid, "b", "github").await;
    let uri_a = format!("/api/v1/users/me/storage-configs/{a}/default");
    let (status, resp) = send_json(&app, Method::POST, &uri_a, Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "set default failed: {resp}");
    assert!(resp["is_default"].as_bool().unwrap());
    let uri_b = format!("/api/v1/users/me/storage-configs/{b}/default");
    let (status, resp) = send_json(&app, Method::POST, &uri_b, Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert!(resp["is_default"].as_bool().unwrap());
    let uri_get = format!("/api/v1/users/me/storage-configs/{a}");
    let (status, resp) = send_json(&app, Method::GET, &uri_get, Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!resp["is_default"].as_bool().unwrap());
}

// ── admin.rs gaps ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_update_user_password() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "pwadmin").await;
    let (target_name, _, target_id) = create_user(&app, "pwtarget").await;
    let uri = format!("/api/v1/admin/users/{target_id}");
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&admin_token),
        &serde_json::json!({"password": "brandnewpass99"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update failed: {resp}");
    let (status, _) = login(&app, &target_name, "brandnewpass99").await;
    assert_eq!(status, StatusCode::OK, "login with admin-set password failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_update_user_short_password_rejected() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "pwshortadmin").await;
    let (_, _, target_id) = create_user(&app, "pwshorttarget").await;
    let uri = format!("/api/v1/admin/users/{target_id}");
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&admin_token),
        &serde_json::json!({"password": "short"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].as_str().unwrap().contains("at least 8"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_update_user_quota_zero_clears() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "quotaadmin").await;
    let (_, _, target_id) = create_user(&app, "quotatarget").await;
    let uri = format!("/api/v1/admin/users/{target_id}");
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&admin_token),
        &serde_json::json!({"storage_quota": 0}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update failed: {resp}");
    assert!(resp["storage_quota"].is_null());
    let quota: Option<i64> = sqlx::query_scalar("SELECT storage_quota FROM users WHERE id = $1")
        .bind(target_id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert!(quota.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_update_user_watermark_null_clears() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "wmadmin").await;
    let (_, _, target_id) = create_user(&app, "wmtarget").await;
    let uri = format!("/api/v1/admin/users/{target_id}");
    let (status, _) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&admin_token),
        &serde_json::json!({"watermark_config": {"enabled": true, "text": "@x"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "watermark set failed");
    let wm: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT watermark_config FROM users WHERE id = $1")
            .bind(target_id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert!(wm.is_some());
    let (status, _) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&admin_token),
        &serde_json::json!({"watermark_config": null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "watermark clear failed");
    let wm: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT watermark_config FROM users WHERE id = $1")
            .bind(target_id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert!(wm.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_delete_user_with_images_cascades() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "cascadeadmin").await;
    let (_, _, target_id) = create_user(&app, "cascadetarget").await;
    let _ = insert_image(&app, target_id, "local").await;
    let _ = insert_image(&app, target_id, "local").await;
    let uri = format!("/api/v1/admin/users/{target_id}");
    let (status, resp) = send_json(&app, Method::DELETE, &uri, Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "delete failed: {resp}");
    let images: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM images WHERE user_id = $1")
        .bind(target_id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(images, 0, "images did not cascade");
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(target_id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert!(!exists);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_delete_user_not_found() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "delmissing").await;
    let uri = format!("/api/v1/admin/users/{}", Uuid::new_v4());
    let (status, resp) = send_json(&app, Method::DELETE, &uri, Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_list_users_pagination() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "pageadmin").await;
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/admin/users?offset=0&limit=1",
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "paged list failed: {resp}");
    assert_eq!(resp["users"].as_array().unwrap().len(), 1);
    assert!(resp["total"].as_i64().unwrap() >= 1);
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/admin/users?offset=999999&limit=5",
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(resp["users"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_stats_reflects_data() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "statsdata").await;
    del_redis_key(&app, "pichost:stats:00000000-0000-0000-0000-000000000000").await;
    let u1 = insert_user_direct(&app, &uname("quota1"), "user123456", false).await;
    let u2 = insert_user_direct(&app, &uname("quota2"), "user123456", false).await;
    sqlx::query("UPDATE users SET storage_quota = 5000 WHERE id = $1")
        .bind(u1)
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE users SET storage_quota = 7000 WHERE id = $1")
        .bind(u2)
        .execute(app.pool())
        .await
        .unwrap();
    let _ = insert_image(&app, u1, "local").await;
    let _ = insert_image(&app, u1, "local").await;
    let _ = insert_image(&app, u2, "rustfs").await;
    let (status, resp) = send_json(&app, Method::GET, "/api/v1/admin/stats", Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "stats failed: {resp}");
    assert!(resp["total_users"].as_i64().unwrap() >= 3);
    assert!(resp["total_images"].as_i64().unwrap() >= 3);
    assert!(resp["total_size"].as_i64().unwrap() >= 300);
    assert!(resp["total_quota"].as_i64().unwrap() >= 12000);
    assert!(resp["storage_backends"]["local"]["total_images"].as_i64().unwrap() >= 2);
    assert!(resp["storage_backends"]["rustfs"]["total_images"].as_i64().unwrap() >= 1);
    assert!(resp["storage_backends"]["local"]["total_size"].as_i64().unwrap() >= 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_stats_cached_prepopulated() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "statscache").await;
    let fields: [(&str, Option<i64>); 8] = [
        ("total_users", Some(42)),
        ("total_images", Some(7)),
        ("total_size", Some(99)),
        ("active_users_24h", Some(1)),
        ("total_quota", Some(500)),
        ("local_images", Some(3)),
        ("local_size", Some(50)),
        ("rustfs_size", Some(49)),
    ];
    app.state.cache.set_user_stats(&Uuid::nil(), &fields).await.unwrap();
    let (status, resp) = send_json(&app, Method::GET, "/api/v1/admin/stats", Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "stats failed: {resp}");
    assert_eq!(resp["total_users"].as_i64().unwrap(), 42);
    assert_eq!(resp["total_images"].as_i64().unwrap(), 7);
    assert_eq!(resp["total_size"].as_i64().unwrap(), 99);
    assert_eq!(resp["total_quota"].as_i64().unwrap(), 500);
    assert_eq!(resp["storage_backends"]["local"]["total_images"].as_i64().unwrap(), 3);
    assert_eq!(resp["storage_backends"]["rustfs"]["total_size"].as_i64().unwrap(), 49);
}

// ── Config endpoints (serial: writes config.toml in test CWD) ──────────

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

fn test_db_url() -> String {
    std::env::var("PICHOST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pichost:pichost@localhost:5432/pichost".to_string())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_config_put_merge_preserves_existing() {
    cleanup_config_files();
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "mergecfg").await;
    let (status, _) = send_json(
        &app,
        Method::PUT,
        "/api/v1/admin/config",
        Some(&admin_token),
        &serde_json::json!({"database_url": test_db_url()}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "first put failed");
    let (status, resp) = send_json(
        &app,
        Method::PUT,
        "/api/v1/admin/config",
        Some(&admin_token),
        &serde_json::json!({"public_url": "https://merge.example.com"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "merge put failed: {resp}");
    assert!(resp["database_url"].as_str().unwrap().contains(":***@"));
    assert_eq!(resp["public_url"].as_str().unwrap(), "https://merge.example.com");
    cleanup_config_files();
}
