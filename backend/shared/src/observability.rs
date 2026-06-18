use tracing_subscriber::EnvFilter;

pub fn init_json_tracing(rust_log: &str) -> Result<(), TracingInitError> {
    let filter = EnvFilter::try_new(rust_log).map_err(|_| TracingInitError::InvalidFilter {
        filter: rust_log.to_owned(),
    })?;

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .init();

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum TracingInitError {
    #[error("invalid tracing filter '{filter}'")]
    InvalidFilter { filter: String },
}
