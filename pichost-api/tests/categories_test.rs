mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::Value;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_empty_tree() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_empty").await;

    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/categories", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "list categories failed: {resp}");
    assert!(resp.as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_create_root_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_create").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Photos"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create category failed: {resp}");
    let id = Uuid::parse_str(resp["id"].as_str().unwrap()).unwrap();
    assert_ne!(id, Uuid::nil());
    assert_eq!(resp["name"].as_str().unwrap(), "Photos");
    assert!(resp["parent_id"].is_null());
    assert!(resp["created_at"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_create_empty_name() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_empty_name").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "   "}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_create_name_too_long() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_long").await;
    let long_name = "x".repeat(129);

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": long_name}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_get_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_get").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Travel"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = resp["id"].as_str().unwrap().to_string();

    let (status, resp) =
        send_json(&app, Method::GET, &format!("/api/v1/categories/{id}"), Some(&token), &Value::Null)
            .await;
    assert_eq!(status, StatusCode::OK, "get category failed: {resp}");
    assert_eq!(resp["id"].as_str().unwrap(), id);
    assert_eq!(resp["name"].as_str().unwrap(), "Travel");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_get_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_get404").await;
    let missing = Uuid::new_v4();

    let (status, resp) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/categories/{missing}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_update_rename_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_rename").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Old"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = resp["id"].as_str().unwrap().to_string();

    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &format!("/api/v1/categories/{id}"),
        Some(&token),
        &serde_json::json!({"name": "New"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "rename category failed: {resp}");
    assert_eq!(resp["name"].as_str().unwrap(), "New");
    assert_eq!(resp["id"].as_str().unwrap(), id);

    let (status, resp) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/categories/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["name"].as_str().unwrap(), "New");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_update_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_up404").await;
    let missing = Uuid::new_v4();

    let (status, resp) = send_json(
        &app,
        Method::PATCH,
        &format!("/api/v1/categories/{missing}"),
        Some(&token),
        &serde_json::json!({"name": "Renamed"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_delete_success() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_del").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Doomed"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = resp["id"].as_str().unwrap().to_string();

    let (status, resp) = send_json(
        &app,
        Method::DELETE,
        &format!("/api/v1/categories/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "delete category failed: {resp}");

    let (status, _) = send_json(
        &app,
        Method::GET,
        &format!("/api/v1/categories/{id}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_delete_not_found() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_del404").await;
    let missing = Uuid::new_v4();

    let (status, resp) = send_json(
        &app,
        Method::DELETE,
        &format!("/api/v1/categories/{missing}"),
        Some(&token),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_tree_nested_structure() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_tree").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Root"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let root_id = resp["id"].as_str().unwrap().to_string();

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Child", "parent_id": root_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create child failed: {resp}");
    let child_id = resp["id"].as_str().unwrap().to_string();
    assert_eq!(resp["parent_id"].as_str().unwrap(), root_id);

    let (status, resp) =
        send_json(&app, Method::GET, "/api/v1/categories", Some(&token), &Value::Null).await;
    assert_eq!(status, StatusCode::OK, "list tree failed: {resp}");
    let tree = resp.as_array().unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0]["id"].as_str().unwrap(), root_id);
    assert_eq!(tree[0]["name"].as_str().unwrap(), "Root");
    let children = tree[0]["children"].as_array().unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0]["id"].as_str().unwrap(), child_id);
    assert_eq!(children[0]["name"].as_str().unwrap(), "Child");
    assert!(children[0]["children"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_duplicate_name_conflict() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_dup").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Photos"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let root_id = resp["id"].as_str().unwrap().to_string();

    let (status, _) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Vacation", "parent_id": root_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Vacation", "parent_id": root_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "expected 409, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_create_child_with_missing_parent() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_orphan").await;
    let missing = Uuid::new_v4();

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Orphan", "parent_id": missing}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn categories_depth_limit_rejects_third_level() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_depth").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "L1"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let l1_id = resp["id"].as_str().unwrap().to_string();

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "L2", "parent_id": l1_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "second level should succeed: {resp}");
    let l2_id = resp["id"].as_str().unwrap().to_string();

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "L3", "parent_id": l2_id}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400, got {status}: {resp}");
    assert!(resp["error"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL"]
async fn duplicate_category_returns_conflict() {
    let app = test_app().await;
    let (_, token, _) = create_user(&app, "cate_dup_conflict").await;

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Photos"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create category failed: {resp}");
    let root_id = resp["id"].as_str().unwrap().to_string();

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Vacation", "parent_id": root_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create child failed: {resp}");

    let (status, resp) = send_json(
        &app,
        Method::POST,
        "/api/v1/categories",
        Some(&token),
        &serde_json::json!({"name": "Vacation", "parent_id": root_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "expected 409, got {status}: {resp}");
    assert!(resp["error"].is_string());
}
