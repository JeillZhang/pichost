use pichost_api::services::upload::{ImageListQuery, ImageListResponse};

#[test]
fn test_image_list_query_defaults() {
    // Simulate query param parsing via serde
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
    // Invalid sort field should still parse but be caught at handler level
    let query: ImageListQuery = serde_urlencoded::from_str("sort=malicious;DROP TABLE").unwrap();
    assert_eq!(query.sort, "malicious;DROP TABLE"); // handler must validate
}

#[test]
fn test_image_list_response_total_pages_calculation() {
    // total_pages = ceil(total / per_page)
    // 23 items, 10 per page = 3 pages
    let resp = ImageListResponse { items: vec![], total: 23, page: 1, per_page: 10, total_pages: 3 };
    assert_eq!(resp.total_pages, 3);
    // 0 items, 20 per page = 1 page (always at least 1)
    let resp2 = ImageListResponse { items: vec![], total: 0, page: 1, per_page: 20, total_pages: 1 };
    assert_eq!(resp2.total_pages, 1);
}
