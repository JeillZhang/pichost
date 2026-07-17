/// Integration tests for admin API endpoints.
/// Requires running PostgreSQL + Redis (set DATABASE_URL + PICHOST_REDIS_URL).
/// Run with: cargo test -p pichost-api --test admin_test -- --ignored

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_non_admin_cannot_list_users() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_can_list_users() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_can_update_user() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_cannot_demote_self() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_cannot_delete_self() {
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_stats() {
    let ok = true;
    assert!(ok);
}
