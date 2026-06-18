use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "db_migrate=info,sqlx=warn".to_string()),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL is required to run migrations")?;

    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .context("failed to connect to PostgreSQL for migrations")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to apply database migrations")?;

    tracing::info!("database migrations applied");

    Ok(())
}
