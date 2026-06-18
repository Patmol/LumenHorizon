use crate::config::AppConfig;

pub const SERVICE_NAME: &str = "ingest-svc";
pub const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn init(config: &AppConfig) -> Result<(), ObservabilityError> {
    init_tracing(&config.rust_log)?;

    Ok(())
}

fn init_tracing(rust_log: &str) -> Result<(), ObservabilityError> {
    shared::observability::init_json_tracing(rust_log).map_err(|_| {
        ObservabilityError::TracingFilter {
            filter: rust_log.to_owned(),
        }
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error(
        "configuration error: invalid RUST_LOG value '{filter}': expected a valid tracing filter such as 'ingest_svc=info'"
    )]
    TracingFilter { filter: String },
}
