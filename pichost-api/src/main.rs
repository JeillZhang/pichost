use std::sync::Arc;

use axum::{extract::DefaultBodyLimit, middleware, routing::{get, post}, Router};
use axum::http::{HeaderName, HeaderValue};
use pichost_api::{app::AppState, cache, db, routes};
use pichost_api::middleware::rate_limit;
use pichost_core::config::load_config;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .json()
        .init();

    let config = load_config()?;
    let pool = db::create_pool(&config.database.url, config.database.max_connections).await?;
    db::run_migrations(&pool).await?;
    let cache_pool = cache::create_pool(&config.redis.url, config.redis.pool_size as usize);
    let state = Arc::new(AppState {
        pool,
        cache: Arc::new(cache::Cache::new(cache_pool)),
        config: Arc::new(config),
    });

    let protected =
        middleware::from_fn_with_state(state.clone(), pichost_api::middleware::auth::require_auth);

    // Auth routes — rate limit by IP, 5 req/min
    let auth_routes = Router::new()
        .route("/register", post(routes::auth::register))
        .route("/login", post(routes::auth::login))
        .route("/refresh", post(routes::auth::refresh))
        .route("/logout", post(routes::auth::logout))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_auth,
        ));

    // Upload route — rate limit by user_id (or IP), 30 req/min + auth
    let upload_routes = Router::new()
        .route("/", post(routes::images::upload_handler))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_upload,
        ))
        .route_layer(protected.clone());

    // Image list/get/delete — rate limit by user_id (or IP), 60 req/min + auth
    let image_routes = Router::new()
        .route("/", get(routes::images::list_images))
        .route(
            "/{id}",
            get(routes::images::get_image).delete(routes::images::delete_image),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_general,
        ))
        .route_layer(protected.clone());

    // Public routes — rate limit by IP, 200 req/min
    let public_routes = Router::new()
        .route("/{public_key}", get(routes::images::public_get))
        .route("/thumb/{image_id}", get(routes::images::public_get_thumb))
        .route("/webp/{image_id}", get(routes::images::public_get_webp))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_public,
        ));

    let app = Router::new()
        .nest("/api/v1/auth", auth_routes)
        .nest("/api/v1/images", upload_routes)
        .nest("/api/v1/images", image_routes)
        .nest("/u", public_routes)
        .layer(CorsLayer::permissive())
        .layer(DefaultBodyLimit::max(52_428_800))
        // Security headers
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
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("API on :3000");
    axum::serve(listener, app).await?;
    Ok(())
}
