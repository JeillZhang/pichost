#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .json()
        .init();

    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://pichost:pichost@localhost:5432/pichost".into()
    });

    let pool = pichost_api::db::create_pool(&url, 5).await?;
    pichost_api::db::run_migrations(&pool).await?;

    tracing::info!("migrations done");

    Ok(())
}
