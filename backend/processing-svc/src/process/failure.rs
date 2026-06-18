use crate::{config::AppConfig, db, models, ui, ServiceError};
use uuid::Uuid;

use super::queue_worker::ProcessingQueue;

pub(super) async fn mark_processing_failure(
    config: &AppConfig,
    processing_message: &models::ProcessingMessage,
    error: &ServiceError,
    deadlettered: bool,
    correlation_id: Uuid,
) -> Result<(), ServiceError> {
    let pool = db::connect(&config.database_url).await?;
    let error_message = error.to_string();

    if deadlettered {
        db::mark_processing_deadlettered(&pool, processing_message.ingest_id, &error_message)
            .await?;
    } else {
        db::mark_processing_failed(&pool, processing_message.ingest_id, &error_message).await?;
    }

    ui::warn(format_args!(
        "recorded {} status for ingest {}",
        if deadlettered {
            "deadlettered"
        } else {
            "failed"
        },
        processing_message.ingest_id
    ));

    tracing::warn!(
        command_correlation_id = %correlation_id,
        ingest_id = %processing_message.ingest_id,
        blob_path = processing_message.blob_path,
        product = processing_message.product,
        tile_h = processing_message.tile_h,
        tile_v = processing_message.tile_v,
        deadlettered,
        error = %error,
        "recorded processing failure status"
    );

    Ok(())
}

pub(super) async fn move_queue_message_to_deadletter(
    queue_client: &impl ProcessingQueue,
    queue_name: &str,
    deadletter_queue_name: &str,
    message_id: &str,
    pop_receipt: &str,
    message_text: &str,
    correlation_id: Uuid,
) -> Result<(), ServiceError> {
    ui::warn(format_args!(
        "enqueueing message {} to dead-letter queue '{}'",
        message_id, deadletter_queue_name
    ));
    queue_client
        .enqueue_message(deadletter_queue_name, message_text)
        .await?;

    tracing::warn!(
        command_correlation_id = %correlation_id,
        queue_name,
        deadletter_queue_name,
        message_id,
        "enqueued processing message to dead-letter queue"
    );

    queue_client
        .delete_message(queue_name, message_id, pop_receipt)
        .await?;

    ui::success(format_args!(
        "dead-lettered and deleted original message {}",
        message_id
    ));

    tracing::warn!(
        command_correlation_id = %correlation_id,
        queue_name,
        deadletter_queue_name,
        message_id,
        "deleted original processing queue message after dead-letter enqueue"
    );

    Ok(())
}
