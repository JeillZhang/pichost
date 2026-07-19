use pichost_api::services::upload::{ImageListQuery, ImageListResponse};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct MoveImageRequest {
    category_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct BatchMoveRequest {
    image_ids: Vec<Uuid>,
    category_id: Uuid,
}

#[test]
fn test_image_list_query_defaults() {
    let query: ImageListQuery = serde_urlencoded::from_str("").unwrap();
    assert_eq!(query.page, 1);
    assert_eq!(query.per_page, 20);
    assert_eq!(query.sort, "created_at");
    assert_eq!(query.order, "desc");
    assert_eq!(query.search, "");
}

#[test]
fn test_image_list_query_parse_all_params() {
    let query: ImageListQuery = serde_urlencoded::from_str(
        "page=2&per_page=10&sort=file_size&order=asc&search=cat"
    ).unwrap();
    assert_eq!(query.page, 2);
    assert_eq!(query.per_page, 10);
    assert_eq!(query.sort, "file_size");
    assert_eq!(query.order, "asc");
    assert_eq!(query.search, "cat");
}

#[test]
fn test_image_list_query_rejects_invalid_sort() {
    let query: ImageListQuery = serde_urlencoded::from_str("sort=malicious;DROP TABLE").unwrap();
    assert_eq!(query.sort, "malicious;DROP TABLE");
}

#[test]
fn test_image_list_response_total_pages_calculation() {
    let resp = ImageListResponse { items: vec![], total: 23, page: 1, per_page: 10, total_pages: 3 };
    assert_eq!(resp.total_pages, 3);
    let resp2 = ImageListResponse { items: vec![], total: 0, page: 1, per_page: 20, total_pages: 1 };
    assert_eq!(resp2.total_pages, 1);
}

#[test]
fn test_move_image_request_serde() {
    let expected = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let json = r#"{"category_id":"00000000-0000-0000-0000-000000000001"}"#;
    let req: MoveImageRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.category_id, expected);
}

#[test]
fn test_batch_move_request_serde() {
    let json = r#"{"image_ids":["00000000-0000-0000-0000-000000000001","00000000-0000-0000-0000-000000000002"],"category_id":"00000000-0000-0000-0000-000000000003"}"#;
    let req: BatchMoveRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.image_ids.len(), 2);
}

#[test]
fn test_image_list_query_with_category_id() {
    let expected = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let query = "page=1&per_page=20&category_id=00000000-0000-0000-0000-000000000001";
    let params: ImageListQuery =
        serde_urlencoded::from_str(query).unwrap();
    assert_eq!(params.category_id, Some(expected));
}

#[test]
fn test_image_list_query_without_category_id() {
    let query = "page=1&per_page=20";
    let params: ImageListQuery =
        serde_urlencoded::from_str(query).unwrap();
    assert_eq!(params.category_id, None);
}
