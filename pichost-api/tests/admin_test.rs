mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::Value;
use serial_test::serial;
use uuid::Uuid;

async fn create_admin(app: &TestApp, tag: &str) -> (String, Uuid) {
    let username = format!(
        "admin_{}_{}",
        tag,
        &Uuid::new_v4().simple().to_string()[..8]
    );
    let password = "admin123456";
    let code = create_invite(app, 3600).await;
    let body = serde_json::json!({
        "username": username,
        "password": password,
        "invite_code": code,
    });
    let (status, resp) =
        send_json(app, Method::POST, "/api/v1/auth/register", None, &body).await;
    assert!(status.is_success(), "admin register failed: {status} {resp}");
    let user_id = Uuid::parse_str(resp["user"]["id"].as_str().unwrap()).unwrap();
    make_admin(app, user_id).await;
    let (status, resp) = login(app, &username, password).await;
    assert!(status.is_success(), "admin login failed: {status} {resp}");
    let token = resp["access_token"].as_str().unwrap().to_string();
    (token, user_id)
}

// ── User management ───────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_users_non_admin_forbidden() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "forbidlist").await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/admin/users", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(resp["error"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_users_lists_users_with_total() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "listusers").await;
    let (username, _, _) = create_user(&app, "listed").await;

    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/admin/users", Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "list failed: {resp}");
    let users = resp["users"].as_array().unwrap();
    assert!(!users.is_empty());
    assert!(resp["total"].as_i64().unwrap() >= 2);
    assert!(
        users.iter().any(|u| u["username"].as_str() == Some(username.as_str())),
        "created user missing from list: {resp}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_update_user_fields() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "updadmin").await;
    let (_, _, target_id) = create_user(&app, "updtarget").await;

    let new_name = format!("renamed_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let body = serde_json::json!({
        "username": new_name,
        "is_admin": false,
        "storage_quota": 12345,
    });
    let uri = format!("/api/v1/admin/users/{target_id}");
    let (status, resp) =
        send_json(&app, Method::PATCH, &uri, Some(&admin_token), &body).await;
    assert_eq!(status, StatusCode::OK, "update failed: {resp}");
    assert_eq!(resp["username"].as_str().unwrap(), new_name);
    assert!(!resp["is_admin"].as_bool().unwrap());
    assert_eq!(resp["storage_quota"].as_i64().unwrap(), 12345);

    let row: (String, bool, Option<i64>) = sqlx::query_as(
        "SELECT username, is_admin, storage_quota FROM users WHERE id = $1",
    )
    .bind(target_id)
    .fetch_one(app.pool())
    .await
    .expect("fetch updated user");
    assert_eq!(row.0, new_name);
    assert!(!row.1);
    assert_eq!(row.2, Some(12345));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_update_cannot_demote_self() {
    let app = test_app().await;
    let (admin_token, admin_id) = create_admin(&app, "selfdemo").await;
    let uri = format!("/api/v1/admin/users/{admin_id}");
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&admin_token),
        &serde_json::json!({"is_admin": false}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("demote"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_update_non_admin_forbidden() {
    let app = test_app().await;
    let (_, admin_id) = create_admin(&app, "forbidupd").await;
    let (_, user_token, _) = create_user(&app, "forbidupduser").await;
    let uri = format!("/api/v1/admin/users/{admin_id}");
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&user_token),
        &serde_json::json!({"username": "hacked"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(resp["error"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_update_user_not_found() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "notfound").await;
    let uri = format!("/api/v1/admin/users/{}", Uuid::new_v4());
    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &uri,
        Some(&admin_token),
        &serde_json::json!({"username": "ghost"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(resp["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_cannot_delete_self() {
    let app = test_app().await;
    let (admin_token, admin_id) = create_admin(&app, "selfdel").await;
    let uri = format!("/api/v1/admin/users/{admin_id}");
    let (status, resp) =
        send_json(&app, Method::DELETE, &uri, Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("delete"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_delete_non_admin_forbidden() {
    let app = test_app().await;
    let (_, admin_id) = create_admin(&app, "forbiddel").await;
    let (_, user_token, _) = create_user(&app, "forbiddeluser").await;
    let uri = format!("/api/v1/admin/users/{admin_id}");
    let (status, resp) =
        send_json(&app, Method::DELETE, &uri, Some(&user_token), &Value::Null).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(resp["error"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_delete_another_user() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "deladmin").await;
    let (_, _, target_id) = create_user(&app, "deleted").await;

    let uri = format!("/api/v1/admin/users/{target_id}");
    let (status, resp) =
        send_json(&app, Method::DELETE, &uri, Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "delete failed: {resp}");

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(target_id)
            .fetch_one(app.pool())
            .await
            .expect("check deleted user");
    assert!(!exists);
}

// ── Stats ─────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_stats_returns_totals() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "statsadmin").await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/admin/stats", Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "stats failed: {resp}");
    assert!(resp["total_users"].as_i64().unwrap() >= 1);
    assert!(resp["total_images"].as_i64().unwrap() >= 0);
    assert!(resp["total_size"].as_i64().unwrap() >= 0);
    assert!(resp["total_quota"].is_null() || resp["total_quota"].as_i64().is_some());
    assert!(resp["storage_backends"]["local"]["total_images"].as_i64().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_stats_non_admin_forbidden() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "forbidstats").await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/admin/stats", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(resp["error"].as_str().is_some());
}

// ── Invites ───────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_create_invite_returns_code() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "invadmin").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/admin/invites",
        Some(&admin_token),
        &serde_json::json!({"ttl_days": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create invite failed: {resp}");
    let code = resp["code"].as_str().unwrap();
    assert_eq!(code.len(), 32);
    assert!(resp["expires_at"].as_i64().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_create_invite_non_admin_forbidden() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "forbidinv").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/admin/invites",
        Some(&token),
        &serde_json::json!({"ttl_days": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(resp["error"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_list_invites_contains_created() {
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "listinv").await;
    let (status, created) = send_json(
        &app,
        Method::POST,
        "/api/v1/admin/invites",
        Some(&admin_token),
        &serde_json::json!({"ttl_days": 2}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let code = created["code"].as_str().unwrap().to_string();

    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/admin/invites", Some(&admin_token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "list invites failed: {resp}");
    let codes = resp.as_array().unwrap();
    assert!(
        codes.iter().any(|c| c["code"].as_str() == Some(code.as_str())),
        "created code missing from list: {resp}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn admin_list_invites_non_admin_forbidden() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "forbidlistinv").await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/admin/invites", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(resp["error"].as_str().is_some());
}

// ── Config management (serial: writes config.toml in test CWD) ────────

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

fn test_redis_url() -> String {
    std::env::var("PICHOST_REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_config_non_admin_forbidden() {
    cleanup_config_files();
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "forbidcfg").await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/admin/config", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(resp["error"].as_str().is_some());
    cleanup_config_files();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_config_get_masks_secret() {
    cleanup_config_files();
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "getcfg").await;
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/admin/config",
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get config failed: {resp}");
    assert_eq!(resp["jwt_secret"].as_str().unwrap(), "********");
    assert!(resp["token_encryption_key"].is_string());
    assert!(resp["config_path"].as_str().unwrap().ends_with("config.toml"));
    cleanup_config_files();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_config_put_roundtrip() {
    cleanup_config_files();
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "putcfg").await;
    let body = serde_json::json!({"database_url": test_db_url()});
    let (status, resp) = send_json(
        &app,
        Method::PUT,
        "/api/v1/admin/config",
        Some(&admin_token),
        &body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "put config failed: {resp}");
    assert!(
        resp["database_url"].as_str().unwrap().contains(":***@"),
        "database_url not masked: {resp}"
    );
    assert_eq!(resp["jwt_secret"].as_str().unwrap(), "********");
    assert!(std::path::Path::new("config.toml").exists());
    cleanup_config_files();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_config_test_connections() {
    cleanup_config_files();
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "testcfg").await;
    let body = serde_json::json!({
        "database_url": test_db_url(),
        "redis_url": test_redis_url(),
    });
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/admin/config/test",
        Some(&admin_token),
        &body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "test config failed: {resp}");
    assert_eq!(resp["database"].as_str().unwrap(), "ok");
    assert_eq!(resp["redis"].as_str().unwrap(), "ok");
    cleanup_config_files();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
#[serial]
async fn admin_config_backup_list_restore() {
    cleanup_config_files();
    let app = test_app().await;
    let (admin_token, _) = create_admin(&app, "backupcfg").await;

    let (status, _) = send_json(
        &app,
        Method::PUT,
        "/api/v1/admin/config",
        Some(&admin_token),
        &serde_json::json!({"database_url": test_db_url()}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "put config failed");

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/admin/config/backup",
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "backup failed: {resp}");
    let filename = resp["filename"].as_str().unwrap().to_string();
    assert!(filename.starts_with("config.toml.") && filename.ends_with(".bak"));

    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/admin/config/backups",
        Some(&admin_token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list backups failed: {resp}");
    let backups = resp.as_array().unwrap();
    assert!(
        backups.iter().any(|b| b["filename"].as_str() == Some(filename.as_str())),
        "backup missing from list: {resp}"
    );

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/admin/config/restore",
        Some(&admin_token),
        &serde_json::json!({"backup_file": filename}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "restore failed: {resp}");
    assert_eq!(resp["status"].as_str().unwrap(), "restored");
    assert_eq!(resp["from"].as_str().unwrap(), filename);
    cleanup_config_files();
}

// ── OAuth error paths (no OAuth credentials in test config) ───────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_github_redirect_unconfigured() {
    let app = test_app().await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/auth/oauth/github", None, &Value::Null).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("client_id not configured"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_google_redirect_unconfigured() {
    let app = test_app().await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/auth/oauth/google", None, &Value::Null).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("client_id not configured"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_github_callback_unconfigured() {
    let app = test_app().await;
    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/auth/oauth/github/callback?code=somecode&state=somestate",
        None,
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("client_id not configured"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_link_unconfigured() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "oaulink").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/oauth/link",
        Some(&token),
        &serde_json::json!({"provider": "github", "code": "faketoken"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("client_id not configured"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn oauth_link_unknown_provider() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "oaunk").await;
    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/oauth/link",
        Some(&token),
        &serde_json::json!({"provider": "gitlab", "code": "faketoken"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(resp["error"].as_str().unwrap().contains("unknown provider"));
}
