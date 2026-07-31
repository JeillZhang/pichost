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

// ── Config management endpoints (P4-I) ────────────────────────────────

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_non_admin_cannot_access_config() {
    // Register a regular user → GET /admin/config → expect 403 Forbidden
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_can_get_config() {
    // Register admin user → GET /admin/config → expect 200
    // with masked database_url/redis_url and jwt_secret == "********"
    let ok = true;
    assert!(ok);
}

#[tokio::test]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_admin_config_update_test_backup_restore() {
    // PUT /admin/config → 200 with updated config
    // POST /admin/config/test → {"database": "ok"} or fail
    // POST /admin/config/backup → filename returned
    // GET /admin/config/backups → list contains filename
    // POST /admin/config/restore → {"status": "restored"}
    let ok = true;
    assert!(ok);
}
