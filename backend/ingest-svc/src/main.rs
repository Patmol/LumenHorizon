mod clients;
mod cmr;
mod commands;
mod config;
mod db;
mod earthdata;
mod error;
mod jobs;
mod models;
mod observability;
mod readiness;
mod server;
mod state;
mod storage;

use std::{env, fmt, process::ExitCode, time::Instant};

use commands::{Command, CommandRequest, USAGE};
use config::AppConfig;
use uuid::Uuid;

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

    let config = AppConfig::from_env_for(command)?;

    observability::init(&config)?;
    let correlation_id = Uuid::new_v4();
    let started_at = Instant::now();

    log_startup(command, &config, correlation_id);

    match dispatch(command, &config, correlation_id).await {
        Ok(()) => {
            let duration = started_at.elapsed();
            tracing::info!(
                service = observability::SERVICE_NAME,
                service_version = observability::SERVICE_VERSION,
                command = command.as_str(),
                correlation_id = %correlation_id,
                duration_ms = duration.as_millis() as u64,
                "ingest-svc command completed"
            );
        }
        Err(error) => {
            let duration = started_at.elapsed();
            tracing::error!(
                service = observability::SERVICE_NAME,
                service_version = observability::SERVICE_VERSION,
                command = command.as_str(),
                correlation_id = %correlation_id,
                duration_ms = duration.as_millis() as u64,
                error = %error,
                "ingest-svc command failed"
            );

            return Err(error);
        }
    }

    Ok(())
}

fn log_startup(command: Command, config: &AppConfig, correlation_id: Uuid) {
    tracing::info!(
        service = observability::SERVICE_NAME,
        service_version = observability::SERVICE_VERSION,
        command = command.as_str(),
        correlation_id = %correlation_id,
        port = config.port,
        database_url_configured = !config.database_url.is_empty(),
        azure_storage_account = config.azure_storage_account,
        azure_storage_access_key_configured = !config.azure_storage_access_key.is_empty(),
        azure_storage_emulator_host = config
            .azure_storage_emulator_host
            .as_deref()
            .unwrap_or("unset"),
        azure_queue_name = config.azure_queue_name,
        earthdata_token_configured = config.earthdata_bearer_token.is_some(),
        bounding_box = %config.bounding_box,
        max_cloud_fraction = config.max_cloud_fraction,
        http_request_timeout_seconds = config.http_request_timeout.as_secs(),
        http_retry_max_attempts = config.http_retry.max_attempts,
        http_retry_base_delay_ms = config.http_retry.base_delay.as_millis() as u64,
        http_retry_max_delay_ms = config.http_retry.max_delay.as_millis() as u64,
        ingest_cadence = config.ingest_cadence.as_str(),
        ingest_products = config.ingest_products.join(","),
        telemetry_sink = "stdout",
        "ingest-svc starting"
    );
}

async fn dispatch(
    command: Command,
    config: &AppConfig,
    correlation_id: Uuid,
) -> Result<(), ServiceError> {
    match command {
        Command::Serve => {
            let pool = db::connect(&config.database_url).await?;
            let state = state::AppState::new(config.clone(), pool);

            server::serve(state).await?;
        }
        Command::Ingest { .. } => {
            let pool = db::connect(&config.database_url).await?;

            jobs::run_ingest(config, &pool, correlation_id).await?;
        }
        Command::RecoverIngest => {
            let pool = db::connect(&config.database_url).await?;

            jobs::run_recovery(config, &pool).await?;
        }
        Command::ReplayRejected { ingest_id } => {
            let pool = db::connect(&config.database_url).await?;

            jobs::replay_rejected_granule(config, &pool, ingest_id).await?;
        }
    }

    Ok(())
}

#[derive(Debug)]
enum ServiceError {
    Command(commands::CommandError),
    Config(config::ConfigError),
    Database(db::DbError),
    Ingest(jobs::IngestError),
    Observability(observability::ObservabilityError),
    Server(server::ServerError),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => write!(formatter, "{error}"),
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Database(error) => write!(formatter, "{error}"),
            Self::Ingest(error) => write!(formatter, "{error}"),
            Self::Observability(error) => write!(formatter, "{error}"),
            Self::Server(error) => write!(formatter, "{error}"),
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

impl From<jobs::IngestError> for ServiceError {
    fn from(error: jobs::IngestError) -> Self {
        Self::Ingest(error)
    }
}

impl From<observability::ObservabilityError> for ServiceError {
    fn from(error: observability::ObservabilityError) -> Self {
        Self::Observability(error)
    }
}

impl From<server::ServerError> for ServiceError {
    fn from(error: server::ServerError) -> Self {
        Self::Server(error)
    }
}
