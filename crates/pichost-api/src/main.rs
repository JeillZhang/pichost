use std::sync::Arc;

use axum::{Router, routing::post};
use pichost_api::{app::AppState, cache, db, routes};
use pichost_core::config::load_config;
use tower_http::cors::CorsLayer;

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

    let app = Router::new()
        .nest(
            "/api/v1/auth",
            Router::new()
                .route("/register", post(routes::auth::register))
                .route("/login", post(routes::auth::login)),
        )
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("API on :3000");
    axum::serve(listener, app).await?;
    Ok(())
}
