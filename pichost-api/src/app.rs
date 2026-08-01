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
use pichost_core::StorageRouter;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::cache::Cache;
use crate::db::DbPool;
use crate::middleware::rate_limit;

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub cache: Arc<Cache>,
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

fn auth_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/register", post(crate::routes::auth::register))
        .route("/login", post(crate::routes::auth::login))
        .route("/refresh", post(crate::routes::auth::refresh))
        .route("/logout", post(crate::routes::auth::logout))
        .route("/oauth/github", get(crate::routes::oauth::github_redirect))
        .route("/oauth/github/callback", get(crate::routes::oauth::github_callback))
        .route("/oauth/google", get(crate::routes::oauth::google_redirect))
        .route("/oauth/google/callback", get(crate::routes::oauth::google_callback))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_auth,
        ))
}

fn upload_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let protected =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    Router::new()
        .route("/", post(crate::routes::images::upload_handler))
        .route("/upload-url", post(crate::routes::images::url_upload_handler))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_upload,
        ))
        .route_layer(protected)
}

fn image_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let protected =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    Router::new()
        .route("/", get(crate::routes::images::list_images))
        .route("/batch-delete", post(crate::routes::images::batch_delete))
        .route("/batch-move", post(crate::routes::images::batch_move_images))
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

fn user_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let protected =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    Router::new()
        .route(
            "/me",
            get(crate::routes::users::get_my_profile)
                .patch(crate::routes::users::update_my_profile),
        )
        .route("/me/stats", get(crate::routes::users::get_my_stats))
        .route("/me/password", post(crate::routes::users::change_my_password))
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

fn category_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
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

fn admin_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
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
            get(crate::routes::admin::get_admin_config).put(crate::routes::admin::update_admin_config),
        )
        .route("/config/test", post(crate::routes::admin::test_admin_config))
        .route("/config/backup", post(crate::routes::admin::backup_admin_config))
        .route("/config/backups", get(crate::routes::admin::list_config_backups))
        .route("/config/restore", post(crate::routes::admin::restore_admin_config))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(admin_protected)
        .route_layer(protected)
}

fn public_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/{public_key}", get(crate::routes::images::public_get))
        .route("/thumb/{image_id}", get(crate::routes::images::public_get_thumb))
        .route("/webp/{image_id}", get(crate::routes::images::public_get_webp))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_public,
        ))
}

fn thumb_alias_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/{public_key}", get(crate::routes::images::public_get_thumb_by_key))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_public,
        ))
}

/// Assembles route groups into the top-level Router with shared middleware layers.
pub fn build_router(state: Arc<AppState>) -> Router {
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
pub fn configure_app(state: Arc<AppState>) -> Router {
    setup_security_headers(build_router(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{create_pool, Cache};
    use pichost_core::config::RustfsStorageConfig;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState {
            pool: sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_lazy("postgres://pichost:pichost@localhost:5432/pichost")
                .unwrap(),
            cache: Arc::new(Cache::new(create_pool("redis://localhost:6379", 2))),
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
        let state = test_state();
        let _ = auth_routes(state.clone());
        let _ = upload_routes(state.clone());
        let _ = image_routes(state.clone());
        let _ = user_routes(state.clone());
        let _ = category_routes(state.clone());
        let _ = admin_routes(state.clone());
        let _ = public_routes(state.clone());
        let _ = thumb_alias_routes(state.clone());
    }
}
