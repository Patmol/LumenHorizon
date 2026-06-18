use uuid::Uuid;

use crate::{commands::Command, config::AppConfig, retention, ui, ServiceError};

use super::{
    message::process_message_payload,
    queue_worker::{process_once, ProcessOnceOutcome},
};

pub(crate) async fn dispatch(
    command: &Command,
    config: &AppConfig,
    correlation_id: Uuid,
) -> Result<(), ServiceError> {
    match command {
        Command::Worker => loop {
            match process_once(config, correlation_id).await? {
                ProcessOnceOutcome::HandledMessage => {
                    ui::success(format_args!("worker handled one queue message"));
                    ui::status(format_args!("worker polling for the next visible message"));
                    tracing::info!(
                        command_correlation_id = %correlation_id,
                        "processing worker handled one queue message"
                    );
                }
                ProcessOnceOutcome::NoMessage => {
                    ui::success(format_args!("worker found no visible messages; exiting"));
                    tracing::info!(
                        command_correlation_id = %correlation_id,
                        "processing worker found no more queue messages"
                    );
                    break;
                }
            }
        },
        Command::ProcessOnce => {
            let _outcome = process_once(config, correlation_id).await?;
        }
        Command::ProcessMessage { message } => {
            process_message_payload(config, message, correlation_id).await?;
        }
        Command::RetentionCleanup { execute } => {
            retention::run_retention_cleanup(config, *execute, correlation_id).await?;
        }
    }

    Ok(())
}
