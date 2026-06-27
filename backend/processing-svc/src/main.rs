mod commands;
mod config;
mod db;
mod generate;
mod hdf_cli;
mod manifest;
mod models;
mod mosaic;
mod process;
mod publish;
mod render;
mod retention;
mod science;
mod storage;
mod tiles;
mod ui;

use std::{env, fmt, process::ExitCode, time::Instant};

use commands::{Command, CommandRequest, USAGE};
use config::AppConfig;
use uuid::Uuid;

const SERVICE_NAME: &str = "processing-svc";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), ServiceError> {
    let command_request = commands::parse_args(env::args().skip(1))?;

    let CommandRequest::Run(command) = command_request else {
        println!("{USAGE}");
        return Ok(());
    };

    let config = AppConfig::from_env_for(&command)?;

    init_tracing(&config)?;
    let correlation_id = Uuid::new_v4();
    let started_at = Instant::now();

    log_startup(&command, &config, correlation_id);
    ui::status(format_args!(
        "starting {} with queue '{}'",
        command.as_str(),
        config.azure_queue_name
    ));

    match process::dispatch(&command, &config, correlation_id).await {
        Ok(()) => {
            let duration = started_at.elapsed();
            ui::success(format_args!(
                "{} completed in {} ms",
                command.as_str(),
                duration.as_millis()
            ));
            tracing::info!(
                service = SERVICE_NAME,
                service_version = SERVICE_VERSION,
                command = command.as_str(),
                correlation_id = %correlation_id,
                duration_ms = duration.as_millis() as u64,
                "processing-svc command completed"
            );
        }
        Err(error) => {
            let duration = started_at.elapsed();
            ui::error(format_args!(
                "{} failed after {} ms: {error}",
                command.as_str(),
                duration.as_millis()
            ));
            tracing::error!(
                service = SERVICE_NAME,
                service_version = SERVICE_VERSION,
                command = command.as_str(),
                correlation_id = %correlation_id,
                duration_ms = duration.as_millis() as u64,
                error = %error,
                "processing-svc command failed"
            );

            return Err(error);
        }
    }

    Ok(())
}

fn init_tracing(config: &AppConfig) -> Result<(), ServiceError> {
    shared::observability::init_json_tracing(&config.rust_log).map_err(ServiceError::Observability)
}

fn log_startup(command: &Command, config: &AppConfig, correlation_id: Uuid) {
    tracing::info!(
        service = SERVICE_NAME,
        service_version = SERVICE_VERSION,
        command = command.as_str(),
        correlation_id = %correlation_id,
        database_url_configured = !config.database_url.is_empty(),
        azure_storage_account = config.azure_storage_account,
        azure_storage_access_key_configured = !config.azure_storage_access_key.is_empty(),
        azure_storage_emulator_host = config
            .azure_storage_emulator_host
            .as_deref()
            .unwrap_or("unset"),
        azure_queue_name = config.azure_queue_name,
        azure_deadletter_queue_name = config.azure_deadletter_queue_name,
        raw_viirs_container = config.raw_viirs_container,
        processed_tiles_container = config.processed_tiles_container,
        max_cloud_fraction = config.max_cloud_fraction,
        processing_visibility_timeout_seconds = config.processing_visibility_timeout_seconds,
        processing_max_dequeue_count = config.processing_max_dequeue_count,
        processing_max_parallelism = config.processing_max_parallelism,
        raw_granule_retention_days = config.raw_granule_retention_days,
        processed_tile_set_retention_days = config.processed_tile_set_retention_days,
        retention_protected_prior_tile_sets = config.retention_protected_prior_tile_sets,
        retention_batch_limit = config.retention_batch_limit,
        retention_tile_blob_limit = config.retention_tile_blob_limit,
        http_request_timeout_seconds = config.http_request_timeout.as_secs(),
        http_retry_max_attempts = config.http_retry.max_attempts,
        http_retry_base_delay_ms = config.http_retry.base_delay.as_millis() as u64,
        http_retry_max_delay_ms = config.http_retry.max_delay.as_millis() as u64,
        telemetry_sink = "stdout",
        "processing-svc starting"
    );
}

#[derive(Debug)]
enum ServiceError {
    Command(commands::CommandError),
    Config(config::ConfigError),
    Database(db::DbError),
    Generate(generate::GenerateError),
    HdfCli(hdf_cli::HdfCliError),
    Mosaic(mosaic::MosaicError),
    Observability(shared::observability::TracingInitError),
    ProcessingMessage(models::ProcessingMessageError),
    Publish(publish::PublishError),
    Retention(retention::RetentionError),
    Science(science::ScienceError),
    Storage(storage::StorageError),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => write!(formatter, "{error}"),
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Database(error) => write!(formatter, "{error}"),
            Self::Generate(error) => write!(formatter, "{error}"),
            Self::HdfCli(error) => write!(formatter, "{error}"),
            Self::Mosaic(error) => write!(formatter, "{error}"),
            Self::Observability(error) => write!(formatter, "{error}"),
            Self::ProcessingMessage(error) => write!(formatter, "{error}"),
            Self::Publish(error) => write!(formatter, "{error}"),
            Self::Retention(error) => write!(formatter, "{error}"),
            Self::Science(error) => write!(formatter, "{error}"),
            Self::Storage(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<commands::CommandError> for ServiceError {
    fn from(error: commands::CommandError) -> Self {
        Self::Command(error)
    }
}

impl From<config::ConfigError> for ServiceError {
    fn from(error: config::ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<db::DbError> for ServiceError {
    fn from(error: db::DbError) -> Self {
        Self::Database(error)
    }
}

impl From<generate::GenerateError> for ServiceError {
    fn from(error: generate::GenerateError) -> Self {
        Self::Generate(error)
    }
}

impl From<hdf_cli::HdfCliError> for ServiceError {
    fn from(error: hdf_cli::HdfCliError) -> Self {
        Self::HdfCli(error)
    }
}

impl From<models::ProcessingMessageError> for ServiceError {
    fn from(error: models::ProcessingMessageError) -> Self {
        Self::ProcessingMessage(error)
    }
}

impl From<mosaic::MosaicError> for ServiceError {
    fn from(error: mosaic::MosaicError) -> Self {
        Self::Mosaic(error)
    }
}

impl From<publish::PublishError> for ServiceError {
    fn from(error: publish::PublishError) -> Self {
        Self::Publish(error)
    }
}

impl From<retention::RetentionError> for ServiceError {
    fn from(error: retention::RetentionError) -> Self {
        Self::Retention(error)
    }
}

impl From<science::ScienceError> for ServiceError {
    fn from(error: science::ScienceError) -> Self {
        Self::Science(error)
    }
}

impl From<storage::StorageError> for ServiceError {
    fn from(error: storage::StorageError) -> Self {
        Self::Storage(error)
    }
}
