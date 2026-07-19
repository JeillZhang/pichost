use pichost_core::models::Category;
use uuid::Uuid;

#[test]
fn test_category_serde_roundtrip() {
    let cat = Category {
        id: Uuid::nil(),
        user_id: Uuid::nil(),
        name: "Travel Photos".into(),
        parent_id: None,
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&cat).unwrap();
    let parsed: Category = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "Travel Photos");
    assert_eq!(parsed.parent_id, None);
}

#[test]
fn test_image_category_id_optional() {
    let json = r#"{"id":"00000000-0000-0000-0000-000000000001","user_id":"00000000-0000-0000-0000-000000000000","public_key":"abc123","original_name":"test.png","storage_key":"k","storage_backend":"local","mime_type":"image/png","file_size":100,"width":null,"height":null,"sha256":"abc","url":"http://x","status":"active","storage_config_id":null,"created_at":"2026-01-01T00:00:00Z","category_id":"00000000-0000-0000-0000-000000000002"}"#;
    let img: pichost_core::models::Image = serde_json::from_str(json).unwrap();
    assert_eq!(img.category_id, Some(Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap()));
    let json_no_cat = r#"{"id":"00000000-0000-0000-0000-000000000001","user_id":"00000000-0000-0000-0000-000000000000","public_key":"abc123","original_name":"test.png","storage_key":"k","storage_backend":"local","mime_type":"image/png","file_size":100,"width":null,"height":null,"sha256":"abc","url":"http://x","status":"active","storage_config_id":null,"created_at":"2026-01-01T00:00:00Z"}"#;
    let img2: pichost_core::models::Image = serde_json::from_str(json_no_cat).unwrap();
    assert_eq!(img2.category_id, None);
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CreateCategoryRequest {
    name: String,
    parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct UpdateCategoryRequest {
    name: Option<String>,
}

#[test]
fn test_create_category_request_serde() {
    let json = r#"{"name":"Blog","parent_id":null}"#;
    let req: CreateCategoryRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.name, "Blog");
    assert_eq!(req.parent_id, None);
}

#[test]
fn test_create_category_request_with_parent() {
    let json = r#"{"name":"Rust","parent_id":"00000000-0000-0000-0000-000000000001"}"#;
    let req: CreateCategoryRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.name, "Rust");
    assert!(req.parent_id.is_some());
}

#[test]
fn test_update_category_request_partial() {
    let json = r#"{"name":"New Name"}"#;
    let req: UpdateCategoryRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.name, Some("New Name".into()));
}
