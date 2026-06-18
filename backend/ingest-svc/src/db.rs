use sqlx::PgPool;

pub async fn connect(database_url: &str) -> Result<PgPool, DbError> {
    shared::postgres::connect_pg_pool(database_url, shared::postgres::DEFAULT_MAX_CONNECTIONS)
        .await
        .map_err(DbError::Connect)
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: failed to connect to PostgreSQL: {0}")]
    Connect(sqlx::Error),
}
