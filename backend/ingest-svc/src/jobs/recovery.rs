use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    models::ProcessingMessage,
    storage::{BlobStorageClient, QueueClient},
};

use super::{
    failures::IngestFailureContext,
    ingest_log::{
        create_pending_enqueue_outbox, mark_enqueued, mark_outbox_completed, mark_outbox_failed,
        mark_recovery_pending, mark_rejected, mark_validated, pending_enqueue_outbox,
        recoverable_raw_rows, replay_rejected,
    },
    validation::validate_raw_granule_bytes,
    IngestError,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RecoverySummary {
    pub raw_rows_examined: usize,
    pub raw_rows_validated: usize,
    pub raw_rows_rejected: usize,
    pub outbox_examined: usize,
    pub enqueued: usize,
    pub failed: usize,
}

pub async fn run_recovery(
    config: &AppConfig,
    pool: &PgPool,
) -> Result<RecoverySummary, IngestError> {
    let storage = BlobStorageClient::new(config)?;
    let queue = QueueClient::new(config)?;
    let mut summary = RecoverySummary::default();

    for row in recoverable_raw_rows(pool).await? {
        summary.raw_rows_examined += 1;

        let bytes = match storage.download_raw_blob(&row.blob_path).await {
            Ok(bytes) => bytes,
            Err(error) => {
                let failure = IngestFailureContext::storage_download(&error).database_message();
                mark_recovery_pending(pool, row.id).await?;
                tracing::warn!(
                    ingest_id = %row.id,
                    blob_path = row.blob_path,
                    status = row.status,
                    error_message = %failure,
                    "recoverable raw blob row remains pending"
                );
                summary.failed += 1;
                continue;
            }
        };

        if let Err(error) = validate_raw_granule_bytes(&bytes) {
            let failure = IngestFailureContext::raw_validation(&error).database_message();
            mark_rejected(pool, row.id, &failure).await?;
            summary.raw_rows_rejected += 1;
            continue;
        }

        mark_validated(pool, row.id).await?;
        create_pending_enqueue_outbox(pool, row.id, "recover_validated_raw_blob").await?;
        summary.raw_rows_validated += 1;
    }

    enqueue_pending_outbox(config, pool, &queue, &mut summary).await?;

    tracing::info!(
        raw_rows_examined = summary.raw_rows_examined,
        raw_rows_validated = summary.raw_rows_validated,
        raw_rows_rejected = summary.raw_rows_rejected,
        outbox_examined = summary.outbox_examined,
        enqueued = summary.enqueued,
        failed = summary.failed,
        "ingest recovery completed"
    );

    Ok(summary)
}

pub async fn replay_rejected_granule(
    config: &AppConfig,
    pool: &PgPool,
    ingest_id: Uuid,
) -> Result<RecoverySummary, IngestError> {
    if !replay_rejected(pool, ingest_id, "operator_replay_rejected").await? {
        return Err(IngestError::ReplayNotRejected { ingest_id });
    }

    let queue = QueueClient::new(config)?;
    let mut summary = RecoverySummary::default();
    enqueue_pending_outbox(config, pool, &queue, &mut summary).await?;

    tracing::info!(
        ingest_id = %ingest_id,
        enqueued = summary.enqueued,
        failed = summary.failed,
        "rejected ingest replay completed"
    );

    Ok(summary)
}

pub async fn enqueue_pending_outbox(
    config: &AppConfig,
    pool: &PgPool,
    queue: &QueueClient,
    summary: &mut RecoverySummary,
) -> Result<(), IngestError> {
    for record in pending_enqueue_outbox(pool).await? {
        summary.outbox_examined += 1;

        let message = match ProcessingMessage::new(
            record.ingest_id,
            record.blob_path,
            record.product,
            record.granule_date,
            record.tile_h,
            record.tile_v,
        ) {
            Ok(message) => message,
            Err(error) => {
                mark_outbox_failed(pool, record.id, &error.to_string()).await?;
                summary.failed += 1;
                continue;
            }
        };

        if let Err(error) = queue
            .enqueue_processing_message(&config.azure_queue_name, &message)
            .await
        {
            let failure = IngestFailureContext::queue_enqueue(&error).database_message();
            mark_outbox_failed(pool, record.id, &failure).await?;
            summary.failed += 1;
            continue;
        }

        mark_enqueued(pool, record.ingest_id).await?;
        mark_outbox_completed(pool, record.id).await?;
        summary.enqueued += 1;
    }

    Ok(())
}
