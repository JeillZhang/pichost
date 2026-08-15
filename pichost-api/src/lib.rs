pub mod app;
pub mod cache;
pub mod cli;
pub mod db;
pub mod i18n_ext;
pub mod metrics;
pub mod middleware;
pub mod routes;
pub mod service;
pub mod services;
pub mod setup;

use pichost_core::config::load_config;
use pichost_core::i18n::{I18n, Language};

/// SQLite lite 模式启动链路(前台 run_app 与 Windows 服务共用;无强制向导)
pub async fn run_lite_from_env() -> Result<(), Box<dyn std::error::Error>> {
    run_lite_from_env_forced(false).await
}

/// 同 `run_lite_from_env`,但可强制运行初始化向导(--setup)
pub async fn run_lite_from_env_forced(forced: bool) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    I18n::init_global(
        Language::from_str_opt(&config.i18n.language),
        config.i18n.locales_dir.clone(),
    );
    let pool =
        db::create_sqlite_pool(&config.database.url, config.database.max_connections).await?;
    db::run_sqlite_migrations(&pool).await?;
    let config = crate::setup::maybe_run(&pool, &config, forced)
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e })?
        .unwrap_or(config);
    app::run_with_sqlite(config, pool).await
}
