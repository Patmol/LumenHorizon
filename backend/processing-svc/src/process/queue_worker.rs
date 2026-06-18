//! Queue polling and retry handling for processing messages.
//!
//! The worker handles one visible queue message at a time, relying on the
//! queue visibility timeout for retries and moving exhausted messages to a
//! dead-letter queue.

use crate::{config::AppConfig, db, models, storage, ui, ServiceError};
use uuid::Uuid;

use super::{
    failure::{mark_processing_failure, move_queue_message_to_deadletter},
    message::process_parsed_message,
};

/// Queue operations used by the processing worker.
///
/// The trait keeps queue behavior injectable for tests while preserving the
/// queue visibility and acknowledgement semantics used by the worker.
pub(super) trait ProcessingQueue {
    /// Receives visible messages and hides them for the requested timeout.
    async fn receive_messages(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout_seconds: u64,
    ) -> Result<Vec<storage::ReceivedQueueMessage>, ServiceError>;

    /// Deletes a message after successful processing.
    async fn delete_message(
        &self,
        queue_name: &str,
        message_id: &str,
        pop_receipt: &str,
    ) -> Result<(), ServiceError>;

    /// Adds a message to a queue, used when dead-lettering failed work.
    async fn enqueue_message(
        &self,
        queue_name: &str,
        message_text: &str,
    ) -> Result<(), ServiceError>;
}

impl ProcessingQueue for storage::QueueClient {
    async fn receive_messages(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout_seconds: u64,
    ) -> Result<Vec<storage::ReceivedQueueMessage>, ServiceError> {
        storage::QueueClient::receive_messages(
            self,
            queue_name,
            max_messages,
            visibility_timeout_seconds,
        )
        .await
        .map_err(ServiceError::from)
    }

    async fn delete_message(
        &self,
        queue_name: &str,
        message_id: &str,
        pop_receipt: &str,
    ) -> Result<(), ServiceError> {
        storage::QueueClient::delete_message(self, queue_name, message_id, pop_receipt)
            .await
            .map_err(ServiceError::from)
    }

    async fn enqueue_message(
        &self,
        queue_name: &str,
        message_text: &str,
    ) -> Result<(), ServiceError> {
        storage::QueueClient::enqueue_message(self, queue_name, message_text)
            .await
            .map_err(ServiceError::from)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcessOnceOutcome {
    /// A message was received and either processed or terminally handled.
    HandledMessage,
    /// No visible message was available in the queue.
    NoMessage,
}

/// Returns true once Azure Queue dequeue attempts have reached the configured limit.
pub(super) fn should_deadletter(dequeue_count: u32, max_dequeue_count: u32) -> bool {
    dequeue_count >= max_dequeue_count
}

pub(super) fn is_missing_ingest_log_error(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::Database(db::DbError::MissingIngestLog { .. })
    )
}

/// Processes at most one message using the configured storage queue client.
///
/// Message-level processing stays serial in one worker iteration; tile rendering
/// inside a message is bounded by `PROCESSING_MAX_PARALLELISM`.
pub(super) async fn process_once(
    config: &AppConfig,
    correlation_id: Uuid,
) -> Result<ProcessOnceOutcome, ServiceError> {
    let queue_client = storage::QueueClient::new(config)?;

    process_once_with_queue(config, &queue_client, correlation_id).await
}

/// Receives, processes, and acknowledges at most one processing queue message.
///
/// Non-terminal failures return an error so the message becomes visible again
/// after the queue visibility timeout. Terminal failures are moved to the
/// dead-letter queue and reported as handled.
pub(super) async fn process_once_with_queue(
    config: &AppConfig,
    queue_client: &impl ProcessingQueue,
    correlation_id: Uuid,
) -> Result<ProcessOnceOutcome, ServiceError> {
    ui::status(format_args!(
        "polling queue '{}' with {}s visibility timeout",
        config.azure_queue_name, config.processing_visibility_timeout_seconds
    ));

    let mut messages = queue_client
        .receive_messages(
            &config.azure_queue_name,
            1,
            config.processing_visibility_timeout_seconds,
        )
        .await?;

    let Some(message) = messages.pop() else {
        ui::success(format_args!(
            "no visible messages in queue '{}'",
            config.azure_queue_name
        ));
        tracing::info!(
            command_correlation_id = %correlation_id,
            queue_name = config.azure_queue_name,
            "no processing queue messages available"
        );
        return Ok(ProcessOnceOutcome::NoMessage);
        // A received message is invisible until deletion or visibility-timeout expiry.
    };

    ui::status(format_args!(
        "received message {} (dequeue count {})",
        message.message_id, message.dequeue_count
    ));
    tracing::info!(
        command_correlation_id = %correlation_id,
        queue_name = config.azure_queue_name,
        message_id = %message.message_id,
        dequeue_count = message.dequeue_count,
        "received processing queue message"
    );

    let processing_message = match models::ProcessingMessage::parse_json(&message.message_text) {
        Ok(processing_message) => processing_message,
        Err(error) => {
            let error = ServiceError::from(error);

            if should_deadletter(message.dequeue_count, config.processing_max_dequeue_count) {
                ui::warn(format_args!(
                    "message {} is malformed and exceeded retry limit; moving to '{}'",
                    message.message_id, config.azure_deadletter_queue_name
                ));
                // Malformed messages cannot be associated with an ingest record for DB updates.
                tracing::error!(
                    command_correlation_id = %correlation_id,
                    queue_name = config.azure_queue_name,
                    deadletter_queue_name = config.azure_deadletter_queue_name,
                    message_id = %message.message_id,
                    dequeue_count = message.dequeue_count,
                    max_dequeue_count = config.processing_max_dequeue_count,
                    error = %error,
                    "malformed processing queue message exceeded max dequeue count; moving to dead-letter queue without DB update"
                );

                move_queue_message_to_deadletter(
                    queue_client,
                    &config.azure_queue_name,
                    &config.azure_deadletter_queue_name,
                    &message.message_id,
                    &message.pop_receipt,
                    &message.message_text,
                    correlation_id,
                )
                .await?;

                return Ok(ProcessOnceOutcome::HandledMessage);
            }

            ui::warn(format_args!(
                "message {} could not be parsed; it will retry after visibility timeout",
                message.message_id
            ));
            tracing::warn!(
                command_correlation_id = %correlation_id,
                queue_name = config.azure_queue_name,
                message_id = %message.message_id,
                dequeue_count = message.dequeue_count,
                max_dequeue_count = config.processing_max_dequeue_count,
                error = %error,
                "processing queue message could not be parsed; leaving message for retry after visibility timeout"
            );

            return Err(error);
        }
    };

    if let Err(error) = process_parsed_message(config, &processing_message, correlation_id).await {
        if is_missing_ingest_log_error(&error) {
            ui::warn(format_args!(
                "message {} references missing ingest row {}; moving to '{}'",
                message.message_id,
                processing_message.ingest_id,
                config.azure_deadletter_queue_name
            ));
            tracing::error!(
                command_correlation_id = %correlation_id,
                queue_name = config.azure_queue_name,
                deadletter_queue_name = config.azure_deadletter_queue_name,
                message_id = %message.message_id,
                ingest_id = %processing_message.ingest_id,
                blob_path = processing_message.blob_path,
                product = processing_message.product,
                tile_h = processing_message.tile_h,
                tile_v = processing_message.tile_v,
                dequeue_count = message.dequeue_count,
                error = %error,
                "processing queue message references missing ingest_log row; moving to dead-letter queue without DB update"
            );

            move_queue_message_to_deadletter(
                queue_client,
                &config.azure_queue_name,
                &config.azure_deadletter_queue_name,
                &message.message_id,
                &message.pop_receipt,
                &message.message_text,
                correlation_id,
            )
            .await?;

            return Ok(ProcessOnceOutcome::HandledMessage);
        }

        if should_deadletter(message.dequeue_count, config.processing_max_dequeue_count) {
            ui::warn(format_args!(
                "message {} exceeded retry limit; moving to '{}'",
                message.message_id, config.azure_deadletter_queue_name
            ));
            // Parsed messages can record failure state before being dead-lettered.
            tracing::error!(
                command_correlation_id = %correlation_id,
                queue_name = config.azure_queue_name,
                deadletter_queue_name = config.azure_deadletter_queue_name,
                message_id = %message.message_id,
                ingest_id = %processing_message.ingest_id,
                blob_path = processing_message.blob_path,
                product = processing_message.product,
                tile_h = processing_message.tile_h,
                tile_v = processing_message.tile_v,
                dequeue_count = message.dequeue_count,
                max_dequeue_count = config.processing_max_dequeue_count,
                error = %error,
                "processing queue message exceeded max dequeue count; moving to dead-letter queue"
            );

            mark_processing_failure(config, &processing_message, &error, true, correlation_id)
                .await?;

            move_queue_message_to_deadletter(
                queue_client,
                &config.azure_queue_name,
                &config.azure_deadletter_queue_name,
                &message.message_id,
                &message.pop_receipt,
                &message.message_text,
                correlation_id,
            )
            .await?;

            return Ok(ProcessOnceOutcome::HandledMessage);
        }

        ui::warn(format_args!(
            "message {} failed: {error}; it will retry after visibility timeout",
            message.message_id
        ));
        tracing::warn!(
            command_correlation_id = %correlation_id,
            queue_name = config.azure_queue_name,
            message_id = %message.message_id,
            ingest_id = %processing_message.ingest_id,
            blob_path = processing_message.blob_path,
            product = processing_message.product,
            tile_h = processing_message.tile_h,
            tile_v = processing_message.tile_v,
            dequeue_count = message.dequeue_count,
            max_dequeue_count = config.processing_max_dequeue_count,
            error = %error,
            "processing queue message failed; leaving message for retry after visibility timeout"
        );

        mark_processing_failure(config, &processing_message, &error, false, correlation_id).await?;

        return Err(error);
    }

    ui::status(format_args!(
        "deleting processed queue message {}",
        message.message_id
    ));
    queue_client
        .delete_message(
            &config.azure_queue_name,
            &message.message_id,
            &message.pop_receipt,
        )
        .await?;

    ui::success(format_args!(
        "deleted queue message {} for ingest {}",
        message.message_id, processing_message.ingest_id
    ));
    tracing::info!(
        command_correlation_id = %correlation_id,
        queue_name = config.azure_queue_name,
        message_id = %message.message_id,
        ingest_id = %processing_message.ingest_id,
        blob_path = processing_message.blob_path,
        product = processing_message.product,
        tile_h = processing_message.tile_h,
        tile_v = processing_message.tile_v,
        "deleted processed queue message"
    );

    Ok(ProcessOnceOutcome::HandledMessage)
}
