use std::{fmt, time::Duration};

use base64::Engine as _;
pub use shared::http_retry::RetryConfig;
use tracing_subscriber::EnvFilter;

use crate::{commands::Command, models::ProductCadence};

const DEFAULT_PORT: u16 = 8083;
const DEFAULT_RUST_LOG: &str = "ingest_svc=info";
const DEFAULT_QUEUE_NAME: &str = "viirs-processing";
const DEFAULT_BOUNDING_BOX: &str = "-125,24,-66,50";
const DEFAULT_MAX_CLOUD_FRACTION: f32 = 0.4;
const DEFAULT_HTTP_REQUEST_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_HTTP_RETRY_MAX_ATTEMPTS: u32 = 3;
const DEFAULT_HTTP_RETRY_BASE_DELAY_MS: u64 = 250;
const DEFAULT_HTTP_RETRY_MAX_DELAY_MS: u64 = 5_000;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub rust_log: String,
    pub database_url: String,
    pub azure_storage_account: String,
    pub azure_storage_access_key: String,
    pub azure_storage_emulator_host: Option<String>,
    pub azure_queue_name: String,
    pub earthdata_bearer_token: Option<String>,
    pub ingest_cadence: ProductCadence,
    pub ingest_products: Vec<String>,
    pub bounding_box: BoundingBox,
    pub max_cloud_fraction: f32,
    pub ingest_max_granules: Option<usize>,
    pub internal_admin_auth: Option<InternalAdminAuthConfig>,
    pub http_request_timeout: Duration,
    pub http_retry: RetryConfig,
}

impl AppConfig {
    pub fn from_env_for(command: Command) -> Result<Self, ConfigError> {
        Self::from_lookup_for(command, |name| std::env::var(name).ok())
    }

    #[cfg(test)]
    pub(crate) fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        Self::from_lookup_for(Command::Serve, lookup)
    }

    pub(crate) fn from_lookup_for<F>(command: Command, lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut missing = Vec::new();

        let database_url = read_required(
            &lookup,
            "DATABASE_URL",
            "PostgreSQL connection used by serve and ingest",
            &mut missing,
        );

        let azure_storage_account = read_required(
            &lookup,
            "AZURE_STORAGE_ACCOUNT",
            "storage account used for raw VIIRS blobs and processing queue access",
            &mut missing,
        );

        let azure_storage_access_key = read_required(
            &lookup,
            "AZURE_STORAGE_ACCESS_KEY",
            "storage access key used for blob and queue access",
            &mut missing,
        );

        if !missing.is_empty() {
            return Err(ConfigError::MissingRequired { variables: missing });
        }

        let port = parse_port(optional_or_default(
            &lookup,
            "PORT",
            DEFAULT_PORT.to_string(),
        ))?;
        let rust_log = optional_or_default(&lookup, "RUST_LOG", DEFAULT_RUST_LOG.to_owned());
        validate_rust_log(&rust_log)?;

        if let Some(azure_storage_access_key) = azure_storage_access_key.as_deref() {
            validate_storage_access_key(azure_storage_access_key)?;
        }

        let ingest_cadence = ingest_cadence_for_command(command);
        let ingest_products = ingest_products_for_cadence(ingest_cadence);

        Ok(Self {
            port,
            rust_log,
            database_url: database_url.expect("missing required config returned above"),
            azure_storage_account: azure_storage_account.unwrap_or_default(),
            azure_storage_access_key: azure_storage_access_key.unwrap_or_default(),
            azure_storage_emulator_host: read_optional(&lookup, "AZURE_STORAGE_EMULATOR_HOST"),
            azure_queue_name: optional_or_default(
                &lookup,
                "AZURE_QUEUE_NAME",
                DEFAULT_QUEUE_NAME.to_owned(),
            ),
            earthdata_bearer_token: read_optional(&lookup, "EARTHDATA_BEARER_TOKEN"),
            ingest_cadence,
            ingest_products,
            bounding_box: parse_bounding_box(optional_or_default(
                &lookup,
                "BOUNDING_BOX",
                DEFAULT_BOUNDING_BOX.to_owned(),
            ))?,
            max_cloud_fraction: parse_max_cloud_fraction(optional_or_default(
                &lookup,
                "MAX_CLOUD_FRACTION",
                DEFAULT_MAX_CLOUD_FRACTION.to_string(),
            ))?,
            ingest_max_granules: parse_ingest_max_granules(read_optional(
                &lookup,
                "INGEST_MAX_GRANULES",
            ))?,
            internal_admin_auth: parse_internal_admin_auth_config(&lookup)?,
            http_request_timeout: parse_positive_duration_seconds(
                "HTTP_REQUEST_TIMEOUT_SECONDS",
                optional_or_default(
                    &lookup,
                    "HTTP_REQUEST_TIMEOUT_SECONDS",
                    DEFAULT_HTTP_REQUEST_TIMEOUT_SECONDS.to_string(),
                ),
            )?,
            http_retry: parse_retry_config(&lookup)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    west: f64,
    south: f64,
    east: f64,
    north: f64,
}

impl fmt::Display for BoundingBox {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{},{},{},{}",
            self.west, self.south, self.east, self.north
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalAdminAuthConfig {
    pub header_name: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    InvalidSecretValue {
        variable: &'static str,
        expected: &'static str,
    },
    InvalidValue {
        variable: &'static str,
        value: String,
        expected: &'static str,
    },
    MissingRequired {
        variables: Vec<MissingVariable>,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSecretValue { variable, expected } => write!(
                formatter,
                "configuration error: invalid {variable} value: {expected}"
            ),
            Self::InvalidValue {
                variable,
                value,
                expected,
            } => write!(
                formatter,
                "configuration error: invalid {variable} value '{value}': {expected}"
            ),
            Self::MissingRequired { variables } => {
                writeln!(
                    formatter,
                    "configuration error: missing required environment variables:"
                )?;
                for variable in variables {
                    writeln!(formatter, "- {}: {}", variable.name, variable.purpose)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingVariable {
    name: &'static str,
    purpose: &'static str,
}

fn read_required<F>(
    lookup: &F,
    name: &'static str,
    purpose: &'static str,
    missing: &mut Vec<MissingVariable>,
) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    match read_optional(lookup, name) {
        Some(value) => Some(value),
        None => {
            missing.push(MissingVariable { name, purpose });
            None
        }
    }
}

fn read_optional<F>(lookup: &F, name: &str) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(name).filter(|value| !value.trim().is_empty())
}

fn optional_or_default<F>(lookup: &F, name: &str, default: String) -> String
where
    F: Fn(&str) -> Option<String>,
{
    read_optional(lookup, name).unwrap_or(default)
}

fn parse_port(value: String) -> Result<u16, ConfigError> {
    let port = value
        .parse::<u16>()
        .map_err(|_| ConfigError::InvalidValue {
            variable: "PORT",
            value: value.clone(),
            expected: "expected a TCP port between 1 and 65535",
        })?;

    if port == 0 {
        return Err(ConfigError::InvalidValue {
            variable: "PORT",
            value,
            expected: "expected a TCP port between 1 and 65535",
        });
    }

    Ok(port)
}

fn validate_rust_log(value: &str) -> Result<(), ConfigError> {
    EnvFilter::try_new(value)
        .map(|_| ())
        .map_err(|_| ConfigError::InvalidValue {
            variable: "RUST_LOG",
            value: value.to_owned(),
            expected: "expected a valid tracing filter such as 'ingest_svc=info'",
        })
}

fn validate_storage_access_key(value: &str) -> Result<(), ConfigError> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map(|_| ())
        .map_err(|_| ConfigError::InvalidSecretValue {
            variable: "AZURE_STORAGE_ACCESS_KEY",
            expected: "expected a base64-encoded Azure Storage account key; for local Azurite use the devstoreaccount1 key from .env.example",
        })
}

fn parse_bounding_box(value: String) -> Result<BoundingBox, ConfigError> {
    let parts = value
        .split(',')
        .map(str::trim)
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid_bounding_box(value.clone()))?;

    let [west, south, east, north] = parts.as_slice() else {
        return Err(invalid_bounding_box(value));
    };

    if !(-180.0..=180.0).contains(west)
        || !(-180.0..=180.0).contains(east)
        || !(-90.0..=90.0).contains(south)
        || !(-90.0..=90.0).contains(north)
        || west >= east
        || south >= north
    {
        return Err(invalid_bounding_box(value));
    }

    Ok(BoundingBox {
        west: *west,
        south: *south,
        east: *east,
        north: *north,
    })
}

fn invalid_bounding_box(value: String) -> ConfigError {
    ConfigError::InvalidValue {
        variable: "BOUNDING_BOX",
        value,
        expected: "expected four comma-separated numbers in west,south,east,north order with valid longitude and latitude ranges",
    }
}

fn parse_max_cloud_fraction(value: String) -> Result<f32, ConfigError> {
    let fraction = value
        .parse::<f32>()
        .map_err(|_| ConfigError::InvalidValue {
            variable: "MAX_CLOUD_FRACTION",
            value: value.clone(),
            expected: "expected a number from 0.0 through 1.0",
        })?;

    if !(0.0..=1.0).contains(&fraction) {
        return Err(ConfigError::InvalidValue {
            variable: "MAX_CLOUD_FRACTION",
            value,
            expected: "expected a number from 0.0 through 1.0",
        });
    }

    Ok(fraction)
}

fn parse_ingest_max_granules(value: Option<String>) -> Result<Option<usize>, ConfigError> {
    let Some(value) = value else {
        return Ok(None);
    };

    let count = value
        .parse::<usize>()
        .map_err(|_| ConfigError::InvalidValue {
            variable: "INGEST_MAX_GRANULES",
            value: value.clone(),
            expected: "expected a positive integer",
        })?;

    if count == 0 {
        return Err(ConfigError::InvalidValue {
            variable: "INGEST_MAX_GRANULES",
            value: value.clone(),
            expected: "expected a positive integer",
        });
    }

    Ok(Some(count))
}

fn parse_internal_admin_auth_config<F>(
    lookup: &F,
) -> Result<Option<InternalAdminAuthConfig>, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(token) = read_optional(lookup, "INTERNAL_SERVICE_AUTH_TOKEN") else {
        return Ok(None);
    };
    validate_internal_service_auth_token(&token)?;

    let header_name = optional_or_default(
        lookup,
        "INTERNAL_SERVICE_AUTH_HEADER",
        "x-lumenhorizon-internal-token".to_owned(),
    );
    axum::http::HeaderName::from_bytes(header_name.as_bytes()).map_err(|_| {
        ConfigError::InvalidValue {
            variable: "INTERNAL_SERVICE_AUTH_HEADER",
            value: header_name.clone(),
            expected: "expected a valid HTTP header name",
        }
    })?;

    Ok(Some(InternalAdminAuthConfig { header_name, token }))
}

fn validate_internal_service_auth_token(value: &str) -> Result<(), ConfigError> {
    if value.len() < 32 || value.chars().any(char::is_whitespace) {
        return Err(ConfigError::InvalidSecretValue {
            variable: "INTERNAL_SERVICE_AUTH_TOKEN",
            expected: "expected at least 32 non-whitespace characters",
        });
    }

    Ok(())
}

fn ingest_cadence_for_command(command: Command) -> ProductCadence {
    command.ingest_cadence().unwrap_or(ProductCadence::Daily)
}

fn ingest_products_for_cadence(cadence: ProductCadence) -> Vec<String> {
    cadence
        .default_products()
        .iter()
        .map(|product| (*product).to_owned())
        .collect()
}

fn parse_positive_duration_seconds(
    variable: &'static str,
    value: String,
) -> Result<Duration, ConfigError> {
    let seconds = value
        .parse::<u64>()
        .map_err(|_| ConfigError::InvalidValue {
            variable,
            value: value.clone(),
            expected: "expected a positive integer number of seconds",
        })?;

    if seconds == 0 {
        return Err(ConfigError::InvalidValue {
            variable,
            value,
            expected: "expected a positive integer number of seconds",
        });
    }

    Ok(Duration::from_secs(seconds))
}

fn parse_retry_config<F>(lookup: &F) -> Result<RetryConfig, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    let max_attempts = parse_positive_u32(
        "HTTP_RETRY_MAX_ATTEMPTS",
        optional_or_default(
            lookup,
            "HTTP_RETRY_MAX_ATTEMPTS",
            DEFAULT_HTTP_RETRY_MAX_ATTEMPTS.to_string(),
        ),
        "expected a positive integer retry attempt count",
    )?;
    let base_delay = parse_positive_duration_millis(
        "HTTP_RETRY_BASE_DELAY_MS",
        optional_or_default(
            lookup,
            "HTTP_RETRY_BASE_DELAY_MS",
            DEFAULT_HTTP_RETRY_BASE_DELAY_MS.to_string(),
        ),
    )?;
    let max_delay = parse_positive_duration_millis(
        "HTTP_RETRY_MAX_DELAY_MS",
        optional_or_default(
            lookup,
            "HTTP_RETRY_MAX_DELAY_MS",
            DEFAULT_HTTP_RETRY_MAX_DELAY_MS.to_string(),
        ),
    )?;

    if base_delay > max_delay {
        return Err(ConfigError::InvalidValue {
            variable: "HTTP_RETRY_BASE_DELAY_MS",
            value: base_delay.as_millis().to_string(),
            expected: "expected a base delay less than or equal to HTTP_RETRY_MAX_DELAY_MS",
        });
    }

    Ok(RetryConfig {
        max_attempts,
        base_delay,
        max_delay,
    })
}

fn parse_positive_duration_millis(
    variable: &'static str,
    value: String,
) -> Result<Duration, ConfigError> {
    let millis = value
        .parse::<u64>()
        .map_err(|_| ConfigError::InvalidValue {
            variable,
            value: value.clone(),
            expected: "expected a positive integer number of milliseconds",
        })?;

    if millis == 0 {
        return Err(ConfigError::InvalidValue {
            variable,
            value,
            expected: "expected a positive integer number of milliseconds",
        });
    }

    Ok(Duration::from_millis(millis))
}

fn parse_positive_u32(
    variable: &'static str,
    value: String,
    expected: &'static str,
) -> Result<u32, ConfigError> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| ConfigError::InvalidValue {
            variable,
            value: value.clone(),
            expected,
        })?;

    if parsed == 0 {
        return Err(ConfigError::InvalidValue {
            variable,
            value,
            expected,
        });
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{commands::Command, models::ProductCadence};

    use super::AppConfig;

    const TEST_STORAGE_ACCESS_KEY: &str = "dGVzdC1zdG9yYWdlLWFjY291bnQta2V5";

    fn valid_env() -> HashMap<&'static str, String> {
        HashMap::from([
            (
                "DATABASE_URL",
                "postgres://localhost/lumenhorizon".to_owned(),
            ),
            ("AZURE_STORAGE_ACCOUNT", "devstoreaccount1".to_owned()),
            (
                "AZURE_STORAGE_ACCESS_KEY",
                TEST_STORAGE_ACCESS_KEY.to_owned(),
            ),
        ])
    }

    fn load_from(env: HashMap<&'static str, String>) -> Result<AppConfig, super::ConfigError> {
        AppConfig::from_lookup(|name| env.get(name).cloned())
    }

    fn load_ingest_from(
        env: HashMap<&'static str, String>,
        cadence: ProductCadence,
    ) -> Result<AppConfig, super::ConfigError> {
        AppConfig::from_lookup_for(Command::Ingest { cadence }, |name| env.get(name).cloned())
    }

    #[test]
    fn loads_defaults_with_required_values() {
        let config = load_from(valid_env()).unwrap();

        assert_eq!(config.port, 8083);
        assert_eq!(config.rust_log, "ingest_svc=info");
        assert_eq!(config.azure_queue_name, "viirs-processing");
        assert_eq!(config.ingest_cadence, ProductCadence::Daily);
        assert_eq!(config.ingest_products, ["VNP46A2", "VJ146A2"]);
        assert_eq!(config.bounding_box.to_string(), "-125,24,-66,50");
        assert_eq!(config.max_cloud_fraction, 0.4);
        assert_eq!(config.ingest_max_granules, None);
        assert!(config.internal_admin_auth.is_none());
        assert_eq!(config.http_request_timeout.as_secs(), 30);
        assert_eq!(config.http_retry.max_attempts, 3);
        assert_eq!(config.http_retry.base_delay.as_millis(), 250);
        assert_eq!(config.http_retry.max_delay.as_millis(), 5_000);
    }

    #[test]
    fn reports_all_missing_required_values() {
        let error = load_from(HashMap::new()).unwrap_err().to_string();

        assert!(error.contains("DATABASE_URL"));
        assert!(error.contains("AZURE_STORAGE_ACCOUNT"));
        assert!(error.contains("AZURE_STORAGE_ACCESS_KEY"));
    }

    #[test]
    fn rejects_invalid_port() {
        let mut env = valid_env();
        env.insert("PORT", "not-a-port".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid PORT"));
    }

    #[test]
    fn rejects_invalid_storage_access_key_without_exposing_value() {
        let mut env = valid_env();
        env.insert(
            "AZURE_STORAGE_ACCESS_KEY",
            "local-storage-account-key".to_owned(),
        );

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid AZURE_STORAGE_ACCESS_KEY"));
        assert!(error.contains("base64-encoded"));
        assert!(!error.contains("local-storage-account-key"));
    }

    #[test]
    fn loads_internal_admin_auth_without_exposing_invalid_secret() {
        let fixture_token = "local-fixture-token-for-tests-only-0000";
        let mut env = valid_env();
        env.insert("INTERNAL_SERVICE_AUTH_TOKEN", fixture_token.to_owned());

        let config = load_from(env).unwrap();
        let auth = config.internal_admin_auth.unwrap();
        assert_eq!(auth.header_name, "x-lumenhorizon-internal-token");
        assert_eq!(auth.token, fixture_token);

        let mut env = valid_env();
        env.insert("INTERNAL_SERVICE_AUTH_TOKEN", "short-secret".to_owned());

        let error = load_from(env).unwrap_err().to_string();
        assert!(error.contains("INTERNAL_SERVICE_AUTH_TOKEN"));
        assert!(!error.contains("short-secret"));
    }

    #[test]
    fn rejects_invalid_bounding_box() {
        let mut env = valid_env();
        env.insert("BOUNDING_BOX", "-125,24".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid BOUNDING_BOX"));
    }

    #[test]
    fn rejects_invalid_cloud_fraction() {
        let mut env = valid_env();
        env.insert("MAX_CLOUD_FRACTION", "1.5".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid MAX_CLOUD_FRACTION"));
    }

    #[test]
    fn loads_ingest_max_granules() {
        let mut env = valid_env();
        env.insert("INGEST_MAX_GRANULES", "1".to_owned());

        let config = load_from(env).unwrap();

        assert_eq!(config.ingest_max_granules, Some(1));
    }

    #[test]
    fn loads_monthly_cadence_from_command() {
        let config = load_ingest_from(valid_env(), ProductCadence::Monthly).unwrap();

        assert_eq!(config.ingest_cadence, ProductCadence::Monthly);
        assert_eq!(config.ingest_products, ["VNP46A3"]);
    }

    #[test]
    fn rejects_zero_ingest_max_granules() {
        let mut env = valid_env();
        env.insert("INGEST_MAX_GRANULES", "0".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid INGEST_MAX_GRANULES"));
    }

    #[test]
    fn rejects_invalid_ingest_max_granules() {
        let mut env = valid_env();
        env.insert("INGEST_MAX_GRANULES", "many".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid INGEST_MAX_GRANULES"));
    }

    #[test]
    fn loads_http_request_timeout() {
        let mut env = valid_env();
        env.insert("HTTP_REQUEST_TIMEOUT_SECONDS", "45".to_owned());

        let config = load_from(env).unwrap();

        assert_eq!(config.http_request_timeout.as_secs(), 45);
    }

    #[test]
    fn rejects_zero_http_request_timeout() {
        let mut env = valid_env();
        env.insert("HTTP_REQUEST_TIMEOUT_SECONDS", "0".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid HTTP_REQUEST_TIMEOUT_SECONDS"));
    }

    #[test]
    fn rejects_invalid_http_request_timeout() {
        let mut env = valid_env();
        env.insert("HTTP_REQUEST_TIMEOUT_SECONDS", "slow".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid HTTP_REQUEST_TIMEOUT_SECONDS"));
    }

    #[test]
    fn loads_retry_configuration() {
        let mut env = valid_env();
        env.insert("HTTP_RETRY_MAX_ATTEMPTS", "5".to_owned());
        env.insert("HTTP_RETRY_BASE_DELAY_MS", "100".to_owned());
        env.insert("HTTP_RETRY_MAX_DELAY_MS", "1000".to_owned());

        let config = load_from(env).unwrap();

        assert_eq!(config.http_retry.max_attempts, 5);
        assert_eq!(config.http_retry.base_delay.as_millis(), 100);
        assert_eq!(config.http_retry.max_delay.as_millis(), 1000);
        assert_eq!(config.http_retry.delay_for_attempt(4).as_millis(), 800);
    }

    #[test]
    fn rejects_invalid_retry_configuration() {
        let mut env = valid_env();
        env.insert("HTTP_RETRY_MAX_ATTEMPTS", "0".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid HTTP_RETRY_MAX_ATTEMPTS"));
    }

    #[test]
    fn rejects_retry_base_delay_above_max_delay() {
        let mut env = valid_env();
        env.insert("HTTP_RETRY_BASE_DELAY_MS", "2000".to_owned());
        env.insert("HTTP_RETRY_MAX_DELAY_MS", "1000".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid HTTP_RETRY_BASE_DELAY_MS"));
    }
}
