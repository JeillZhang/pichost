mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::Value;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_my_profile_success() {
    let app = test_app().await;
    let (username, token, user_id) = create_user(&app, "profile").await;

    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/users/me", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "get profile failed: {resp}");
    assert_eq!(resp["id"].as_str().unwrap(), user_id.to_string());
    assert_eq!(resp["username"].as_str().unwrap(), username);
    assert!(resp["email"].is_string());
    assert!(!resp["is_admin"].as_bool().unwrap());
    assert!(resp["storage_quota"].is_number());
    assert_eq!(resp["storage_backend"].as_str().unwrap(), "local");
    assert!(resp["storage_prefix"].is_string());
    assert!(resp["created_at"].is_string());
    assert!(resp["updated_at"].is_string());
    assert!(resp["watermark_config"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_my_profile_unauthorized() {
    let app = test_app().await;
    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/users/me", None, &Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn patch_my_profile_username() {
    let app = test_app().await;
    let (_, token, user_id) = create_user(&app, "patchuname").await;
    let new_username = format!("renamed_{}", Uuid::new_v4().simple());

    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        "/api/v1/users/me",
        Some(&token),
        &serde_json::json!({"username": new_username}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "rename failed: {resp}");
    assert_eq!(resp["username"].as_str().unwrap(), new_username);
    assert_eq!(resp["id"].as_str().unwrap(), user_id.to_string());

    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/users/me", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["username"].as_str().unwrap(), new_username);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn patch_my_profile_email() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "patchemail").await;
    let new_email = format!("new{}@example.com", Uuid::new_v4().simple());

    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        "/api/v1/users/me",
        Some(&token),
        &serde_json::json!({"email": new_email}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "email update failed: {resp}");
    assert_eq!(resp["email"].as_str().unwrap(), new_email);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn patch_my_profile_watermark_set() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "wm_set").await;

    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        "/api/v1/users/me",
        Some(&token),
        &serde_json::json!({"watermark_config": {"enabled": true, "text": "@x"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "watermark set failed: {resp}");
    assert!(resp["watermark_config"]["enabled"].as_bool().unwrap());
    assert_eq!(resp["watermark_config"]["text"].as_str().unwrap(), "@x");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn patch_my_profile_watermark_clear_with_null() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "wm_clear").await;

    let (status, _) = send_json(
        &app,
        Method::PATCH,
        "/api/v1/users/me",
        Some(&token),
        &serde_json::json!({"watermark_config": {"enabled": true, "text": "@x"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        "/api/v1/users/me",
        Some(&token),
        &serde_json::json!({"watermark_config": null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "watermark clear failed: {resp}");
    assert!(resp["watermark_config"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn patch_my_profile_unknown_backend() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "badbackend").await;

    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        "/api/v1/users/me",
        Some(&token),
        &serde_json::json!({"storage_backend": "nope"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].as_str().unwrap().contains("unknown backend"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn patch_my_profile_username_conflict() {
    let app = test_app().await;
    let (user_a, token_a, _) = create_user(&app, "conflict_a").await;
    let (user_b, _, _) = create_user(&app, "conflict_b").await;
    assert_ne!(user_a, user_b);

    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        "/api/v1/users/me",
        Some(&token_a),
        &serde_json::json!({"username": user_b}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "expected 409, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_my_stats_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "stats").await;

    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/users/me/stats", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "stats failed: {resp}");
    assert_eq!(resp["total_images"].as_i64().unwrap(), 0);
    assert_eq!(resp["total_size"].as_i64().unwrap(), 0);
    assert_eq!(resp["backend"].as_str().unwrap(), "local");
    assert!(resp["storage_quota"].is_number());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn get_my_stats_reflects_uploads() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "statsupload").await;

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

    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/users/me/stats", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "stats after upload failed: {resp}");
    assert!(resp["total_images"].as_i64().unwrap() >= 1);
    assert!(resp["total_size"].as_i64().unwrap() > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn change_password_success() {
    let app = test_app().await;
    let (username, token, _) = create_user(&app, "pwchange").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/me/password",
        Some(&token),
        &serde_json::json!({"current_password": "user123456", "new_password": "newpass12345"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "password change failed: {resp}");
    assert_eq!(resp["message"].as_str().unwrap(), "password updated");

    let (status, _) = send_json(
        &app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        &serde_json::json!({"username": username, "password": "newpass12345"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login with new password failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn change_password_wrong_current() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "pwwrong").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/me/password",
        Some(&token),
        &serde_json::json!({"current_password": "wrongpass", "new_password": "newpass12345"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "expected 401, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn change_password_short_new_password() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "pwshort").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/me/password",
        Some(&token),
        &serde_json::json!({"current_password": "user123456", "new_password": "short"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].as_str().unwrap().contains("at least 8"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_list_empty() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "sc_empty").await;

    let (status, resp) = send_json(
        &app,
        Method::GET,
        "/api/v1/users/me/storage-configs",
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list configs failed: {resp}");
    assert!(resp.as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_create_unsupported_provider() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "sc_prov1").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/me/storage-configs",
        Some(&token),
        &serde_json::json!({
            "name": "cfg",
            "provider": "aws",
            "token": "tok",
            "repo": "owner/repo",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_create_unknown_provider() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "sc_prov2").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/me/storage-configs",
        Some(&token),
        &serde_json::json!({
            "name": "cfg",
            "provider": "gitlab",
            "token": "tok",
            "repo": "owner/repo",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_create_github_unreachable_repo() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "sc_net").await;
    let repo = format!("owner/nonexistent-{}", Uuid::new_v4().simple());

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/users/me/storage-configs",
        Some(&token),
        &serde_json::json!({
            "name": "github-cfg",
            "provider": "github",
            "token": "ghp_invalid_test_token",
            "repo": repo,
            "branch": "main",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_get_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "sc_get404").await;
    let missing = Uuid::new_v4();

    let (status, resp) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/users/me/storage-configs/{missing}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_patch_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "sc_patch404").await;
    let missing = Uuid::new_v4();

    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &format!("/api/v1/users/me/storage-configs/{missing}"),
        Some(&token),
        &serde_json::json!({"name": "renamed"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_delete_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "sc_del404").await;
    let missing = Uuid::new_v4();

    let (status, resp) = send_json(
        &app,
        Method::DELETE,
        &format!("/api/v1/users/me/storage-configs/{missing}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn storage_configs_set_default_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "sc_def404").await;
    let missing = Uuid::new_v4();

    let (status, resp) = send_json(
        &app,
        Method::POST,
        &format!("/api/v1/users/me/storage-configs/{missing}/default"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
    assert!(resp["error"].is_string());
}
