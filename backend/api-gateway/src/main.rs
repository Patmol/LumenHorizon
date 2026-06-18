mod auth;
mod commands;
mod config;
mod db;
mod error;
mod observability;
mod rate_limit;
mod readiness;
mod server;
mod state;
mod storage;
mod upstream;

use std::{env, fmt, process::ExitCode, time::Instant};

use commands::{CommandRequest, USAGE};
use config::AppConfig;
use state::AppState;
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

    let CommandRequest::Run = command_request else {
        println!("{USAGE}");
        return Ok(());
    };

    let config = AppConfig::from_env()?;
    observability::init(&config)?;

    let correlation_id = Uuid::new_v4();
    let started_at = Instant::now();

    tracing::info!(
        service = observability::SERVICE_NAME,
        service_version = observability::SERVICE_VERSION,
        correlation_id = %correlation_id,
        port = config.port,
        runtime_environment = config.runtime_environment.as_str(),
        jwt_issuer_configured = !config.auth.issuer.is_empty(),
        jwt_audience_configured = !config.auth.audience.is_empty(),
        jwks_url_configured = !config.auth.jwks_url.is_empty(),
        jwt_tenant_configured = config.auth.tenant_id.is_some(),
        admin_role_claim = config.auth.admin_role_claim,
        rate_limit_backend = config.rate_limit.backend.as_str(),
        database_configured = config.database.is_some(),
        tile_manifest_storage_configured = config.tile_manifest_storage.is_some(),
        ingest_admin_configured = config.ingest_admin.is_some(),
        internal_service_auth_configured = config.internal_service_auth.is_some(),
        processing_queue_configured = config.processing_queue.is_some(),
        public_timeout_seconds = config.public_timeout.as_secs(),
        admin_timeout_seconds = config.admin_timeout.as_secs(),
        health_timeout_seconds = config.health_timeout.as_secs(),
        cors_enabled = false,
        telemetry_sink = "stdout",
        "api-gateway starting"
    );

    let state = AppState::new(config);
    server::serve(state).await?;

    tracing::info!(
        service = observability::SERVICE_NAME,
        service_version = observability::SERVICE_VERSION,
        correlation_id = %correlation_id,
        duration_ms = started_at.elapsed().as_millis() as u64,
        "api-gateway stopped"
    );

    Ok(())
}

#[derive(Debug)]
enum ServiceError {
    Command(commands::CommandError),
    Config(config::ConfigError),
    Observability(observability::ObservabilityError),
    Server(server::ServerError),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => write!(formatter, "{error}"),
            Self::Config(error) => write!(formatter, "{error}"),
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
