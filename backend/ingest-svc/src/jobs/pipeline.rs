use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    earthdata::EarthdataClient,
    models::{processing_message_from_granule, GranuleCandidate},
    storage::{BlobStorageClient, QueueClient},
};

use super::{
    failures::{FailurePhase, IngestFailureContext},
    ingest_log::{
        create_pending_enqueue_outbox, insert_downloading_row, mark_downloaded, mark_enqueued,
        mark_failed, mark_outbox_completed, mark_rejected, mark_validated,
        InsertDownloadingRowOutcome,
    },
    summary::GranuleProcessingOutcome,
    validation::validate_raw_granule_bytes,
};

pub(super) async fn process_granule(
    config: &AppConfig,
    pool: &PgPool,
    earthdata: &EarthdataClient,
    storage: &BlobStorageClient,
    queue: &QueueClient,
    correlation_id: Uuid,
    granule: &GranuleCandidate,
) -> GranuleProcessingOutcome {
    let blob_path = granule.raw_blob_path();
    let span = tracing::info_span!(
        "granule_ingest",
        service = crate::observability::SERVICE_NAME,
        service_version = crate::observability::SERVICE_VERSION,
        command = "ingest",
        correlation_id = %correlation_id,
        product = granule.product,
        granule_title = granule.title,
        granule_date = %granule.granule_date,
        tile_h = granule.tile.h,
        tile_v = granule.tile.v,
        blob_path,
        ingest_id = tracing::field::Empty,
        final_status = tracing::field::Empty,
    );

    process_granule_inner(config, pool, earthdata, storage, queue, granule, blob_path)
        .instrument(span)
        .await
}

async fn process_granule_inner(
    config: &AppConfig,
    pool: &PgPool,
    earthdata: &EarthdataClient,
    storage: &BlobStorageClient,
    queue: &QueueClient,
    granule: &GranuleCandidate,
    blob_path: String,
) -> GranuleProcessingOutcome {
    let ingest_id = match insert_downloading_row(pool, granule, &blob_path).await {
        Ok(InsertDownloadingRowOutcome::Created(ingest_id)) => ingest_id,
        Ok(InsertDownloadingRowOutcome::AlreadyExists) => {
            tracing::info!(
                product = granule.product,
                granule_title = granule.title,
                granule_date = %granule.granule_date,
                tile_h = granule.tile.h,
                tile_v = granule.tile.v,
                blob_path,
                "ingest log row already exists; skipping granule"
            );

            record_final_status("skipped");

            return GranuleProcessingOutcome::Skipped;
        }
        Err(error) => {
            tracing::warn!(
                product = granule.product,
                granule_title = granule.title,
                granule_date = %granule.granule_date,
                tile_h = granule.tile.h,
                tile_v = granule.tile.v,
                blob_path,
                failure_phase = FailurePhase::RecordDownloading.as_str(),
                error_code = "ingest_log_insert_failed",
                error_category = "database",
                retry_eligible = true,
                http_status = ?Option::<u16>::None,
                error = %error,
                "failed to create ingest log row; skipping granule"
            );

            record_final_status("skipped");

            return GranuleProcessingOutcome::Skipped;
        }
    };
    tracing::Span::current().record("ingest_id", tracing::field::display(ingest_id));

    tracing::info!(
        product = granule.product,
        granule_title = granule.title,
        granule_date = %granule.granule_date,
        tile_h = granule.tile.h,
        tile_v = granule.tile.v,
        blob_path,
        ingest_id = %ingest_id,
        "starting granule download and raw upload"
    );

    let bytes = match earthdata.download(granule).await {
        Ok(bytes) => bytes,
        Err(error) => {
            let failure = IngestFailureContext::earthdata_download(&error);
            let error_message = failure.database_message();

            if let Err(update_error) = mark_failed(pool, ingest_id, &error_message).await {
                log_failure_status_update_error(
                    granule,
                    &blob_path,
                    ingest_id,
                    &failure,
                    &update_error,
                    "failed to mark ingest row failed after download error",
                );
            }

            log_granule_failure(
                granule,
                &blob_path,
                ingest_id,
                None,
                &failure,
                &error,
                "Earthdata download failed",
            );

            record_final_status("failed");

            return GranuleProcessingOutcome::FailedBeforeDownloaded;
        }
    };

    if let Err(error) = storage.upload_raw_blob(&blob_path, &bytes).await {
        let failure = IngestFailureContext::storage_upload(&error);
        let error_message = failure.database_message();

        if let Err(update_error) = mark_failed(pool, ingest_id, &error_message).await {
            log_failure_status_update_error(
                granule,
                &blob_path,
                ingest_id,
                &failure,
                &update_error,
                "failed to mark ingest row failed after upload error",
            );
        }

        log_granule_failure(
            granule,
            &blob_path,
            ingest_id,
            None,
            &failure,
            &error,
            "raw blob upload failed",
        );

        record_final_status("failed");

        return GranuleProcessingOutcome::FailedBeforeDownloaded;
    }

    if let Err(error) = mark_downloaded(pool, ingest_id).await {
        let failure = IngestFailureContext::database_status_update(
            FailurePhase::MarkDownloaded,
            "mark_downloaded_failed",
            &error,
        );

        log_granule_failure(
            granule,
            &blob_path,
            ingest_id,
            None,
            &failure,
            &error,
            "failed to mark ingest row downloaded",
        );

        record_final_status("failed");

        return GranuleProcessingOutcome::FailedBeforeDownloaded;
    }

    if let Err(error) = validate_raw_granule_bytes(&bytes) {
        let failure = IngestFailureContext::raw_validation(&error);
        let error_message = failure.database_message();

        if let Err(update_error) = mark_rejected(pool, ingest_id, &error_message).await {
            log_failure_status_update_error(
                granule,
                &blob_path,
                ingest_id,
                &failure,
                &update_error,
                "failed to mark ingest row rejected after validation error",
            );
        }

        log_granule_failure(
            granule,
            &blob_path,
            ingest_id,
            None,
            &failure,
            &error,
            "raw granule validation rejected file",
        );

        record_final_status("rejected");

        return GranuleProcessingOutcome::RejectedAfterDownloaded;
    }

    if let Err(error) = mark_validated(pool, ingest_id).await {
        let failure = IngestFailureContext::database_status_update(
            FailurePhase::MarkValidated,
            "mark_validated_failed",
            &error,
        );

        log_granule_failure(
            granule,
            &blob_path,
            ingest_id,
            None,
            &failure,
            &error,
            "failed to mark ingest row validated",
        );

        record_final_status("failed");

        return GranuleProcessingOutcome::FailedAfterDownloaded;
    }

    let outbox_id = match create_pending_enqueue_outbox(pool, ingest_id, "ingest_validated").await {
        Ok(outbox_id) => outbox_id,
        Err(error) => {
            let failure = IngestFailureContext::database_status_update(
                FailurePhase::CreateOutbox,
                "create_enqueue_outbox_failed",
                &error,
            );
            let error_message = failure.database_message();

            if let Err(update_error) = mark_failed(pool, ingest_id, &error_message).await {
                log_failure_status_update_error(
                    granule,
                    &blob_path,
                    ingest_id,
                    &failure,
                    &update_error,
                    "failed to mark ingest row failed after enqueue outbox creation error",
                );
            }

            log_granule_failure(
                granule,
                &blob_path,
                ingest_id,
                Some(&config.azure_queue_name),
                &failure,
                &error,
                "failed to create ingest enqueue outbox record",
            );

            record_final_status("failed");

            return GranuleProcessingOutcome::FailedAfterValidated;
        }
    };

    let message = match processing_message_from_granule(ingest_id, granule, &blob_path) {
        Ok(message) => message,
        Err(error) => {
            let failure = IngestFailureContext::processing_message(&error);
            let error_message = failure.database_message();

            if let Err(update_error) = mark_failed(pool, ingest_id, &error_message).await {
                log_failure_status_update_error(
                    granule,
                    &blob_path,
                    ingest_id,
                    &failure,
                    &update_error,
                    "failed to mark ingest row failed after processing message construction error",
                );
            }

            log_granule_failure(
                granule,
                &blob_path,
                ingest_id,
                None,
                &failure,
                &error,
                "processing message construction failed",
            );

            record_final_status("failed");

            return GranuleProcessingOutcome::FailedAfterValidated;
        }
    };

    if let Err(error) = queue
        .enqueue_processing_message(&config.azure_queue_name, &message)
        .await
    {
        let failure = IngestFailureContext::queue_enqueue(&error);
        let error_message = failure.database_message();

        if let Err(update_error) = mark_failed(pool, ingest_id, &error_message).await {
            log_failure_status_update_error(
                granule,
                &blob_path,
                ingest_id,
                &failure,
                &update_error,
                "failed to mark ingest row failed after queue enqueue error",
            );
        }

        log_granule_failure(
            granule,
            &blob_path,
            ingest_id,
            Some(&config.azure_queue_name),
            &failure,
            &error,
            "processing message enqueue failed",
        );

        record_final_status("failed");

        return GranuleProcessingOutcome::FailedAfterValidated;
    }

    if let Err(error) = mark_enqueued(pool, ingest_id).await {
        let failure = IngestFailureContext::database_status_update(
            FailurePhase::MarkEnqueued,
            "mark_enqueued_failed",
            &error,
        );

        log_granule_failure(
            granule,
            &blob_path,
            ingest_id,
            Some(&config.azure_queue_name),
            &failure,
            &error,
            "failed to mark ingest row enqueued after queue enqueue",
        );

        record_final_status("failed");

        return GranuleProcessingOutcome::FailedAfterValidated;
    }

    if let Err(error) = mark_outbox_completed(pool, outbox_id).await {
        let failure = IngestFailureContext::database_status_update(
            FailurePhase::CompleteOutbox,
            "complete_enqueue_outbox_failed",
            &error,
        );

        log_granule_failure(
            granule,
            &blob_path,
            ingest_id,
            Some(&config.azure_queue_name),
            &failure,
            &error,
            "failed to complete ingest enqueue outbox after queue enqueue",
        );

        record_final_status("failed");

        return GranuleProcessingOutcome::FailedAfterValidated;
    }

    tracing::info!(
        product = granule.product,
        granule_title = granule.title,
        granule_date = %granule.granule_date,
        tile_h = granule.tile.h,
        tile_v = granule.tile.v,
        blob_path,
        ingest_id = %ingest_id,
        queue_name = config.azure_queue_name,
        "granule downloaded, uploaded, validated, and enqueued"
    );

    record_final_status("enqueued");

    GranuleProcessingOutcome::Enqueued
}

fn record_final_status(status: &'static str) {
    tracing::Span::current().record("final_status", status);
}

fn log_granule_failure(
    granule: &GranuleCandidate,
    blob_path: &str,
    ingest_id: Uuid,
    queue_name: Option<&str>,
    failure: &IngestFailureContext,
    error: &dyn std::fmt::Display,
    event: &'static str,
) {
    tracing::warn!(
        product = granule.product,
        granule_title = granule.title,
        granule_date = %granule.granule_date,
        tile_h = granule.tile.h,
        tile_v = granule.tile.v,
        blob_path,
        ingest_id = %ingest_id,
        queue_name = queue_name.unwrap_or(""),
        failure_phase = failure.phase.as_str(),
        error_code = failure.code,
        error_category = failure.category,
        retry_eligible = failure.retry_eligible,
        http_status = ?failure.http_status,
        error_message = %failure.message,
        error = %error,
        event,
        "ingest granule failure"
    );
}

fn log_failure_status_update_error(
    granule: &GranuleCandidate,
    blob_path: &str,
    ingest_id: Uuid,
    failure: &IngestFailureContext,
    update_error: &sqlx::Error,
    event: &'static str,
) {
    tracing::warn!(
        product = granule.product,
        granule_title = granule.title,
        granule_date = %granule.granule_date,
        tile_h = granule.tile.h,
        tile_v = granule.tile.v,
        blob_path,
        ingest_id = %ingest_id,
        failure_phase = failure.phase.as_str(),
        error_code = failure.code,
        error_category = failure.category,
        retry_eligible = failure.retry_eligible,
        http_status = ?failure.http_status,
        original_error_message = %failure.message,
        original_error_context = %failure.database_message(),
        error = %update_error,
        event,
        "failed to persist ingest failure status"
    );
}
