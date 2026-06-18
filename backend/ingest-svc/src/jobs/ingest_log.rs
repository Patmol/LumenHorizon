use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    GranuleCandidate, INGEST_STATUS_DOWNLOADED, INGEST_STATUS_DOWNLOADING, INGEST_STATUS_ENQUEUED,
    INGEST_STATUS_FAILED, INGEST_STATUS_RECOVERY_PENDING, INGEST_STATUS_REJECTED,
    INGEST_STATUS_REPLAY_PENDING, INGEST_STATUS_VALIDATED,
};

const OUTBOX_MAX_ATTEMPTS: i32 = 3;

pub(super) async fn get_discovery_resume_points_for_products(
    pool: &PgPool,
    products: &[String],
) -> Result<HashMap<String, DateTime<Utc>>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        r#"
        SELECT product, MAX(granule_date) AS resume_point
        FROM ingest_log
        WHERE status NOT IN ($1, $2)
          AND (cardinality($3::text[]) = 0 OR product = ANY($3))
        GROUP BY product
        "#,
    )
    .bind(INGEST_STATUS_FAILED)
    .bind(INGEST_STATUS_REJECTED)
    .bind(products)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InsertDownloadingRowOutcome {
    Created(Uuid),
    AlreadyExists,
}

pub(super) async fn insert_downloading_row(
    pool: &PgPool,
    granule: &GranuleCandidate,
    blob_path: &str,
) -> Result<InsertDownloadingRowOutcome, sqlx::Error> {
    let ingest_id = Uuid::new_v4();

    let inserted_id = sqlx::query_scalar::<_, Uuid>(
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
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL)
        ON CONFLICT ON CONSTRAINT uq_ingest_log_product_tile_date DO NOTHING
        RETURNING id
        "#,
    )
    .bind(ingest_id)
    .bind(&granule.product)
    .bind(&granule.title)
    .bind(blob_path)
    .bind(i16::from(granule.tile.h))
    .bind(i16::from(granule.tile.v))
    .bind(granule.granule_date)
    .bind(INGEST_STATUS_DOWNLOADING)
    .fetch_optional(pool)
    .await?;

    Ok(match inserted_id {
        Some(ingest_id) => InsertDownloadingRowOutcome::Created(ingest_id),
        None => InsertDownloadingRowOutcome::AlreadyExists,
    })
}

pub(super) async fn mark_downloaded(pool: &PgPool, ingest_id: Uuid) -> Result<(), sqlx::Error> {
    mark_success_status(pool, ingest_id, INGEST_STATUS_DOWNLOADED).await
}

pub(super) async fn mark_validated(pool: &PgPool, ingest_id: Uuid) -> Result<(), sqlx::Error> {
    mark_success_status(pool, ingest_id, INGEST_STATUS_VALIDATED).await
}

pub(super) async fn mark_recovery_pending(
    pool: &PgPool,
    ingest_id: Uuid,
) -> Result<(), sqlx::Error> {
    mark_success_status(pool, ingest_id, INGEST_STATUS_RECOVERY_PENDING).await
}

pub(super) async fn mark_enqueued(pool: &PgPool, ingest_id: Uuid) -> Result<(), sqlx::Error> {
    mark_success_status(pool, ingest_id, INGEST_STATUS_ENQUEUED).await
}

async fn mark_success_status(
    pool: &PgPool,
    ingest_id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE ingest_log
        SET
            status = $2,
            error_message = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(ingest_id)
    .bind(status)
    .execute(pool)
    .await?;

    Ok(())
}

pub(super) async fn mark_failed(
    pool: &PgPool,
    ingest_id: Uuid,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    mark_failed_status(pool, ingest_id, INGEST_STATUS_FAILED, error_message).await
}

pub(super) async fn mark_rejected(
    pool: &PgPool,
    ingest_id: Uuid,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    mark_failed_status(pool, ingest_id, INGEST_STATUS_REJECTED, error_message).await
}

async fn mark_failed_status(
    pool: &PgPool,
    ingest_id: Uuid,
    status: &str,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE ingest_log
        SET
            status = $2,
            error_message = $3,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(ingest_id)
    .bind(status)
    .bind(truncate_error_message(error_message))
    .execute(pool)
    .await?;

    Ok(())
}

pub(super) fn truncate_error_message(message: &str) -> String {
    const MAX_ERROR_MESSAGE_LENGTH: usize = 1_000;

    message.chars().take(MAX_ERROR_MESSAGE_LENGTH).collect()
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct IngestRecoveryRecord {
    pub id: Uuid,
    pub blob_path: String,
    pub status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct PendingOutboxRecord {
    pub id: Uuid,
    pub ingest_id: Uuid,
    pub product: String,
    pub blob_path: String,
    pub granule_date: DateTime<Utc>,
    pub tile_h: i16,
    pub tile_v: i16,
}

pub(super) async fn recoverable_raw_rows(
    pool: &PgPool,
) -> Result<Vec<IngestRecoveryRecord>, sqlx::Error> {
    sqlx::query_as::<_, IngestRecoveryRecord>(
        r#"
        SELECT id, blob_path, status
        FROM ingest_log
        WHERE status IN ($1, $2, $3)
        ORDER BY updated_at ASC, created_at ASC
        LIMIT 100
        "#,
    )
    .bind(INGEST_STATUS_DOWNLOADING)
    .bind(INGEST_STATUS_DOWNLOADED)
    .bind(INGEST_STATUS_RECOVERY_PENDING)
    .fetch_all(pool)
    .await
}

pub(super) async fn create_pending_enqueue_outbox(
    pool: &PgPool,
    ingest_id: Uuid,
    reason: &str,
) -> Result<Uuid, sqlx::Error> {
    let outbox_id = Uuid::new_v4();
    sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO ingest_recovery_outbox (
            id,
            ingest_id,
            operation,
            status,
            reason,
            error_message
        )
        VALUES ($1, $2, 'enqueue_processing', 'pending', $3, NULL)
        -- Keep one retryable pending enqueue record while preserving completed/failed history.
        ON CONFLICT (ingest_id, operation) WHERE status = 'pending'
        DO UPDATE SET
            reason = EXCLUDED.reason,
            updated_at = now(),
            error_message = NULL
        RETURNING id
        "#,
    )
    .bind(outbox_id)
    .bind(ingest_id)
    .bind(reason)
    .fetch_one(pool)
    .await
}

pub(super) async fn pending_enqueue_outbox(
    pool: &PgPool,
) -> Result<Vec<PendingOutboxRecord>, sqlx::Error> {
    sqlx::query_as::<_, PendingOutboxRecord>(
        r#"
        SELECT
            ingest_recovery_outbox.id,
            ingest_log.id AS ingest_id,
            ingest_log.product,
            ingest_log.blob_path,
            ingest_log.granule_date,
            ingest_log.tile_h,
            ingest_log.tile_v
        FROM ingest_recovery_outbox
        JOIN ingest_log ON ingest_log.id = ingest_recovery_outbox.ingest_id
        WHERE ingest_recovery_outbox.operation = 'enqueue_processing'
          AND ingest_recovery_outbox.status = 'pending'
          AND ingest_log.status IN ($1, $2)
        ORDER BY ingest_recovery_outbox.created_at ASC
        LIMIT 100
        "#,
    )
    .bind(INGEST_STATUS_VALIDATED)
    .bind(INGEST_STATUS_REPLAY_PENDING)
    .fetch_all(pool)
    .await
}

pub(super) async fn mark_outbox_completed(
    pool: &PgPool,
    outbox_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE ingest_recovery_outbox
        SET status = 'completed',
            error_message = NULL,
            completed_at = now(),
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(outbox_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub(super) async fn mark_outbox_failed(
    pool: &PgPool,
    outbox_id: Uuid,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE ingest_recovery_outbox
        SET attempts = attempts + 1,
            status = CASE
                WHEN attempts + 1 >= $3 THEN 'failed'
                ELSE status
            END,
            error_message = $2,
            completed_at = CASE
                WHEN attempts + 1 >= $3 THEN now()
                ELSE completed_at
            END,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(outbox_id)
    .bind(truncate_error_message(error_message))
    .bind(OUTBOX_MAX_ATTEMPTS)
    .execute(pool)
    .await?;

    Ok(())
}

pub(super) async fn replay_rejected(
    pool: &PgPool,
    ingest_id: Uuid,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        r#"
        UPDATE ingest_log
        SET status = $2,
            error_message = NULL,
            updated_at = now()
        WHERE id = $1
          AND status = $3
        "#,
    )
    .bind(ingest_id)
    .bind(INGEST_STATUS_REPLAY_PENDING)
    .bind(INGEST_STATUS_REJECTED)
    .execute(pool)
    .await?
    .rows_affected();

    if updated == 0 {
        return Ok(false);
    }

    create_pending_enqueue_outbox(pool, ingest_id, reason).await?;

    Ok(true)
}
