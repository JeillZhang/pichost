use std::collections::HashMap;
use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue};
use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, patch, post},
    Router,
};
use pichost_core::config::AppConfig;
use pichost_core::storage::local::LocalStorage;
use pichost_core::storage::s3::RustfsStorage;
use pichost_core::storage::StorageBackend;
use pichost_core::DbType;
use pichost_core::StorageRouter;
use sqlx::Pool;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::cache::{Cache, CachePool, RedisInviteStore};
use crate::middleware::auth::RedisBlacklist;
use crate::middleware::rate_limit;
use crate::middleware::rate_limit::RedisRateLimiter;
use pichost_worker::queue::RedisQueue;

#[derive(Clone)]
pub struct AppState<DB: DbType> {
    pub pool: Pool<DB>,
    pub queue: Arc<dyn pichost_core::state::Queue>,
    pub blacklist: Arc<dyn pichost_core::state::Blacklist>,
    pub rate_limiter: Arc<dyn pichost_core::state::RateLimiter>,
    pub invites: Arc<dyn pichost_core::state::InviteStore>,
    pub cache: Arc<dyn pichost_core::state::Cache>,
    pub config: Arc<AppConfig>,
    pub router: Arc<StorageRouter>,
}

/// Initialize storage backends: always registers local storage, and
/// conditionally registers RustFS if configured.
pub async fn init_storage_backends(config: &AppConfig) -> StorageRouter {
    let mut backends: HashMap<String, Arc<dyn StorageBackend>> = HashMap::new();

    let local = LocalStorage::new(
        config.storage.local_base_path.clone(),
        config.server.public_url.clone(),
    );
    backends.insert("local".into(), Arc::new(local));

    if let Some(rustfs_config) = &config.storage.rustfs {
        let rustfs = RustfsStorage::new(rustfs_config).await;
        tracing::info!(
            endpoint = %rustfs_config.endpoint,
            bucket = %rustfs_config.bucket,
            "Rustfs storage backend initialized"
        );
        backends.insert("rustfs".into(), Arc::new(rustfs));
    }

    StorageRouter::new(backends, config.storage.default_backend.clone())
}

/// Trait-object state components. `build_state_components` wires the
/// standard-mode (Redis) implementations; T26 adds the SQLite branch.
pub struct StateComponents {
    pub queue: Arc<dyn pichost_core::state::Queue>,
    pub blacklist: Arc<dyn pichost_core::state::Blacklist>,
    pub rate_limiter: Arc<dyn pichost_core::state::RateLimiter>,
    pub invites: Arc<dyn pichost_core::state::InviteStore>,
    pub cache: Arc<dyn pichost_core::state::Cache>,
}

/// Assemble the standard-mode (Redis) trait-object components shared by the
/// API server, unit tests, and the integration-test harness.
pub fn build_state_components(cache_pool: CachePool, queue_pool: CachePool) -> StateComponents {
    let cache: Arc<dyn pichost_core::state::Cache> = Arc::new(Cache::new(cache_pool.clone()));
    let queue: Arc<dyn pichost_core::state::Queue> = Arc::new(RedisQueue::new(queue_pool));
    let blacklist: Arc<dyn pichost_core::state::Blacklist> =
        Arc::new(RedisBlacklist::new(Cache::new(cache_pool.clone())));
    let rate_limiter: Arc<dyn pichost_core::state::RateLimiter> =
        Arc::new(RedisRateLimiter::new(Cache::new(cache_pool.clone())));
    let invites: Arc<dyn pichost_core::state::InviteStore> =
        Arc::new(RedisInviteStore::new(Cache::new(cache_pool)));
    StateComponents {
        queue,
        blacklist,
        rate_limiter,
        invites,
        cache,
    }
}

fn auth_routes<DB: DbType>(state: Arc<AppState<DB>>) -> Router<Arc<AppState<DB>>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (
        uuid::Uuid,
        String,
        Option<String>,
        String,
        bool,
        Option<i64>,
    ): crate::db::DbRow<DB>,
    (String, Option<String>, bool, Option<i64>): crate::db::DbRow<DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (i64,): crate::db::DbRow<DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (uuid::Uuid, String, Option<String>, bool, Option<i64>): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    Router::new()
        .route("/register", post(crate::routes::auth::register))
        .route("/login", post(crate::routes::auth::login))
        .route("/refresh", post(crate::routes::auth::refresh))
        .route("/logout", post(crate::routes::auth::logout))
        .route("/oauth/github", get(crate::routes::oauth::github_redirect))
        .route(
            "/oauth/github/callback",
            get(crate::routes::oauth::github_callback),
        )
        .route("/oauth/google", get(crate::routes::oauth::google_redirect))
        .route(
            "/oauth/google/callback",
            get(crate::routes::oauth::google_callback),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_auth,
        ))
}

fn upload_routes<DB: DbType>(state: Arc<AppState<DB>>) -> Router<Arc<AppState<DB>>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>, Option<serde_json::Value>): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (bool,): crate::db::DbRow<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i32>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> &'a [uuid::Uuid]: sqlx::Encode<'q, DB>,
    [uuid::Uuid]: sqlx::Type<DB>,
{
    let protected =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    Router::new()
        .route("/", post(crate::routes::images::upload_handler))
        .route(
            "/upload-url",
            post(crate::routes::images::url_upload_handler),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_upload,
        ))
        .route_layer(protected)
}

fn image_routes<DB: DbType>(state: Arc<AppState<DB>>) -> Router<Arc<AppState<DB>>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>, Option<serde_json::Value>): crate::db::DbRow<DB>,
    (
        std::string::String,
        std::string::String,
        std::string::String,
    ): crate::db::DbRow<DB>,
    (String, String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Vec<uuid::Uuid>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> &'a [uuid::Uuid]: sqlx::Encode<'q, DB>,
    [uuid::Uuid]: sqlx::Type<DB>,
{
    let protected =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    Router::new()
        .route("/", get(crate::routes::images::list_images))
        .route("/batch-delete", post(crate::routes::images::batch_delete))
        .route(
            "/batch-move",
            post(crate::routes::images::batch_move_images),
        )
        .route("/{id}/links", get(crate::routes::images::get_image_links))
        .route(
            "/{id}",
            get(crate::routes::images::get_image)
                .patch(crate::routes::images::rename_image)
                .delete(crate::routes::images::delete_image),
        )
        .route("/{id}/move", post(crate::routes::images::move_image))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(protected)
}

fn user_routes<DB: DbType>(state: Arc<AppState<DB>>) -> Router<Arc<AppState<DB>>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>, Option<serde_json::Value>): crate::db::DbRow<DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    (bool,): crate::db::DbRow<DB>,
    (i64, Option<i64>): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    (std::string::String,): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (Option<i64>,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
{
    let protected =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    Router::new()
        .route(
            "/me",
            get(crate::routes::users::get_my_profile)
                .patch(crate::routes::users::update_my_profile),
        )
        .route("/me/stats", get(crate::routes::users::get_my_stats))
        .route(
            "/me/password",
            post(crate::routes::users::change_my_password),
        )
        .route(
            "/me/storage-configs",
            get(crate::routes::storage_configs::list_configs)
                .post(crate::routes::storage_configs::create_config),
        )
        .route(
            "/me/storage-configs/{id}",
            get(crate::routes::storage_configs::get_config)
                .patch(crate::routes::storage_configs::update_config)
                .delete(crate::routes::storage_configs::delete_config),
        )
        .route(
            "/me/storage-configs/{id}/default",
            post(crate::routes::storage_configs::set_default),
        )
        .route("/oauth/link", post(crate::routes::oauth::oauth_link))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(protected)
}

fn category_routes<DB: DbType>(state: Arc<AppState<DB>>) -> Router<Arc<AppState<DB>>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>, Option<serde_json::Value>): crate::db::DbRow<DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let protected =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    Router::new()
        .route(
            "/",
            get(crate::routes::categories::list_categories)
                .post(crate::routes::categories::create_category),
        )
        .route(
            "/{id}",
            get(crate::routes::categories::get_category)
                .patch(crate::routes::categories::update_category)
                .delete(crate::routes::categories::delete_category),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(protected)
}

fn admin_routes<DB: DbType>(state: Arc<AppState<DB>>) -> Router<Arc<AppState<DB>>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>, Option<serde_json::Value>): crate::db::DbRow<DB>,
    (String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    (i64, i64): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (Option<i64>,): crate::db::DbRow<DB>,
    (bool,): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let protected =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    let admin_protected =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_admin);
    Router::new()
        .route("/stats", get(crate::routes::admin::get_admin_stats))
        .route("/users", get(crate::routes::admin::list_users))
        .route(
            "/users/{id}",
            patch(crate::routes::admin::update_user).delete(crate::routes::admin::delete_user),
        )
        .route(
            "/invites",
            get(crate::routes::admin::list_invites).post(crate::routes::admin::create_invite),
        )
        .route(
            "/config",
            get(crate::routes::admin::get_admin_config)
                .put(crate::routes::admin::update_admin_config),
        )
        .route(
            "/config/test",
            post(crate::routes::admin::test_admin_config),
        )
        .route(
            "/config/backup",
            post(crate::routes::admin::backup_admin_config),
        )
        .route(
            "/config/backups",
            get(crate::routes::admin::list_config_backups),
        )
        .route(
            "/config/restore",
            post(crate::routes::admin::restore_admin_config),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(admin_protected)
        .route_layer(protected)
}

fn public_routes<DB: DbType>(state: Arc<AppState<DB>>) -> Router<Arc<AppState<DB>>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<std::string::String>, std::string::String): crate::db::DbRow<DB>,
    (
        std::string::String,
        std::string::String,
        std::string::String,
        std::string::String,
    ): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    Router::new()
        .route("/{public_key}", get(crate::routes::images::public_get))
        .route(
            "/thumb/{image_id}",
            get(crate::routes::images::public_get_thumb),
        )
        .route(
            "/webp/{image_id}",
            get(crate::routes::images::public_get_webp),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_public,
        ))
}

fn thumb_alias_routes<DB: DbType>(state: Arc<AppState<DB>>) -> Router<Arc<AppState<DB>>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<std::string::String>, std::string::String): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
{
    Router::new()
        .route(
            "/{public_key}",
            get(crate::routes::images::public_get_thumb_by_key),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_public,
        ))
}

/// Assembles route groups into the top-level Router with shared middleware layers.
pub fn build_router<DB: DbType>(state: Arc<AppState<DB>>) -> Router
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>, Option<serde_json::Value>): crate::db::DbRow<DB>,
    (Option<std::string::String>, std::string::String): crate::db::DbRow<DB>,
    (
        std::string::String,
        std::string::String,
        std::string::String,
    ): crate::db::DbRow<DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    (String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    (String, String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (i64, i64): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    (Option<i64>,): crate::db::DbRow<DB>,
    (bool,): crate::db::DbRow<DB>,
    (
        uuid::Uuid,
        String,
        Option<String>,
        String,
        bool,
        Option<i64>,
    ): crate::db::DbRow<DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    (String, Option<String>, bool, Option<i64>): crate::db::DbRow<DB>,
    (String, String, String, String): crate::db::DbRow<DB>,
    (i64, Option<i64>): crate::db::DbRow<DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    usize: sqlx::ColumnIndex<DB::Row>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    (String,): crate::db::DbRow<DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Vec<uuid::Uuid>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i32>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> &'a [uuid::Uuid]: sqlx::Encode<'q, DB>,
    [uuid::Uuid]: sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
{
    Router::new()
        .nest("/api/v1/auth", auth_routes(state.clone()))
        .nest("/api/v1/images", upload_routes(state.clone()))
        .nest("/api/v1/images", image_routes(state.clone()))
        .nest("/api/v1/users", user_routes(state.clone()))
        .nest("/api/v1/categories", category_routes(state.clone()))
        .nest("/api/v1/admin", admin_routes(state.clone()))
        .nest("/u", public_routes(state.clone()))
        .nest("/t", thumb_alias_routes(state.clone()))
        .route("/api/health", get(crate::routes::health::health_check))
        .route("/metrics", get(metrics_handler))
        .layer(middleware::from_fn(
            crate::middleware::metrics::track_metrics,
        ))
        .layer(CorsLayer::permissive())
        .layer(DefaultBodyLimit::max(52_428_800))
        .with_state(state)
}

async fn metrics_handler() -> String {
    crate::metrics::encode_metrics()
}

/// Adds security-related response headers to the router.
fn setup_security_headers(router: Router) -> Router {
    router
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(
                "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; sandbox",
            ),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
}

/// Assembles the complete application router with security headers.
pub fn configure_app<DB: DbType>(state: Arc<AppState<DB>>) -> Router
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>, Option<serde_json::Value>): crate::db::DbRow<DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    (Option<std::string::String>, std::string::String): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (
        std::string::String,
        std::string::String,
        std::string::String,
    ): crate::db::DbRow<DB>,
    (String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    (String, String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    (i64, i64): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    (Option<i64>,): crate::db::DbRow<DB>,
    (bool,): crate::db::DbRow<DB>,
    (
        uuid::Uuid,
        String,
        Option<String>,
        String,
        bool,
        Option<i64>,
    ): crate::db::DbRow<DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    usize: sqlx::ColumnIndex<DB::Row>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    (i64, Option<i64>): crate::db::DbRow<DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Vec<uuid::Uuid>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i32>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> &'a [uuid::Uuid]: sqlx::Encode<'q, DB>,
    [uuid::Uuid]: sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
{
    setup_security_headers(build_router(state))
}

/// Per-mode application runner. The Postgres branch wires the Redis
/// implementations; the SQLite branch is added in T26.
pub async fn run_with<DB: DbType>(
    config: AppConfig,
    pool: Pool<DB>,
    cache_pool: CachePool,
    queue_pool: CachePool,
) -> Result<(), Box<dyn std::error::Error>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (Option<i64>, Option<serde_json::Value>): crate::db::DbRow<DB>,
    pichost_core::models::Category: crate::db::DbRow<DB>,
    (Option<std::string::String>, std::string::String): crate::db::DbRow<DB>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    (
        std::string::String,
        std::string::String,
        std::string::String,
    ): crate::db::DbRow<DB>,
    (String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    (String, String, Option<String>, Option<String>): crate::db::DbRow<DB>,
    (i64, i64): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    (Option<i64>,): crate::db::DbRow<DB>,
    (bool,): crate::db::DbRow<DB>,
    (
        uuid::Uuid,
        String,
        Option<String>,
        String,
        bool,
        Option<i64>,
    ): crate::db::DbRow<DB>,
    for<'r> &'r str: sqlx::ColumnIndex<DB::Row>,
    usize: sqlx::ColumnIndex<DB::Row>,
    <DB as sqlx::Database>::QueryResult: crate::db::DbQueryResult,
    (i64, Option<i64>): crate::db::DbRow<DB>,
    pichost_core::models::UserStorageConfig: crate::db::DbRow<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    chrono::DateTime<chrono::Utc>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    sqlx::types::Json<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<uuid::Uuid>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i32: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<serde_json::Value>:
        for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Vec<uuid::Uuid>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i32>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'a, 'q> &'a [uuid::Uuid]: sqlx::Encode<'q, DB>,
    [uuid::Uuid]: sqlx::Type<DB>,
    for<'a, 'q> sqlx::types::Json<&'a serde_json::Value>: sqlx::Encode<'q, DB>,
{
    let router = Arc::new(init_storage_backends(&config).await);
    let components = build_state_components(cache_pool, queue_pool);
    let state = Arc::new(AppState {
        pool,
        queue: components.queue,
        blacklist: components.blacklist,
        rate_limiter: components.rate_limiter,
        invites: components.invites,
        cache: components.cache,
        config: Arc::new(config),
        router,
    });
    let app = configure_app::<DB>(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("API on :3000");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::create_pool;
    use pichost_core::config::RustfsStorageConfig;

    async fn test_state() -> Arc<AppState<sqlx::Sqlite>> {
        let cache_pool = create_pool("redis://localhost:6379", 2);
        let components =
            build_state_components(cache_pool.clone(), create_pool("redis://localhost:6379", 2));
        Arc::new(AppState {
            pool: crate::db::create_sqlite_pool("sqlite::memory:", 1)
                .await
                .unwrap(),
            queue: components.queue,
            blacklist: components.blacklist,
            rate_limiter: components.rate_limiter,
            invites: components.invites,
            cache: components.cache,
            config: Arc::new(AppConfig::default()),
            router: Arc::new(StorageRouter::new(HashMap::new(), "local".into())),
        })
    }

    #[tokio::test]
    async fn test_init_storage_backends_local_only() {
        let router = init_storage_backends(&AppConfig::default()).await;
        assert!(router.get("local").is_some());
        assert_eq!(router.backend_count(), 1);
    }

    #[tokio::test]
    async fn test_init_storage_backends_with_rustfs() {
        let mut config = AppConfig::default();
        config.storage.rustfs = Some(RustfsStorageConfig {
            endpoint: "http://localhost:9000".into(),
            bucket: "pichost".into(),
            access_key: "minioadmin".into(),
            secret_key: "minioadmin".into(),
            region: "us-east-1".into(),
            use_ssl: false,
            public_endpoint: None,
        });
        let router = init_storage_backends(&config).await;
        assert!(router.get("rustfs").is_some());
        assert_eq!(router.backend_count(), 2);
    }

    #[tokio::test]
    async fn test_route_builders_construct() {
        let state = test_state().await;
        let _ = auth_routes(state.clone());
        // upload/image routes bind &[Uuid] (ANY($1)) — PostgreSQL-only;
        // exercised by the PG integration harness (tests/common).
        let _ = user_routes(state.clone());
        let _ = category_routes(state.clone());
        let _ = admin_routes(state.clone());
        let _ = public_routes(state.clone());
        let _ = thumb_alias_routes(state.clone());
    }
}
