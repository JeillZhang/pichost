use std::sync::Arc;

use pichost_api::app::{configure_app, init_storage_backends};
use pichost_api::{app::AppState, cache, db};
use pichost_core::config::load_config;
use pichost_core::i18n::{I18n, Language};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file (sibling of Cargo.toml, i.e. project root at runtime)
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter("info")
        .json()
        .init();

    let config = load_config()?;
    I18n::init_global(
        Language::from_str_opt(&config.i18n.language),
        config.i18n.locales_dir.clone(),
    );
    let pool = db::create_pg_pool(&config.database.url, config.database.max_connections).await?;
    db::run_pg_migrations(&pool).await?;
    let cache_pool = cache::create_pool(&config.redis.url, config.redis.pool_size as usize);

    let router = Arc::new(init_storage_backends(&config).await);
    let state = Arc::new(AppState {
        pool,
        cache: Arc::new(cache::Cache::new(cache_pool)),
        config: Arc::new(config),
        router,
    });

    let app = configure_app::<sqlx::Postgres>(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("API on :3000");
    axum::serve(listener, app).await?;
    Ok(())
}
