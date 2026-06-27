use std::{future::Future, pin::Pin};

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::DatabaseConfig;

pub type DbFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, DbError>> + Send + 'a>>;

pub trait GatewayDatabaseClient: Send + Sync {
    fn check(&self) -> DbFuture<'_, ()>;

    fn list_tile_sets(&self, limit: i64, offset: i64) -> DbFuture<'_, Vec<TileSetSummary>>;

    fn list_ingest_runs(&self, limit: i64, offset: i64) -> DbFuture<'_, Vec<IngestRunSummary>>;

    fn list_processing_runs(
        &self,
        limit: i64,
        offset: i64,
    ) -> DbFuture<'_, Vec<ProcessingRunSummary>>;

    fn processing_message_for_requeue(
        &self,
        ingest_id: Uuid,
    ) -> DbFuture<'_, ProcessingRequeueRecord>;
}

#[derive(Clone)]
pub struct GatewayDatabase {
    pool: PgPool,
}

impl GatewayDatabase {
    pub fn new(config: &DatabaseConfig) -> Result<Self, DbError> {
        let pool =
            shared::postgres::connect_lazy_pg_pool(&config.database_url, config.max_connections)
                .map_err(DbError::Connect)?;

        Ok(Self { pool })
    }

    pub async fn check(&self) -> Result<(), DbError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map(|_| ())
            .map_err(|source| DbError::Query {
                operation: "database_health_check",
                source,
            })
    }

    pub async fn list_tile_sets(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TileSetSummary>, DbError> {
        sqlx::query_as::<_, TileSetSummary>(
            r#"
            SELECT
                id AS tile_set_id,
                dataset_date,
                classification_version,
                render_version,
                format,
                min_zoom,
                max_native_zoom,
                max_display_zoom,
                bounds,
                tile_count,
                manifest_blob_path,
                latest,
                product,
                cadence,
                tile_set_kind,
                product_latest,
                created_at
            FROM tile_sets
            WHERE retention_deleted_at IS NULL
            ORDER BY latest DESC, product_latest DESC, dataset_date DESC, created_at DESC, id DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|source| DbError::Query {
            operation: "list_tile_sets",
            source,
        })
    }

    pub async fn list_ingest_runs(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<IngestRunSummary>, DbError> {
        sqlx::query_as::<_, IngestRunSummary>(
            r#"
            SELECT
                id,
                product,
                granule_title,
                blob_path,
                tile_h,
                tile_v,
                granule_date,
                status,
                error_message,
                created_at,
                updated_at
            FROM ingest_log
            ORDER BY updated_at DESC, created_at DESC, id DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|source| DbError::Query {
            operation: "list_ingest_runs",
            source,
        })
    }

    pub async fn list_processing_runs(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ProcessingRunSummary>, DbError> {
        sqlx::query_as::<_, ProcessingRunSummary>(
            r#"
            SELECT
                id,
                ingest_id,
                source_blob_path,
                product,
                tile_h,
                tile_v,
                granule_date,
                status,
                attempts,
                cloud_fraction,
                valid_pixel_count,
                rejected_pixel_count,
                tile_set_id,
                metadata,
                error_message,
                started_at,
                completed_at,
                created_at,
                updated_at
            FROM processing_log
            ORDER BY updated_at DESC, created_at DESC, id DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|source| DbError::Query {
            operation: "list_processing_runs",
            source,
        })
    }

    pub async fn processing_message_for_requeue(
        &self,
        ingest_id: Uuid,
    ) -> Result<ProcessingRequeueRecord, DbError> {
        let record = sqlx::query_as::<_, ProcessingRequeueRecord>(
            r#"
            SELECT
                ingest_log.id AS ingest_id,
                ingest_log.blob_path,
                ingest_log.product,
                ingest_log.granule_date,
                ingest_log.tile_h,
                ingest_log.tile_v,
                processing_log.status AS processing_status
            FROM ingest_log
            LEFT JOIN processing_log ON processing_log.ingest_id = ingest_log.id
            WHERE ingest_log.id = $1
            "#,
        )
        .bind(ingest_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|source| DbError::Query {
            operation: "processing_message_for_requeue",
            source,
        })?;

        record.ok_or(DbError::NotFound)
    }
}

impl GatewayDatabaseClient for GatewayDatabase {
    fn check(&self) -> DbFuture<'_, ()> {
        Box::pin(GatewayDatabase::check(self))
    }

    fn list_tile_sets(&self, limit: i64, offset: i64) -> DbFuture<'_, Vec<TileSetSummary>> {
        Box::pin(GatewayDatabase::list_tile_sets(self, limit, offset))
    }

    fn list_ingest_runs(&self, limit: i64, offset: i64) -> DbFuture<'_, Vec<IngestRunSummary>> {
        Box::pin(GatewayDatabase::list_ingest_runs(self, limit, offset))
    }

    fn list_processing_runs(
        &self,
        limit: i64,
        offset: i64,
    ) -> DbFuture<'_, Vec<ProcessingRunSummary>> {
        Box::pin(GatewayDatabase::list_processing_runs(self, limit, offset))
    }

    fn processing_message_for_requeue(
        &self,
        ingest_id: Uuid,
    ) -> DbFuture<'_, ProcessingRequeueRecord> {
        Box::pin(GatewayDatabase::processing_message_for_requeue(
            self, ingest_id,
        ))
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TileSetSummary {
    pub tile_set_id: String,
    pub dataset_date: NaiveDate,
    pub classification_version: String,
    pub render_version: String,
    pub format: String,
    pub min_zoom: i16,
    pub max_native_zoom: i16,
    pub max_display_zoom: i16,
    pub bounds: Value,
    pub tile_count: i32,
    pub manifest_blob_path: String,
    pub latest: bool,
    pub product: Option<String>,
    pub cadence: Option<String>,
    pub tile_set_kind: String,
    pub product_latest: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IngestRunSummary {
    pub id: Uuid,
    pub product: String,
    pub granule_title: String,
    pub blob_path: String,
    pub tile_h: i16,
    pub tile_v: i16,
    pub granule_date: DateTime<Utc>,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProcessingRunSummary {
    pub id: Uuid,
    pub ingest_id: Uuid,
    pub source_blob_path: String,
    pub product: String,
    pub tile_h: i16,
    pub tile_v: i16,
    pub granule_date: DateTime<Utc>,
    pub status: String,
    pub attempts: i32,
    pub cloud_fraction: Option<f64>,
    pub valid_pixel_count: Option<i64>,
    pub rejected_pixel_count: Option<i64>,
    pub tile_set_id: Option<String>,
    pub metadata: Value,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProcessingRequeueRecord {
    pub ingest_id: Uuid,
    pub blob_path: String,
    pub product: String,
    pub granule_date: DateTime<Utc>,
    pub tile_h: i16,
    pub tile_v: i16,
    pub processing_status: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: failed to create PostgreSQL pool: {0}")]
    Connect(sqlx::Error),
    #[error("database error: record not found")]
    NotFound,
    #[error("database error: {operation} failed: {source}")]
    Query {
        operation: &'static str,
        source: sqlx::Error,
    },
}
