use pichost_api::{app, cache, db};
use pichost_core::config::{load_config, DatabaseMode};
use pichost_core::i18n::{I18n, Language};

mod cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = match cli::parse_cli_args(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(c) => c,
        Err(usage) => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    match cmd {
        cli::CliCommand::Run => return run_app().await,
        cli::CliCommand::Help => {
            println!("{}", cli::USAGE);
            return Ok(());
        }
        other => {
            #[cfg(windows)]
            {
                crate::service::dispatch_cli(other).await;
            }
            #[cfg(not(windows))]
            {
                eprintln!("error: {:?} is only supported on Windows", other);
                std::process::exit(1);
            }
        }
    }
}

async fn run_app() -> Result<(), Box<dyn std::error::Error>> {
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

    match config.database.mode {
        DatabaseMode::Postgres => {
            let pool =
                db::create_pg_pool(&config.database.url, config.database.max_connections).await?;
            db::run_pg_migrations(&pool).await?;
            let cache_pool = cache::create_pool(&config.redis.url, config.redis.pool_size as usize);
            let queue_pool = cache::create_pool(&config.redis.url, config.redis.pool_size as usize);
            app::run_with::<sqlx::Postgres>(config, pool, cache_pool, queue_pool).await
        }
        DatabaseMode::Sqlite => {
            let pool =
                db::create_sqlite_pool(&config.database.url, config.database.max_connections)
                    .await?;
            db::run_sqlite_migrations(&pool).await?;
            app::run_with_sqlite(config, pool).await
        }
    }
}
