use std::collections::HashMap;
use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue};
use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, patch, post},
    Router,
};
use pichost_api::middleware::rate_limit;
use pichost_api::{app::AppState, cache, db, metrics, routes};
use pichost_core::config::{load_config, AppConfig};
use pichost_core::storage::local::LocalStorage;
use pichost_core::storage::s3::RustfsStorage;
use pichost_core::storage::StorageBackend;
use pichost_core::StorageRouter;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

async fn metrics_handler() -> String {
    metrics::encode_metrics()
}

/// Initialize storage backends: always registers local storage, and
/// conditionally registers RustFS if configured.
async fn init_storage_backends(config: &AppConfig) -> StorageRouter {
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
        .route("/register", post(routes::auth::register))
        .route("/login", post(routes::auth::login))
        .route("/refresh", post(routes::auth::refresh))
        .route("/logout", post(routes::auth::logout))
        .route("/oauth/github", get(routes::oauth::github_redirect))
        .route("/oauth/github/callback", get(routes::oauth::github_callback))
        .route("/oauth/google", get(routes::oauth::google_redirect))
        .route("/oauth/google/callback", get(routes::oauth::google_callback))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_auth,
        ))
}

fn upload_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let protected =
        middleware::from_fn_with_state(state.clone(), pichost_api::middleware::auth::require_auth);
    Router::new()
        .route("/", post(routes::images::upload_handler))
        .route("/upload-url", post(routes::images::url_upload_handler))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_upload,
        ))
        .route_layer(protected)
}

fn image_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let protected =
        middleware::from_fn_with_state(state.clone(), pichost_api::middleware::auth::require_auth);
    Router::new()
        .route("/", get(routes::images::list_images))
        .route("/batch-delete", post(routes::images::batch_delete))
        .route("/batch-move", post(routes::images::batch_move_images))
        .route("/{id}/links", get(routes::images::get_image_links))
        .route(
            "/{id}",
            get(routes::images::get_image).delete(routes::images::delete_image),
        )
        .route("/{id}/move", post(routes::images::move_image))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(protected)
}

fn user_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let protected =
        middleware::from_fn_with_state(state.clone(), pichost_api::middleware::auth::require_auth);
    Router::new()
        .route(
            "/me",
            get(routes::users::get_my_profile)
                .patch(routes::users::update_my_profile),
        )
        .route("/me/stats", get(routes::users::get_my_stats))
        .route("/me/password", post(routes::users::change_my_password))
        .route(
            "/me/storage-configs",
            get(routes::storage_configs::list_configs)
                .post(routes::storage_configs::create_config),
        )
        .route(
            "/me/storage-configs/{id}",
            get(routes::storage_configs::get_config)
                .patch(routes::storage_configs::update_config)
                .delete(routes::storage_configs::delete_config),
        )
        .route(
            "/me/storage-configs/{id}/default",
            post(routes::storage_configs::set_default),
        )
        .route("/oauth/link", post(routes::oauth::oauth_link))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(protected)
}

fn category_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let protected =
        middleware::from_fn_with_state(state.clone(), pichost_api::middleware::auth::require_auth);
    Router::new()
        .route(
            "/",
            get(routes::categories::list_categories).post(routes::categories::create_category),
        )
        .route(
            "/{id}",
            get(routes::categories::get_category)
                .patch(routes::categories::update_category)
                .delete(routes::categories::delete_category),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(protected)
}

fn admin_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let protected =
        middleware::from_fn_with_state(state.clone(), pichost_api::middleware::auth::require_auth);
    let admin_protected =
        middleware::from_fn_with_state(state.clone(), pichost_api::middleware::auth::require_admin);
    Router::new()
        .route("/stats", get(routes::admin::get_admin_stats))
        .route("/users", get(routes::admin::list_users))
        .route(
            "/users/{id}",
            patch(routes::admin::update_user).delete(routes::admin::delete_user),
        )
        .route(
            "/invites",
            get(routes::admin::list_invites).post(routes::admin::create_invite),
        )
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_general,
        ))
        .route_layer(admin_protected)
        .route_layer(protected)
}

fn public_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/{public_key}", get(routes::images::public_get))
        .route("/thumb/{image_id}", get(routes::images::public_get_thumb))
        .route("/webp/{image_id}", get(routes::images::public_get_webp))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_public,
        ))
}

fn thumb_alias_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/{public_key}", get(routes::images::public_get_thumb_by_key))
        .route_layer(middleware::from_fn_with_state(
            state,
            rate_limit::rate_limit_public,
        ))
}

/// Assembles route groups into the top-level Router with shared middleware layers.
fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api/v1/auth", auth_routes(state.clone()))
        .nest("/api/v1/images", upload_routes(state.clone()))
        .nest("/api/v1/images", image_routes(state.clone()))
        .nest("/api/v1/users", user_routes(state.clone()))
        .nest("/api/v1/categories", category_routes(state.clone()))
        .nest("/api/v1/admin", admin_routes(state.clone()))
        .nest("/u", public_routes(state.clone()))
        .nest("/t", thumb_alias_routes(state.clone()))
        .route("/api/health", get(routes::health::health_check))
        .route("/metrics", get(metrics_handler))
        .layer(middleware::from_fn(
            pichost_api::middleware::metrics::track_metrics,
        ))
        .layer(CorsLayer::permissive())
        .layer(DefaultBodyLimit::max(52_428_800))
        .with_state(state)
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
fn configure_app(state: Arc<AppState>) -> Router {
    setup_security_headers(build_router(state))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file (sibling of Cargo.toml, i.e. project root at runtime)
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter("info")
        .json()
        .init();

    let config = load_config()?;
    let pool = db::create_pool(&config.database.url, config.database.max_connections).await?;
    db::run_migrations(&pool).await?;
    let cache_pool = cache::create_pool(&config.redis.url, config.redis.pool_size as usize);

    let router = Arc::new(init_storage_backends(&config).await);
    let state = Arc::new(AppState {
        pool,
        cache: Arc::new(cache::Cache::new(cache_pool)),
        config: Arc::new(config),
        router,
    });

    let app = configure_app(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("API on :3000");
    axum::serve(listener, app).await?;
    Ok(())
}
