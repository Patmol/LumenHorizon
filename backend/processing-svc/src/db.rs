use chrono::{DateTime, NaiveDate, Utc};
use shared::processing_message::ProductCadence;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{manifest::TileManifest, models::ProcessingMessage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessingLogRecord {
    pub id: Uuid,
    pub attempts: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlobRetentionCandidate {
    pub blob_path: String,
    pub oldest_created_at: DateTime<Utc>,
    pub record_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileSetRetentionCandidate {
    pub tile_set_id: String,
    pub classification_version: String,
    pub manifest_blob_path: String,
    pub created_at: DateTime<Utc>,
    pub tile_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MosaicSourceRecord {
    pub ingest_id: Uuid,
    pub blob_path: String,
    pub product: String,
    pub granule_date: DateTime<Utc>,
    pub tile_h: i16,
    pub tile_v: i16,
    pub tile_set_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileSetKind {
    Granule,
    Mosaic,
}

impl TileSetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Granule => "granule",
            Self::Mosaic => "mosaic",
        }
    }
}

pub struct TileSetInsert<'a> {
    pub manifest: &'a TileManifest,
    pub product: Option<&'a str>,
    pub cadence: Option<ProductCadence>,
    pub kind: TileSetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionEventMode {
    DryRun,
    Execute,
}

impl RetentionEventMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry_run",
            Self::Execute => "execute",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionTargetKind {
    RawBlob,
    ProcessedTile,
    ProcessedManifest,
    TileSet,
}

impl RetentionTargetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::RawBlob => "raw_blob",
            Self::ProcessedTile => "processed_tile",
            Self::ProcessedManifest => "processed_manifest",
            Self::TileSet => "tile_set",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionEventAction {
    Selected,
    Deleted,
    Missing,
    Skipped,
}

impl RetentionEventAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::Deleted => "deleted",
            Self::Missing => "missing",
            Self::Skipped => "skipped",
        }
    }
}

pub struct RetentionEvent<'a> {
    pub cleanup_run_id: Uuid,
    pub mode: RetentionEventMode,
    pub target_kind: RetentionTargetKind,
    pub target_identifier: &'a str,
    pub blob_container: Option<&'a str>,
    pub blob_path: Option<&'a str>,
    pub action: RetentionEventAction,
    pub reason: &'a str,
}

pub async fn connect(database_url: &str) -> Result<PgPool, DbError> {
    shared::postgres::connect_pg_pool(database_url, shared::postgres::DEFAULT_MAX_CONNECTIONS)
        .await
        .map_err(DbError::Connect)
}

pub async fn select_raw_blob_retention_candidates(
    pool: &PgPool,
    raw_retention_days: u32,
    batch_limit: u32,
) -> Result<Vec<RawBlobRetentionCandidate>, DbError> {
    sqlx::query_as::<_, (String, DateTime<Utc>, i64)>(
        r#"
        SELECT
            ingest_log.blob_path,
            min(ingest_log.created_at) AS oldest_created_at,
            count(*) AS record_count
        FROM ingest_log
        WHERE ingest_log.created_at < now() - ($1::bigint * interval '1 day')
          AND ingest_log.status IN (
              'downloading',
              'downloaded',
              'validated',
              'enqueued',
              'rejected',
              'failed',
              'recovery_pending',
              'replay_pending'
          )
          AND NOT EXISTS (
              SELECT 1
              FROM retention_cleanup_events
              WHERE retention_cleanup_events.target_kind = 'raw_blob'
                AND retention_cleanup_events.target_identifier = ingest_log.blob_path
                AND retention_cleanup_events.mode = 'execute'
                AND retention_cleanup_events.action IN ('deleted', 'missing')
          )
        GROUP BY ingest_log.blob_path
        ORDER BY min(ingest_log.created_at), ingest_log.blob_path
        LIMIT $2
        "#,
    )
    .bind(i64::from(raw_retention_days))
    .bind(i64::from(batch_limit))
    .fetch_all(pool)
    .await
    .map_err(DbError::SelectRetentionCandidates)
    .map(|rows| {
        rows.into_iter()
            .map(|row| RawBlobRetentionCandidate {
                blob_path: row.0,
                oldest_created_at: row.1,
                record_count: row.2,
            })
            .collect()
    })
}

pub async fn select_tile_set_retention_candidates(
    pool: &PgPool,
    processed_retention_days: u32,
    protected_prior_tile_sets: u32,
    batch_limit: u32,
) -> Result<Vec<TileSetRetentionCandidate>, DbError> {
    sqlx::query_as::<_, (String, String, String, DateTime<Utc>, i32)>(
        r#"
        WITH ranked_tile_sets AS (
            SELECT
                id,
                row_number() OVER (
                    PARTITION BY classification_version
                    ORDER BY created_at DESC, id DESC
                ) AS retention_rank
            FROM tile_sets
            WHERE retention_deleted_at IS NULL
        )
        SELECT
            tile_sets.id,
            tile_sets.classification_version,
            tile_sets.manifest_blob_path,
            tile_sets.created_at,
            tile_sets.tile_count
        FROM tile_sets
        JOIN ranked_tile_sets ON ranked_tile_sets.id = tile_sets.id
        WHERE tile_sets.retention_deleted_at IS NULL
          AND tile_sets.latest = false
                    AND tile_sets.product_latest = false
          AND tile_sets.created_at < now() - ($1::bigint * interval '1 day')
          AND ranked_tile_sets.retention_rank > ($2::bigint + 1)
        ORDER BY tile_sets.created_at, tile_sets.id
        LIMIT $3
        "#,
    )
    .bind(i64::from(processed_retention_days))
    .bind(i64::from(protected_prior_tile_sets))
    .bind(i64::from(batch_limit))
    .fetch_all(pool)
    .await
    .map_err(DbError::SelectRetentionCandidates)
    .map(|rows| {
        rows.into_iter()
            .map(|row| TileSetRetentionCandidate {
                tile_set_id: row.0,
                classification_version: row.1,
                manifest_blob_path: row.2,
                created_at: row.3,
                tile_count: row.4,
            })
            .collect()
    })
}

pub async fn mark_tile_set_retention_deleted(
    pool: &PgPool,
    tile_set_id: &str,
    reason: &str,
) -> Result<(), DbError> {
    let result = sqlx::query(
        r#"
        UPDATE tile_sets
        SET
            retention_deleted_at = now(),
            retention_delete_reason = $2
        WHERE id = $1
          AND latest = false
                    AND product_latest = false
          AND retention_deleted_at IS NULL
        "#,
    )
    .bind(tile_set_id)
    .bind(reason)
    .execute(pool)
    .await
    .map_err(DbError::UpdateRetentionMetadata)?;

    if result.rows_affected() != 1 {
        return Err(DbError::TileSetNotFound {
            tile_set_id: tile_set_id.to_owned(),
        });
    }

    Ok(())
}

pub async fn record_retention_event(
    pool: &PgPool,
    event: RetentionEvent<'_>,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        INSERT INTO retention_cleanup_events (
            id,
            cleanup_run_id,
            mode,
            target_kind,
            target_identifier,
            blob_container,
            blob_path,
            action,
            reason
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(event.cleanup_run_id)
    .bind(event.mode.as_str())
    .bind(event.target_kind.as_str())
    .bind(event.target_identifier)
    .bind(event.blob_container)
    .bind(event.blob_path)
    .bind(event.action.as_str())
    .bind(event.reason)
    .execute(pool)
    .await
    .map_err(DbError::InsertRetentionEvent)?;

    Ok(())
}

pub async fn upsert_processing_started(
    pool: &PgPool,
    message: &ProcessingMessage,
) -> Result<ProcessingLogRecord, DbError> {
    let record = sqlx::query_as::<_, (Uuid, i32)>(
        r#"
        INSERT INTO processing_log (
            id,
            ingest_id,
            source_blob_path,
            product,
            tile_h,
            tile_v,
            granule_date,
            status,
            attempts,
            started_at,
            updated_at
        )
        SELECT
            $1,
            ingest_log.id,
            $3,
            $4,
            $5,
            $6,
            $7,
            'processing',
            1,
            now(),
            now()
        FROM ingest_log
        WHERE ingest_log.id = $2
        ON CONFLICT (ingest_id)
        DO UPDATE SET
            source_blob_path = EXCLUDED.source_blob_path,
            product = EXCLUDED.product,
            tile_h = EXCLUDED.tile_h,
            tile_v = EXCLUDED.tile_v,
            granule_date = EXCLUDED.granule_date,
            status = 'processing',
            attempts = processing_log.attempts + 1,
            started_at = COALESCE(processing_log.started_at, now()),
            completed_at = NULL,
            error_message = NULL,
            updated_at = now()
        RETURNING id, attempts
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(message.ingest_id)
    .bind(&message.blob_path)
    .bind(&message.product)
    .bind(message.tile_h)
    .bind(message.tile_v)
    .bind(message.granule_date)
    .fetch_optional(pool)
    .await
    .map_err(DbError::UpsertProcessingLog)?;

    let Some(record) = record else {
        return Err(DbError::MissingIngestLog {
            ingest_id: message.ingest_id,
        });
    };

    Ok(ProcessingLogRecord {
        id: record.0,
        attempts: record.1,
    })
}

pub async fn mark_processing_failed(
    pool: &PgPool,
    ingest_id: Uuid,
    error_message: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE processing_log
        SET
            status = 'failed',
            error_message = $2,
            completed_at = now(),
            updated_at = now()
        WHERE ingest_id = $1
        "#,
    )
    .bind(ingest_id)
    .bind(error_message)
    .execute(pool)
    .await
    .map_err(DbError::UpdateProcessingLog)?;

    Ok(())
}

pub async fn mark_processing_deadlettered(
    pool: &PgPool,
    ingest_id: Uuid,
    error_message: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE processing_log
        SET
            status = 'deadlettered',
            error_message = $2,
            completed_at = now(),
            updated_at = now()
        WHERE ingest_id = $1
        "#,
    )
    .bind(ingest_id)
    .bind(error_message)
    .execute(pool)
    .await
    .map_err(DbError::UpdateProcessingLog)?;

    Ok(())
}

pub async fn update_processing_metadata(
    pool: &PgPool,
    processing_log_id: Uuid,
    metadata: serde_json::Value,
    cloud_fraction: f64,
    valid_pixel_count: i64,
    rejected_pixel_count: i64,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE processing_log
        SET
            metadata = $2,
            cloud_fraction = $3,
            valid_pixel_count = $4,
            rejected_pixel_count = $5,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(processing_log_id)
    .bind(metadata)
    .bind(cloud_fraction)
    .bind(valid_pixel_count)
    .bind(rejected_pixel_count)
    .execute(pool)
    .await
    .map_err(DbError::UpdateProcessingLog)?;

    Ok(())
}

pub async fn mark_processing_rejected(
    pool: &PgPool,
    ingest_id: Uuid,
    reason: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE processing_log
        SET
            status = 'rejected',
            error_message = $2,
            completed_at = now(),
            updated_at = now()
        WHERE ingest_id = $1
        "#,
    )
    .bind(ingest_id)
    .bind(reason)
    .execute(pool)
    .await
    .map_err(DbError::UpdateProcessingLog)?;

    Ok(())
}

pub async fn mark_processing_processed_with_tile_set(
    pool: &PgPool,
    ingest_id: Uuid,
    tile_set_id: &str,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE processing_log
        SET
            status = 'processed',
            tile_set_id = $2,
            error_message = NULL,
            completed_at = now(),
            updated_at = now()
        WHERE ingest_id = $1
        "#,
    )
    .bind(ingest_id)
    .bind(tile_set_id)
    .execute(pool)
    .await
    .map_err(DbError::UpdateProcessingLog)?;

    Ok(())
}

pub async fn select_mosaic_sources(
    pool: &PgPool,
    product: &str,
    dataset_date: NaiveDate,
    classification_version: &str,
    render_version: &str,
) -> Result<Vec<MosaicSourceRecord>, DbError> {
    let rows = sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>, i16, i16, String)>(
        r#"
        SELECT
            processing_log.ingest_id,
            processing_log.source_blob_path,
            processing_log.product,
            processing_log.granule_date,
            processing_log.tile_h,
            processing_log.tile_v,
            processing_log.tile_set_id AS tile_set_id
        FROM processing_log
        JOIN tile_sets ON tile_sets.id = processing_log.tile_set_id
        WHERE processing_log.status = 'processed'
          AND processing_log.product = $1
          AND processing_log.granule_date::date = $2
          AND processing_log.tile_set_id IS NOT NULL
          AND tile_sets.classification_version = $3
          AND tile_sets.render_version = $4
          AND tile_sets.tile_set_kind = 'granule'
          AND tile_sets.retention_deleted_at IS NULL
        ORDER BY processing_log.tile_h, processing_log.tile_v, processing_log.ingest_id
        "#,
    )
    .bind(product)
    .bind(dataset_date)
    .bind(classification_version)
    .bind(render_version)
    .fetch_all(pool)
    .await
    .map_err(DbError::SelectMosaicSources)?;

    Ok(rows
        .into_iter()
        .map(
            |(ingest_id, blob_path, product, granule_date, tile_h, tile_v, tile_set_id)| {
                MosaicSourceRecord {
                    ingest_id,
                    blob_path,
                    product,
                    granule_date,
                    tile_h,
                    tile_v,
                    tile_set_id,
                }
            },
        )
        .collect())
}

pub async fn select_latest_mosaic_dataset_date(
    pool: &PgPool,
    product: &str,
    classification_version: &str,
    render_version: &str,
) -> Result<Option<NaiveDate>, DbError> {
    sqlx::query_scalar::<_, NaiveDate>(
        r#"
        SELECT processing_log.granule_date::date AS dataset_date
        FROM processing_log
        JOIN tile_sets ON tile_sets.id = processing_log.tile_set_id
        WHERE processing_log.status = 'processed'
          AND processing_log.product = $1
          AND processing_log.tile_set_id IS NOT NULL
          AND tile_sets.classification_version = $2
          AND tile_sets.render_version = $3
          AND tile_sets.tile_set_kind = 'granule'
          AND tile_sets.retention_deleted_at IS NULL
        GROUP BY processing_log.granule_date::date
        HAVING count(*) >= 2
        ORDER BY processing_log.granule_date::date DESC
        LIMIT 1
        "#,
    )
    .bind(product)
    .bind(classification_version)
    .bind(render_version)
    .fetch_optional(pool)
    .await
    .map_err(DbError::SelectMosaicSources)
}

pub async fn insert_tile_set_with_metadata(
    pool: &PgPool,
    insert: TileSetInsert<'_>,
) -> Result<(), DbError> {
    let manifest = insert.manifest;
    let tile_count =
        i32::try_from(manifest.tile_count).map_err(|_| DbError::TileCountTooLarge {
            tile_count: manifest.tile_count,
        })?;
    let bounds = serde_json::to_value(manifest.bounds).map_err(DbError::SerializeTileSetBounds)?;
    let manifest_blob_path = manifest.manifest_blob_path()?;
    let cadence = insert.cadence.map(ProductCadence::as_str);

    sqlx::query(
        r#"
        INSERT INTO tile_sets (
            id,
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
            product_latest
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, false, $12, $13, $14, false)
        "#,
    )
    .bind(&manifest.tile_set_id)
    .bind(manifest.dataset_date)
    .bind(&manifest.classification_version)
    .bind(&manifest.render_version)
    .bind(&manifest.format)
    .bind(i16::from(manifest.min_zoom))
    .bind(i16::from(manifest.max_native_zoom))
    .bind(i16::from(manifest.max_display_zoom))
    .bind(bounds)
    .bind(tile_count)
    .bind(manifest_blob_path)
    .bind(insert.product)
    .bind(cadence)
    .bind(insert.kind.as_str())
    .execute(pool)
    .await
    .map_err(DbError::InsertTileSet)?;

    Ok(())
}

pub async fn promote_product_latest_tile_set(
    pool: &PgPool,
    tile_set_id: &str,
) -> Result<(), DbError> {
    let mut transaction = pool.begin().await.map_err(DbError::BeginTransaction)?;

    let row = sqlx::query_as::<_, (Option<String>, String, String, String)>(
        r#"
        SELECT product, classification_version, render_version, tile_set_kind
        FROM tile_sets
        WHERE id = $1
          AND retention_deleted_at IS NULL
        "#,
    )
    .bind(tile_set_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(DbError::PromoteTileSet)?;

    let Some((product, classification_version, render_version, tile_set_kind)) = row else {
        return Err(DbError::TileSetNotFound {
            tile_set_id: tile_set_id.to_owned(),
        });
    };

    let Some(product) = product else {
        return Err(DbError::TileSetMissingProduct {
            tile_set_id: tile_set_id.to_owned(),
        });
    };

    if tile_set_kind != TileSetKind::Mosaic.as_str() {
        return Err(DbError::TileSetNotMosaic {
            tile_set_id: tile_set_id.to_owned(),
            tile_set_kind,
        });
    }

    sqlx::query(
        r#"
        UPDATE tile_sets
        SET product_latest = false
        WHERE product_latest = true
          AND product = $1
          AND classification_version = $2
          AND render_version = $3
        "#,
    )
    .bind(&product)
    .bind(&classification_version)
    .bind(&render_version)
    .execute(&mut *transaction)
    .await
    .map_err(DbError::PromoteTileSet)?;

    let result = sqlx::query(
        r#"
        UPDATE tile_sets
        SET product_latest = true
        WHERE id = $1
          AND retention_deleted_at IS NULL
        "#,
    )
    .bind(tile_set_id)
    .execute(&mut *transaction)
    .await
    .map_err(DbError::PromoteTileSet)?;

    if result.rows_affected() != 1 {
        return Err(DbError::TileSetNotFound {
            tile_set_id: tile_set_id.to_owned(),
        });
    }

    transaction
        .commit()
        .await
        .map_err(DbError::CommitTransaction)?;

    Ok(())
}

pub async fn promote_latest_tile_set(pool: &PgPool, tile_set_id: &str) -> Result<(), DbError> {
    let mut transaction = pool.begin().await.map_err(DbError::BeginTransaction)?;

    sqlx::query(
        r#"
        UPDATE tile_sets
        SET latest = false
        WHERE latest = true
        "#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(DbError::PromoteTileSet)?;

    let result = sqlx::query(
        r#"
        UPDATE tile_sets
        SET latest = true
        WHERE id = $1
        "#,
    )
    .bind(tile_set_id)
    .execute(&mut *transaction)
    .await
    .map_err(DbError::PromoteTileSet)?;

    if result.rows_affected() != 1 {
        return Err(DbError::TileSetNotFound {
            tile_set_id: tile_set_id.to_owned(),
        });
    }

    transaction
        .commit()
        .await
        .map_err(DbError::CommitTransaction)?;

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: failed to begin transaction: {0}")]
    BeginTransaction(sqlx::Error),
    #[error("database error: failed to commit transaction: {0}")]
    CommitTransaction(sqlx::Error),
    #[error("database error: failed to connect to PostgreSQL: {0}")]
    Connect(sqlx::Error),
    #[error("database error: failed to insert tile_set: {0}")]
    InsertTileSet(sqlx::Error),
    #[error("database error: failed to insert retention cleanup event: {0}")]
    InsertRetentionEvent(sqlx::Error),
    #[error(transparent)]
    Manifest(#[from] crate::manifest::ManifestError),
    #[error(
        "database error: processing queue message references missing ingest_log row: {ingest_id}"
    )]
    MissingIngestLog { ingest_id: Uuid },
    #[error("database error: failed to promote tile_set: {0}")]
    PromoteTileSet(sqlx::Error),
    #[error("database error: failed to serialize tile set bounds: {0}")]
    SerializeTileSetBounds(serde_json::Error),
    #[error("database error: failed to select retention candidates: {0}")]
    SelectRetentionCandidates(sqlx::Error),
    #[error("database error: failed to select mosaic sources: {0}")]
    SelectMosaicSources(sqlx::Error),
    #[error("database error: tile_count {tile_count} exceeds PostgreSQL integer range")]
    TileCountTooLarge { tile_count: u32 },
    #[error("database error: tile_set '{tile_set_id}' does not exist")]
    TileSetNotFound { tile_set_id: String },
    #[error("database error: tile_set '{tile_set_id}' does not have product metadata")]
    TileSetMissingProduct { tile_set_id: String },
    #[error(
        "database error: tile_set '{tile_set_id}' has kind '{tile_set_kind}', expected mosaic"
    )]
    TileSetNotMosaic {
        tile_set_id: String,
        tile_set_kind: String,
    },
    #[error("database error: failed to update processing_log: {0}")]
    UpdateProcessingLog(sqlx::Error),
    #[error("database error: failed to update retention metadata: {0}")]
    UpdateRetentionMetadata(sqlx::Error),
    #[error("database error: failed to upsert processing_log: {0}")]
    UpsertProcessingLog(sqlx::Error),
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    use sqlx::{PgPool, Row};
    use uuid::Uuid;

    use super::{
        insert_tile_set_with_metadata, mark_processing_deadlettered, mark_processing_failed,
        mark_processing_processed_with_tile_set, mark_processing_rejected, promote_latest_tile_set,
        promote_product_latest_tile_set, select_latest_mosaic_dataset_date,
        update_processing_metadata, upsert_processing_started, DbError, TileSetInsert, TileSetKind,
    };
    use crate::models::ProcessingMessage;
    use crate::{
        config::TileBounds,
        manifest::{TileManifest, TileManifestInput},
        tiles::GeographicBounds,
    };
    use shared::processing_message::ProductCadence;

    async fn insert_ingest_row(pool: &PgPool) -> Uuid {
        insert_ingest_row_for(
            pool,
            "VNP46A2",
            Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap(),
            11,
            6,
        )
        .await
    }

    async fn insert_ingest_row_for(
        pool: &PgPool,
        product: &str,
        granule_date: DateTime<Utc>,
        tile_h: i16,
        tile_v: i16,
    ) -> Uuid {
        let ingest_id = Uuid::new_v4();
        let dataset_date = granule_date.date_naive();
        let blob_path = format!("{product}/{dataset_date}/h{tile_h:02}v{tile_v:02}.h5");

        sqlx::query(
            r#"
            INSERT INTO ingest_log (
                id,
                product,
                granule_title,
                blob_path,
                tile_h,
                tile_v,
                granule_date,
                status,
                error_message
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'enqueued', NULL)
            "#,
        )
        .bind(ingest_id)
        .bind(product)
        .bind(format!(
            "{product}.A{}.h{tile_h:02}v{tile_v:02}.{ingest_id}.h5",
            granule_date.format("%Y%j")
        ))
        .bind(blob_path)
        .bind(tile_h)
        .bind(tile_v)
        .bind(granule_date)
        .execute(pool)
        .await
        .expect("insert sample ingest row");

        ingest_id
    }

    fn processing_message(ingest_id: Uuid) -> ProcessingMessage {
        processing_message_for(
            ingest_id,
            "VNP46A2",
            Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap(),
            11,
            6,
        )
    }

    fn processing_message_for(
        ingest_id: Uuid,
        product: &str,
        granule_date: DateTime<Utc>,
        tile_h: i16,
        tile_v: i16,
    ) -> ProcessingMessage {
        let dataset_date = granule_date.date_naive();

        ProcessingMessage::new(
            ingest_id,
            &format!("{product}/{dataset_date}/h{tile_h:02}v{tile_v:02}.h5"),
            product,
            granule_date,
            tile_h,
            tile_v,
        )
        .expect("sample processing message should be valid")
    }

    fn tile_manifest(tile_set_id: &str) -> TileManifest {
        tile_manifest_for(
            tile_set_id,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
            "radiance-dark-sky-v1",
            "tiles-v1",
        )
    }

    fn tile_manifest_for(
        tile_set_id: &str,
        dataset_date: NaiveDate,
        classification_version: &str,
        render_version: &str,
    ) -> TileManifest {
        let config = crate::config::AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some("test-key".to_owned()),
            _ => None,
        })
        .expect("test config");

        let mut manifest = TileManifest::from_config(
            &config,
            TileManifestInput {
                tile_set_id: tile_set_id.to_owned(),
                dataset_date,
                generated_at: Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
                processor_version: "processing-svc:test-sha".to_owned(),
                bounds: GeographicBounds::from(TileBounds {
                    west: -125.0,
                    south: 24.0,
                    east: -66.0,
                    north: 50.0,
                }),
                tile_count: 12345,
                source_granules: Vec::new(),
                coverage: None,
            },
        )
        .expect("tile manifest");
        manifest.classification_version = classification_version.to_owned();
        manifest.render_version = render_version.to_owned();

        manifest
    }

    async fn insert_granule_tile_set(pool: &PgPool, manifest: &TileManifest) {
        insert_tile_set_with_metadata(
            pool,
            TileSetInsert {
                manifest,
                product: Some("VNP46A2"),
                cadence: Some(ProductCadence::Daily),
                kind: TileSetKind::Granule,
            },
        )
        .await
        .expect("insert granule tile set");
    }

    async fn insert_processed_source(
        pool: &PgPool,
        product: &str,
        granule_date: DateTime<Utc>,
        tile_h: i16,
        tile_v: i16,
        classification_version: &str,
        render_version: &str,
        kind: TileSetKind,
    ) -> TileManifest {
        let ingest_id = insert_ingest_row_for(pool, product, granule_date, tile_h, tile_v).await;
        let message = processing_message_for(ingest_id, product, granule_date, tile_h, tile_v);
        let tile_set_id = format!(
            "{}-{classification_version}-{render_version}-h{tile_h:02}v{tile_v:02}-{}",
            granule_date.date_naive(),
            Uuid::new_v4().simple()
        );
        let manifest = tile_manifest_for(
            &tile_set_id,
            granule_date.date_naive(),
            classification_version,
            render_version,
        );

        upsert_processing_started(pool, &message)
            .await
            .expect("create processing_log row");
        insert_tile_set_with_metadata(
            pool,
            TileSetInsert {
                manifest: &manifest,
                product: Some(product),
                cadence: Some(ProductCadence::Daily),
                kind,
            },
        )
        .await
        .expect("insert source tile set");
        mark_processing_processed_with_tile_set(pool, ingest_id, &manifest.tile_set_id)
            .await
            .expect("mark source processed");

        manifest
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn upsert_processing_started_is_idempotent_by_ingest_id(pool: PgPool) {
        let ingest_id = insert_ingest_row(&pool).await;
        let message = processing_message(ingest_id);

        let first = upsert_processing_started(&pool, &message)
            .await
            .expect("first processing_log upsert");
        let second = upsert_processing_started(&pool, &message)
            .await
            .expect("second processing_log upsert");

        assert_eq!(first.id, second.id);
        assert_eq!(first.attempts, 1);
        assert_eq!(second.attempts, 2);

        let row_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT count(*)
            FROM processing_log
            WHERE ingest_id = $1
            "#,
        )
        .bind(ingest_id)
        .fetch_one(&pool)
        .await
        .expect("load processing_log row count");

        let row = sqlx::query(
            r#"
            SELECT status, attempts
            FROM processing_log
            WHERE ingest_id = $1
            "#,
        )
        .bind(ingest_id)
        .fetch_one(&pool)
        .await
        .expect("load processing_log row");

        assert_eq!(row_count, 1);
        assert_eq!(row.get::<String, _>("status"), "processing");
        assert_eq!(row.get::<i32, _>("attempts"), 2);
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn upsert_processing_started_reports_missing_ingest_log(pool: PgPool) {
        let ingest_id = Uuid::new_v4();
        let message = processing_message(ingest_id);

        let error = upsert_processing_started(&pool, &message)
            .await
            .expect_err("missing ingest row should be reported explicitly");

        assert!(matches!(error, DbError::MissingIngestLog { ingest_id: id } if id == ingest_id));

        let row_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT count(*)
            FROM processing_log
            WHERE ingest_id = $1
            "#,
        )
        .bind(ingest_id)
        .fetch_one(&pool)
        .await
        .expect("load processing_log row count");

        assert_eq!(row_count, 0);
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn failure_status_updates_record_retry_and_deadletter_outcomes(pool: PgPool) {
        let ingest_id = insert_ingest_row(&pool).await;
        let message = processing_message(ingest_id);

        upsert_processing_started(&pool, &message)
            .await
            .expect("create processing_log row");
        mark_processing_failed(&pool, ingest_id, "retryable science error")
            .await
            .expect("mark processing failed");

        let failed = sqlx::query(
            r#"
            SELECT status, error_message
            FROM processing_log
            WHERE ingest_id = $1
            "#,
        )
        .bind(ingest_id)
        .fetch_one(&pool)
        .await
        .expect("load failed processing_log row");

        assert_eq!(failed.get::<String, _>("status"), "failed");
        assert_eq!(
            failed.get::<Option<String>, _>("error_message"),
            Some("retryable science error".to_owned())
        );

        upsert_processing_started(&pool, &message)
            .await
            .expect("restart processing after retry");
        mark_processing_deadlettered(&pool, ingest_id, "poison message")
            .await
            .expect("mark processing deadlettered");

        let deadlettered = sqlx::query(
            r#"
            SELECT status, error_message
            FROM processing_log
            WHERE ingest_id = $1
            "#,
        )
        .bind(ingest_id)
        .fetch_one(&pool)
        .await
        .expect("load deadlettered processing_log row");

        assert_eq!(deadlettered.get::<String, _>("status"), "deadlettered");
        assert_eq!(
            deadlettered.get::<Option<String>, _>("error_message"),
            Some("poison message".to_owned())
        );
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn update_processing_metadata_persists_json_metadata_and_summary_columns(pool: PgPool) {
        let ingest_id = insert_ingest_row(&pool).await;
        let message = processing_message(ingest_id);

        let processing_log = upsert_processing_started(&pool, &message)
            .await
            .expect("create processing_log row");

        let metadata = serde_json::json!({
            "quality_rule_version": "viirs-quality-v1",
            "quality_summary": {
                "total_pixel_count": 4,
                "valid_pixel_count": 3,
                "rejected_pixel_count": 1,
                "cloud_contaminated_valid_pixel_count": 2,
                "cloud_fraction": 0.6666667,
                "max_cloud_fraction": 0.5,
                "exceeds_max_cloud_fraction": true
            }
        });

        update_processing_metadata(&pool, processing_log.id, metadata.clone(), 0.6666667, 3, 1)
            .await
            .expect("update processing metadata");

        let stored = sqlx::query(
            r#"
            SELECT metadata, cloud_fraction, valid_pixel_count, rejected_pixel_count
            FROM processing_log
            WHERE id = $1
            "#,
        )
        .bind(processing_log.id)
        .fetch_one(&pool)
        .await
        .expect("load processing metadata");

        assert_eq!(stored.get::<serde_json::Value, _>("metadata"), metadata);
        assert_eq!(
            stored.get::<Option<f64>, _>("cloud_fraction"),
            Some(0.6666667)
        );
        assert_eq!(stored.get::<Option<i64>, _>("valid_pixel_count"), Some(3));
        assert_eq!(
            stored.get::<Option<i64>, _>("rejected_pixel_count"),
            Some(1)
        );
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn mark_processing_rejected_records_completed_science_rejection(pool: PgPool) {
        let ingest_id = insert_ingest_row(&pool).await;
        let message = processing_message(ingest_id);

        upsert_processing_started(&pool, &message)
            .await
            .expect("create processing_log row");

        mark_processing_rejected(
            &pool,
            ingest_id,
            "cloud fraction exceeds configured maximum",
        )
        .await
        .expect("mark processing rejected");

        let rejected = sqlx::query(
            r#"
            SELECT status, error_message, completed_at
            FROM processing_log
            WHERE ingest_id = $1
            "#,
        )
        .bind(ingest_id)
        .fetch_one(&pool)
        .await
        .expect("load rejected processing_log row");

        assert_eq!(rejected.get::<String, _>("status"), "rejected");
        assert_eq!(
            rejected.get::<Option<String>, _>("error_message"),
            Some("cloud fraction exceeds configured maximum".to_owned())
        );
        assert!(rejected
            .get::<Option<chrono::DateTime<Utc>>, _>("completed_at")
            .is_some());
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn mark_processing_processed_with_tile_set_records_output_reference(pool: PgPool) {
        let ingest_id = insert_ingest_row(&pool).await;
        let message = processing_message(ingest_id);
        let manifest = tile_manifest("2026-05-21-radiance-dark-sky-v1-a1b2c3d4");

        upsert_processing_started(&pool, &message)
            .await
            .expect("create processing_log row");
        insert_granule_tile_set(&pool, &manifest).await;
        mark_processing_processed_with_tile_set(&pool, ingest_id, &manifest.tile_set_id)
            .await
            .expect("mark processing processed with tile set");

        let processed = sqlx::query(
            r#"
            SELECT status, tile_set_id, error_message, completed_at
            FROM processing_log
            WHERE ingest_id = $1
            "#,
        )
        .bind(ingest_id)
        .fetch_one(&pool)
        .await
        .expect("load processed processing_log row");

        assert_eq!(processed.get::<String, _>("status"), "processed");
        assert_eq!(
            processed.get::<Option<String>, _>("tile_set_id"),
            Some(manifest.tile_set_id)
        );
        assert_eq!(processed.get::<Option<String>, _>("error_message"), None);
        assert!(processed
            .get::<Option<chrono::DateTime<Utc>>, _>("completed_at")
            .is_some());
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn latest_mosaic_dataset_date_uses_newest_eligible_processed_sources(pool: PgPool) {
        let target_date = Utc.with_ymd_and_hms(2026, 5, 22, 0, 0, 0).unwrap();
        let newer_but_single_source = Utc.with_ymd_and_hms(2026, 5, 23, 0, 0, 0).unwrap();

        insert_processed_source(
            &pool,
            "VNP46A2",
            target_date,
            11,
            6,
            "radiance-dark-sky-v1",
            "tiles-v1",
            TileSetKind::Granule,
        )
        .await;
        insert_processed_source(
            &pool,
            "VNP46A2",
            target_date,
            12,
            6,
            "radiance-dark-sky-v1",
            "tiles-v1",
            TileSetKind::Granule,
        )
        .await;
        insert_processed_source(
            &pool,
            "VNP46A2",
            newer_but_single_source,
            11,
            6,
            "radiance-dark-sky-v1",
            "tiles-v1",
            TileSetKind::Granule,
        )
        .await;
        insert_processed_source(
            &pool,
            "VNP46A2",
            newer_but_single_source,
            12,
            6,
            "radiance-dark-sky-v1",
            "tiles-v1",
            TileSetKind::Mosaic,
        )
        .await;
        insert_processed_source(
            &pool,
            "VNP46A2",
            newer_but_single_source,
            13,
            6,
            "radiance-dark-sky-v1",
            "tiles-v2",
            TileSetKind::Granule,
        )
        .await;

        let latest_date =
            select_latest_mosaic_dataset_date(&pool, "VNP46A2", "radiance-dark-sky-v1", "tiles-v1")
                .await
                .expect("select latest mosaic dataset date");

        assert_eq!(latest_date, Some(target_date.date_naive()));
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn tile_set_insert_and_latest_promotion_are_transactional(pool: PgPool) {
        let first = tile_manifest("2026-05-21-radiance-dark-sky-v1-a1b2c3d4");
        let second = tile_manifest("2026-05-22-radiance-dark-sky-v1-deadbeef");

        insert_granule_tile_set(&pool, &first).await;
        insert_granule_tile_set(&pool, &second).await;

        promote_latest_tile_set(&pool, &first.tile_set_id)
            .await
            .expect("promote first tile set");
        promote_latest_tile_set(&pool, &second.tile_set_id)
            .await
            .expect("promote second tile set");

        let latest_rows = sqlx::query(
            r#"
        SELECT id, latest
        FROM tile_sets
        WHERE latest = true
        "#,
        )
        .fetch_all(&pool)
        .await
        .expect("load latest tile sets");

        assert_eq!(latest_rows.len(), 1);
        assert_eq!(latest_rows[0].get::<String, _>("id"), second.tile_set_id);
        assert!(latest_rows[0].get::<bool, _>("latest"));
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn product_latest_promotion_is_scoped_by_product(pool: PgPool) {
        let first_daily = tile_manifest("2026-05-21-radiance-dark-sky-v1-daily111");
        let second_daily = tile_manifest("2026-05-22-radiance-dark-sky-v1-daily222");
        let monthly = tile_manifest("2026-05-01-radiance-dark-sky-v1-month111");

        for manifest in [&first_daily, &second_daily] {
            insert_tile_set_with_metadata(
                &pool,
                TileSetInsert {
                    manifest,
                    product: Some("VNP46A2"),
                    cadence: Some(ProductCadence::Daily),
                    kind: TileSetKind::Mosaic,
                },
            )
            .await
            .expect("insert daily mosaic tile set");
        }
        insert_tile_set_with_metadata(
            &pool,
            TileSetInsert {
                manifest: &monthly,
                product: Some("VNP46A3"),
                cadence: Some(ProductCadence::Monthly),
                kind: TileSetKind::Mosaic,
            },
        )
        .await
        .expect("insert monthly mosaic tile set");

        promote_product_latest_tile_set(&pool, &first_daily.tile_set_id)
            .await
            .expect("promote first daily mosaic");
        promote_product_latest_tile_set(&pool, &monthly.tile_set_id)
            .await
            .expect("promote monthly mosaic");
        promote_product_latest_tile_set(&pool, &second_daily.tile_set_id)
            .await
            .expect("promote second daily mosaic");

        let product_latest_rows = sqlx::query(
            r#"
            SELECT id, product, product_latest, latest
            FROM tile_sets
            WHERE product_latest = true
            ORDER BY product, id
            "#,
        )
        .fetch_all(&pool)
        .await
        .expect("load product latest tile sets");

        assert_eq!(product_latest_rows.len(), 2);
        assert_eq!(
            product_latest_rows[0].get::<String, _>("id"),
            second_daily.tile_set_id
        );
        assert_eq!(
            product_latest_rows[0].get::<String, _>("product"),
            "VNP46A2"
        );
        assert!(product_latest_rows[0].get::<bool, _>("product_latest"));
        assert!(!product_latest_rows[0].get::<bool, _>("latest"));
        assert_eq!(
            product_latest_rows[1].get::<String, _>("id"),
            monthly.tile_set_id
        );
        assert_eq!(
            product_latest_rows[1].get::<String, _>("product"),
            "VNP46A3"
        );
        assert!(product_latest_rows[1].get::<bool, _>("product_latest"));
        assert!(!product_latest_rows[1].get::<bool, _>("latest"));
    }

    #[sqlx::test(migrations = "../db-migrate/migrations")]
    async fn product_latest_promotion_rejects_granule_tile_sets(pool: PgPool) {
        let granule = tile_manifest("2026-05-21-radiance-dark-sky-v1-granule1");
        insert_tile_set_with_metadata(
            &pool,
            TileSetInsert {
                manifest: &granule,
                product: Some("VNP46A2"),
                cadence: Some(ProductCadence::Daily),
                kind: TileSetKind::Granule,
            },
        )
        .await
        .expect("insert granule tile set");

        let error = promote_product_latest_tile_set(&pool, &granule.tile_set_id)
            .await
            .expect_err("granules must not become product latest");

        assert!(matches!(
            error,
            DbError::TileSetNotMosaic {
                tile_set_id,
                tile_set_kind,
            } if tile_set_id == granule.tile_set_id && tile_set_kind == "granule"
        ));

        let product_latest = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT product_latest
            FROM tile_sets
            WHERE id = $1
            "#,
        )
        .bind(&granule.tile_set_id)
        .fetch_one(&pool)
        .await
        .expect("load granule latest flag");

        assert!(!product_latest);
    }
}
