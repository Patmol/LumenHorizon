use std::{fmt, time::Duration};

use crate::commands::Command;
use shared::http_retry::RetryConfig;

const DEFAULT_RUST_LOG: &str = "processing_svc=info";
const DEFAULT_QUEUE_NAME: &str = "viirs-processing";
const DEFAULT_DEADLETTER_QUEUE_NAME: &str = "viirs-processing-deadletter";
const DEFAULT_RAW_VIIRS_CONTAINER: &str = "raw-viirs";
const DEFAULT_PROCESSED_TILES_CONTAINER: &str = "processed-tiles";
const DEFAULT_MAX_CLOUD_FRACTION: f32 = 0.4;
const DEFAULT_VISIBILITY_TIMEOUT_SECONDS: u64 = 900;
const DEFAULT_MAX_DEQUEUE_COUNT: u32 = 5;
const DEFAULT_MAX_PARALLELISM: usize = 1;
const DEFAULT_HTTP_REQUEST_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_HTTP_RETRY_MAX_ATTEMPTS: u32 = 3;
const DEFAULT_HTTP_RETRY_BASE_DELAY_MS: u64 = 250;
const DEFAULT_HTTP_RETRY_MAX_DELAY_MS: u64 = 5_000;
const DEFAULT_TILE_MIN_ZOOM: u8 = 3;
const DEFAULT_TILE_MAX_NATIVE_ZOOM: u8 = 10;
const DEFAULT_TILE_MAX_DISPLAY_ZOOM: u8 = 12;
const DEFAULT_TILE_SIZE: u16 = 256;
const DEFAULT_TILE_FORMAT: &str = "png";
const DEFAULT_TILE_CLASSIFICATION_VERSION: &str = "radiance-dark-sky-v1";
const DEFAULT_TILE_RENDER_VERSION: &str = "tiles-v1";
const DEFAULT_TILE_CDN_BASE_URL: &str = "https://tiles.lumenhorizon.com";
const DEFAULT_TILE_BOUNDS: &str = "-125,24,-66,50";
const DEFAULT_TILE_IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
const DEFAULT_TILE_LATEST_CACHE_CONTROL: &str = "public, max-age=300, must-revalidate";
const DEFAULT_RAW_GRANULE_RETENTION_DAYS: u32 = 90;
const DEFAULT_PROCESSED_TILE_SET_RETENTION_DAYS: u32 = 180;
const DEFAULT_RETENTION_PROTECTED_PRIOR_TILE_SETS: u32 = 2;
const DEFAULT_RETENTION_BATCH_LIMIT: u32 = 500;
const DEFAULT_RETENTION_TILE_BLOB_LIMIT: u32 = 5_000;
const MAX_RETENTION_TILE_BLOB_LIMIT: u32 = 5_000;
const WEB_MERCATOR_LATITUDE_LIMIT: f64 = 85.051_128_78;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub rust_log: String,
    pub database_url: String,
    pub azure_storage_account: String,
    pub azure_storage_access_key: String,
    pub azure_storage_emulator_host: Option<String>,
    pub azure_queue_name: String,
    pub azure_deadletter_queue_name: String,
    pub raw_viirs_container: String,
    pub processed_tiles_container: String,
    pub max_cloud_fraction: f32,
    pub processing_visibility_timeout_seconds: u64,
    pub processing_max_dequeue_count: u32,
    pub processing_max_parallelism: usize,
    pub http_request_timeout: Duration,
    pub http_retry: RetryConfig,
    pub tile_min_zoom: u8,
    pub tile_max_native_zoom: u8,
    pub tile_max_display_zoom: u8,
    pub tile_size: u16,
    pub tile_format: String,
    pub tile_classification_version: String,
    pub tile_render_version: String,
    pub tile_cdn_base_url: String,
    pub tile_bounds: TileBounds,
    pub tile_immutable_cache_control: String,
    pub tile_latest_cache_control: String,
    pub raw_granule_retention_days: u32,
    pub processed_tile_set_retention_days: u32,
    pub retention_protected_prior_tile_sets: u32,
    pub retention_batch_limit: u32,
    pub retention_tile_blob_limit: u32,
}

impl AppConfig {
    pub fn from_env_for(command: &Command) -> Result<Self, ConfigError> {
        Self::from_lookup_for(command, |name| std::env::var(name).ok())
    }

    #[cfg(test)]
    pub(crate) fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        Self::from_lookup_for(&Command::Worker, lookup)
    }

    pub(crate) fn from_lookup_for<F>(_command: &Command, lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut missing = Vec::new();

        let database_url = read_required(
            &lookup,
            "DATABASE_URL",
            "PostgreSQL connection for ingest, processing, and tile metadata",
            &mut missing,
        );

        let azure_storage_account = read_required(
            &lookup,
            "AZURE_STORAGE_ACCOUNT",
            "storage account used for raw VIIRS blobs, processed tiles, and queue access",
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

        let tile_min_zoom = parse_u8_range(
            "TILE_MIN_ZOOM",
            optional_or_default(&lookup, "TILE_MIN_ZOOM", DEFAULT_TILE_MIN_ZOOM.to_string()),
            0,
            22,
        )?;

        let tile_max_native_zoom = parse_u8_range(
            "TILE_MAX_NATIVE_ZOOM",
            optional_or_default(
                &lookup,
                "TILE_MAX_NATIVE_ZOOM",
                DEFAULT_TILE_MAX_NATIVE_ZOOM.to_string(),
            ),
            tile_min_zoom,
            22,
        )?;

        let tile_max_display_zoom = parse_u8_range(
            "TILE_MAX_DISPLAY_ZOOM",
            optional_or_default(
                &lookup,
                "TILE_MAX_DISPLAY_ZOOM",
                DEFAULT_TILE_MAX_DISPLAY_ZOOM.to_string(),
            ),
            tile_max_native_zoom,
            22,
        )?;

        Ok(Self {
            rust_log: optional_or_default(&lookup, "RUST_LOG", DEFAULT_RUST_LOG.to_owned()),
            database_url: database_url.expect("missing required config returned above"),
            azure_storage_account: azure_storage_account.unwrap_or_default(),
            azure_storage_access_key: azure_storage_access_key.unwrap_or_default(),
            azure_storage_emulator_host: read_optional(&lookup, "AZURE_STORAGE_EMULATOR_HOST"),
            azure_queue_name: optional_or_default(
                &lookup,
                "AZURE_QUEUE_NAME",
                DEFAULT_QUEUE_NAME.to_owned(),
            ),
            azure_deadletter_queue_name: optional_or_default(
                &lookup,
                "AZURE_DEADLETTER_QUEUE_NAME",
                DEFAULT_DEADLETTER_QUEUE_NAME.to_owned(),
            ),
            raw_viirs_container: optional_or_default(
                &lookup,
                "RAW_VIIRS_CONTAINER",
                DEFAULT_RAW_VIIRS_CONTAINER.to_owned(),
            ),
            processed_tiles_container: optional_or_default(
                &lookup,
                "PROCESSED_TILES_CONTAINER",
                DEFAULT_PROCESSED_TILES_CONTAINER.to_owned(),
            ),
            max_cloud_fraction: parse_max_cloud_fraction(optional_or_default(
                &lookup,
                "MAX_CLOUD_FRACTION",
                DEFAULT_MAX_CLOUD_FRACTION.to_string(),
            ))?,
            processing_visibility_timeout_seconds: parse_positive_u64(
                "PROCESSING_VISIBILITY_TIMEOUT_SECONDS",
                optional_or_default(
                    &lookup,
                    "PROCESSING_VISIBILITY_TIMEOUT_SECONDS",
                    DEFAULT_VISIBILITY_TIMEOUT_SECONDS.to_string(),
                ),
            )?,
            processing_max_dequeue_count: parse_positive_u32(
                "PROCESSING_MAX_DEQUEUE_COUNT",
                optional_or_default(
                    &lookup,
                    "PROCESSING_MAX_DEQUEUE_COUNT",
                    DEFAULT_MAX_DEQUEUE_COUNT.to_string(),
                ),
            )?,
            processing_max_parallelism: parse_positive_usize(
                "PROCESSING_MAX_PARALLELISM",
                optional_or_default(
                    &lookup,
                    "PROCESSING_MAX_PARALLELISM",
                    DEFAULT_MAX_PARALLELISM.to_string(),
                ),
            )?,
            http_request_timeout: parse_positive_duration_seconds(
                "HTTP_REQUEST_TIMEOUT_SECONDS",
                optional_or_default(
                    &lookup,
                    "HTTP_REQUEST_TIMEOUT_SECONDS",
                    DEFAULT_HTTP_REQUEST_TIMEOUT_SECONDS.to_string(),
                ),
            )?,
            http_retry: parse_retry_config(&lookup)?,
            tile_min_zoom,
            tile_max_native_zoom,
            tile_max_display_zoom,
            tile_size: parse_positive_u16(
                "TILE_SIZE",
                optional_or_default(&lookup, "TILE_SIZE", DEFAULT_TILE_SIZE.to_string()),
            )?,
            tile_format: parse_tile_format(optional_or_default(
                &lookup,
                "TILE_FORMAT",
                DEFAULT_TILE_FORMAT.to_owned(),
            ))?,
            tile_classification_version: optional_or_default(
                &lookup,
                "TILE_CLASSIFICATION_VERSION",
                DEFAULT_TILE_CLASSIFICATION_VERSION.to_owned(),
            ),
            tile_render_version: optional_or_default(
                &lookup,
                "TILE_RENDER_VERSION",
                DEFAULT_TILE_RENDER_VERSION.to_owned(),
            ),
            tile_cdn_base_url: optional_or_default(
                &lookup,
                "TILE_CDN_BASE_URL",
                DEFAULT_TILE_CDN_BASE_URL.to_owned(),
            )
            .trim_end_matches('/')
            .to_owned(),
            tile_bounds: parse_tile_bounds(optional_or_default(
                &lookup,
                "TILE_BOUNDS",
                DEFAULT_TILE_BOUNDS.to_owned(),
            ))?,
            tile_immutable_cache_control: optional_or_default(
                &lookup,
                "TILE_IMMUTABLE_CACHE_CONTROL",
                DEFAULT_TILE_IMMUTABLE_CACHE_CONTROL.to_owned(),
            ),
            tile_latest_cache_control: optional_or_default(
                &lookup,
                "TILE_LATEST_CACHE_CONTROL",
                DEFAULT_TILE_LATEST_CACHE_CONTROL.to_owned(),
            ),
            raw_granule_retention_days: parse_positive_u32(
                "RAW_GRANULE_RETENTION_DAYS",
                optional_or_default(
                    &lookup,
                    "RAW_GRANULE_RETENTION_DAYS",
                    DEFAULT_RAW_GRANULE_RETENTION_DAYS.to_string(),
                ),
            )?,
            processed_tile_set_retention_days: parse_positive_u32(
                "PROCESSED_TILE_SET_RETENTION_DAYS",
                optional_or_default(
                    &lookup,
                    "PROCESSED_TILE_SET_RETENTION_DAYS",
                    DEFAULT_PROCESSED_TILE_SET_RETENTION_DAYS.to_string(),
                ),
            )?,
            retention_protected_prior_tile_sets: parse_u32_allow_zero(
                "RETENTION_PROTECTED_PRIOR_TILE_SETS",
                optional_or_default(
                    &lookup,
                    "RETENTION_PROTECTED_PRIOR_TILE_SETS",
                    DEFAULT_RETENTION_PROTECTED_PRIOR_TILE_SETS.to_string(),
                ),
            )?,
            retention_batch_limit: parse_positive_u32(
                "RETENTION_BATCH_LIMIT",
                optional_or_default(
                    &lookup,
                    "RETENTION_BATCH_LIMIT",
                    DEFAULT_RETENTION_BATCH_LIMIT.to_string(),
                ),
            )?,
            retention_tile_blob_limit: parse_positive_u32_at_most(
                "RETENTION_TILE_BLOB_LIMIT",
                optional_or_default(
                    &lookup,
                    "RETENTION_TILE_BLOB_LIMIT",
                    DEFAULT_RETENTION_TILE_BLOB_LIMIT.to_string(),
                ),
                MAX_RETENTION_TILE_BLOB_LIMIT,
            )?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
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
    pub name: &'static str,
    pub purpose: &'static str,
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
    match lookup(name).filter(|value| !value.trim().is_empty()) {
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

fn parse_positive_u64(variable: &'static str, value: String) -> Result<u64, ConfigError> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| ConfigError::InvalidValue {
            variable,
            value: value.clone(),
            expected: "must be a positive integer",
        })?;

    if parsed == 0 {
        return Err(ConfigError::InvalidValue {
            variable,
            value,
            expected: "must be greater than zero",
        });
    }

    Ok(parsed)
}

fn parse_positive_duration_seconds(
    variable: &'static str,
    value: String,
) -> Result<Duration, ConfigError> {
    parse_positive_u64(variable, value).map(Duration::from_secs)
}

fn parse_positive_duration_millis(
    variable: &'static str,
    value: String,
) -> Result<Duration, ConfigError> {
    parse_positive_u64(variable, value).map(Duration::from_millis)
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
            expected: "must be less than or equal to HTTP_RETRY_MAX_DELAY_MS",
        });
    }

    Ok(RetryConfig {
        max_attempts,
        base_delay,
        max_delay,
    })
}

fn parse_positive_u32(variable: &'static str, value: String) -> Result<u32, ConfigError> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| ConfigError::InvalidValue {
            variable,
            value: value.clone(),
            expected: "must be a positive integer",
        })?;

    if parsed == 0 {
        return Err(ConfigError::InvalidValue {
            variable,
            value,
            expected: "must be greater than zero",
        });
    }

    Ok(parsed)
}

fn parse_positive_u32_at_most(
    variable: &'static str,
    value: String,
    max: u32,
) -> Result<u32, ConfigError> {
    let parsed = parse_positive_u32(variable, value.clone())?;

    if parsed > max {
        return Err(ConfigError::InvalidValue {
            variable,
            value,
            expected: "must be within the allowed range",
        });
    }

    Ok(parsed)
}

fn parse_u32_allow_zero(variable: &'static str, value: String) -> Result<u32, ConfigError> {
    value.parse::<u32>().map_err(|_| ConfigError::InvalidValue {
        variable,
        value,
        expected: "must be a non-negative integer",
    })
}

fn parse_positive_usize(variable: &'static str, value: String) -> Result<usize, ConfigError> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| ConfigError::InvalidValue {
            variable,
            value: value.clone(),
            expected: "must be a positive integer",
        })?;

    if parsed == 0 {
        return Err(ConfigError::InvalidValue {
            variable,
            value,
            expected: "must be greater than zero",
        });
    }

    Ok(parsed)
}

fn parse_max_cloud_fraction(value: String) -> Result<f32, ConfigError> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| ConfigError::InvalidValue {
            variable: "MAX_CLOUD_FRACTION",
            value: value.clone(),
            expected: "must be a number between 0 and 1",
        })?;

    if !(0.0..=1.0).contains(&parsed) {
        return Err(ConfigError::InvalidValue {
            variable: "MAX_CLOUD_FRACTION",
            value,
            expected: "must be between 0 and 1",
        });
    }

    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

fn parse_u8_range(
    variable: &'static str,
    value: String,
    min: u8,
    max: u8,
) -> Result<u8, ConfigError> {
    let parsed = value.parse::<u8>().map_err(|_| ConfigError::InvalidValue {
        variable,
        value: value.clone(),
        expected: "must be an integer within the allowed range",
    })?;

    if parsed < min || parsed > max {
        return Err(ConfigError::InvalidValue {
            variable,
            value,
            expected: "must be within the allowed range",
        });
    }

    Ok(parsed)
}

fn parse_positive_u16(variable: &'static str, value: String) -> Result<u16, ConfigError> {
    let parsed = value
        .parse::<u16>()
        .map_err(|_| ConfigError::InvalidValue {
            variable,
            value: value.clone(),
            expected: "must be a positive integer",
        })?;

    if parsed == 0 {
        return Err(ConfigError::InvalidValue {
            variable,
            value,
            expected: "must be greater than zero",
        });
    }

    Ok(parsed)
}

fn parse_tile_format(value: String) -> Result<String, ConfigError> {
    let normalized = value.trim().to_ascii_lowercase();

    if normalized != "png" {
        return Err(ConfigError::InvalidValue {
            variable: "TILE_FORMAT",
            value,
            expected: "must be 'png'",
        });
    }

    Ok(normalized)
}

fn parse_tile_bounds(value: String) -> Result<TileBounds, ConfigError> {
    let parts = value.split(',').map(str::trim).collect::<Vec<_>>();

    if parts.len() != 4 {
        return Err(ConfigError::InvalidValue {
            variable: "TILE_BOUNDS",
            value,
            expected: "must use west,south,east,north",
        });
    }

    let west = parse_tile_bound_number(parts[0], &value)?;
    let south = parse_tile_bound_number(parts[1], &value)?;
    let east = parse_tile_bound_number(parts[2], &value)?;
    let north = parse_tile_bound_number(parts[3], &value)?;

    if !(-180.0..=180.0).contains(&west) || !(-180.0..=180.0).contains(&east) {
        return Err(ConfigError::InvalidValue {
            variable: "TILE_BOUNDS",
            value,
            expected: "longitude values must be between -180 and 180",
        });
    }

    if !(-WEB_MERCATOR_LATITUDE_LIMIT..=WEB_MERCATOR_LATITUDE_LIMIT).contains(&south)
        || !(-WEB_MERCATOR_LATITUDE_LIMIT..=WEB_MERCATOR_LATITUDE_LIMIT).contains(&north)
    {
        return Err(ConfigError::InvalidValue {
            variable: "TILE_BOUNDS",
            value,
            expected: "latitude values must be within Web Mercator limits",
        });
    }

    if west >= east || south >= north {
        return Err(ConfigError::InvalidValue {
            variable: "TILE_BOUNDS",
            value,
            expected: "west must be less than east and south must be less than north",
        });
    }

    Ok(TileBounds {
        west,
        south,
        east,
        north,
    })
}

fn parse_tile_bound_number(part: &str, full_value: &str) -> Result<f64, ConfigError> {
    part.parse::<f64>().map_err(|_| ConfigError::InvalidValue {
        variable: "TILE_BOUNDS",
        value: full_value.to_owned(),
        expected: "bounds must contain numeric longitude and latitude values",
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{AppConfig, ConfigError};
    use crate::commands::Command;

    fn valid_env() -> HashMap<&'static str, String> {
        HashMap::from([
            (
                "DATABASE_URL",
                "postgres://localhost/lumenhorizon".to_owned(),
            ),
            ("AZURE_STORAGE_ACCOUNT", "devstoreaccount1".to_owned()),
            (
                "AZURE_STORAGE_ACCESS_KEY",
                "dGVzdC1zdG9yYWdlLWFjY291bnQta2V5".to_owned(),
            ),
        ])
    }

    fn load_from(env: HashMap<&'static str, String>) -> Result<AppConfig, ConfigError> {
        AppConfig::from_lookup_for(&Command::Worker, |name| env.get(name).cloned())
    }

    #[test]
    fn loads_http_retry_defaults() {
        let config = load_from(valid_env()).unwrap();

        assert_eq!(config.http_request_timeout.as_secs(), 30);
        assert_eq!(config.http_retry.max_attempts, 3);
        assert_eq!(config.http_retry.base_delay.as_millis(), 250);
        assert_eq!(config.http_retry.max_delay.as_millis(), 5_000);
    }

    #[test]
    fn loads_retention_defaults() {
        let config = load_from(valid_env()).unwrap();

        assert_eq!(config.raw_granule_retention_days, 90);
        assert_eq!(config.processed_tile_set_retention_days, 180);
        assert_eq!(config.retention_protected_prior_tile_sets, 2);
        assert_eq!(config.retention_batch_limit, 500);
        assert_eq!(config.retention_tile_blob_limit, 5_000);
    }

    #[test]
    fn defaults_tile_cdn_base_url_to_production_cdn_when_unset() {
        let config = load_from(valid_env()).unwrap();

        assert_eq!(config.tile_cdn_base_url, "https://tiles.lumenhorizon.com");
    }

    #[test]
    fn loads_tile_cdn_base_url_override_and_trims_trailing_slash() {
        let mut env = valid_env();
        env.insert(
            "TILE_CDN_BASE_URL",
            "http://127.0.0.1:10000/devstoreaccount1/processed-tiles/".to_owned(),
        );

        let config = load_from(env).unwrap();

        assert_eq!(
            config.tile_cdn_base_url,
            "http://127.0.0.1:10000/devstoreaccount1/processed-tiles"
        );
    }

    #[test]
    fn loads_retention_overrides() {
        let mut env = valid_env();
        env.insert("RAW_GRANULE_RETENTION_DAYS", "120".to_owned());
        env.insert("PROCESSED_TILE_SET_RETENTION_DAYS", "365".to_owned());
        env.insert("RETENTION_PROTECTED_PRIOR_TILE_SETS", "0".to_owned());
        env.insert("RETENTION_BATCH_LIMIT", "25".to_owned());
        env.insert("RETENTION_TILE_BLOB_LIMIT", "2500".to_owned());

        let config = load_from(env).unwrap();

        assert_eq!(config.raw_granule_retention_days, 120);
        assert_eq!(config.processed_tile_set_retention_days, 365);
        assert_eq!(config.retention_protected_prior_tile_sets, 0);
        assert_eq!(config.retention_batch_limit, 25);
        assert_eq!(config.retention_tile_blob_limit, 2500);
    }

    #[test]
    fn loads_http_retry_overrides() {
        let mut env = valid_env();
        env.insert("HTTP_REQUEST_TIMEOUT_SECONDS", "45".to_owned());
        env.insert("HTTP_RETRY_MAX_ATTEMPTS", "5".to_owned());
        env.insert("HTTP_RETRY_BASE_DELAY_MS", "100".to_owned());
        env.insert("HTTP_RETRY_MAX_DELAY_MS", "1000".to_owned());

        let config = load_from(env).unwrap();

        assert_eq!(config.http_request_timeout.as_secs(), 45);
        assert_eq!(config.http_retry.max_attempts, 5);
        assert_eq!(config.http_retry.base_delay.as_millis(), 100);
        assert_eq!(config.http_retry.max_delay.as_millis(), 1000);
        assert_eq!(config.http_retry.delay_for_attempt(4).as_millis(), 800);
    }

    #[test]
    fn rejects_zero_http_retry_attempts() {
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

    #[test]
    fn rejects_zero_retention_windows_and_batch_limits() {
        for variable in [
            "RAW_GRANULE_RETENTION_DAYS",
            "PROCESSED_TILE_SET_RETENTION_DAYS",
            "RETENTION_BATCH_LIMIT",
            "RETENTION_TILE_BLOB_LIMIT",
        ] {
            let mut env = valid_env();
            env.insert(variable, "0".to_owned());

            let error = load_from(env).unwrap_err().to_string();

            assert!(error.contains(&format!("invalid {variable}")));
        }
    }

    #[test]
    fn rejects_retention_tile_blob_limit_above_azure_page_limit() {
        let mut env = valid_env();
        env.insert("RETENTION_TILE_BLOB_LIMIT", "5001".to_owned());

        let error = load_from(env).unwrap_err().to_string();

        assert!(error.contains("invalid RETENTION_TILE_BLOB_LIMIT"));
        assert!(error.contains("must be within the allowed range"));
    }
}
