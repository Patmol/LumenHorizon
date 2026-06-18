use std::{fmt, time::Duration};

use shared::http_retry::RetryConfig;

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_RUST_LOG: &str = "api_gateway=info";
const DEFAULT_PROCESSED_TILES_CONTAINER: &str = "processed-tiles";
const DEFAULT_TILE_LATEST_CACHE_CONTROL: &str = "public, max-age=300, must-revalidate";
const DEFAULT_HTTP_RETRY_MAX_ATTEMPTS: u32 = 3;
const DEFAULT_HTTP_RETRY_BASE_DELAY_MS: u64 = 250;
const DEFAULT_HTTP_RETRY_MAX_DELAY_MS: u64 = 5_000;
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 5;
const DEFAULT_QUEUE_NAME: &str = "viirs-processing";
const DEFAULT_ADMIN_ROLE_CLAIM: &str = "roles";
const DEFAULT_ADMIN_REQUIRED_ROLE: &str = "lumenhorizon.admin";
const DEFAULT_JWKS_CACHE_TTL_SECONDS: u64 = 300;
const DEFAULT_JWT_CLOCK_SKEW_SECONDS: u64 = 60;
const DEFAULT_JWT_MAX_ADMIN_TOKEN_LIFETIME_SECONDS: u64 = 3600;
const DEFAULT_MAX_URL_LENGTH_BYTES: usize = 8192;
const DEFAULT_ADMIN_MAX_BODY_BYTES: u64 = 65_536;
const DEFAULT_PUBLIC_TIMEOUT_SECONDS: u64 = 5;
const DEFAULT_ADMIN_TIMEOUT_SECONDS: u64 = 15;
const DEFAULT_HEALTH_TIMEOUT_SECONDS: u64 = 2;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub rust_log: String,
    pub runtime_environment: RuntimeEnvironment,
    pub auth: AuthConfig,
    pub rate_limit: RateLimitConfig,
    pub database: Option<DatabaseConfig>,
    pub tile_manifest_storage: Option<TileManifestStorageConfig>,
    pub ingest_admin: Option<IngestAdminConfig>,
    pub internal_service_auth: Option<InternalServiceAuthConfig>,
    pub processing_queue: Option<ProcessingQueueConfig>,
    pub tile_latest_cache_control: String,
    pub http_retry: RetryConfig,
    pub max_url_length_bytes: usize,
    pub admin_max_body_bytes: u64,
    pub public_timeout: Duration,
    pub admin_timeout: Duration,
    pub health_timeout: Duration,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub(crate) fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut missing = Vec::new();
        let jwt_issuer = read_required(
            &lookup,
            "JWT_ISSUER",
            "admin JWT issuer for Microsoft Entra ID tokens",
            &mut missing,
        );
        let jwt_audience = read_required(
            &lookup,
            "JWT_AUDIENCE",
            "admin JWT audience accepted by api-gateway",
            &mut missing,
        );
        let jwks_url = read_required(
            &lookup,
            "JWKS_URL",
            "JWKS endpoint used to validate Entra ID token signatures",
            &mut missing,
        );

        let rate_limit_backend = parse_rate_limit_backend(optional_or_default(
            &lookup,
            "RATE_LIMIT_BACKEND",
            "memory".to_owned(),
        ))?;
        let runtime_environment = parse_runtime_environment(optional_or_default(
            &lookup,
            "RUNTIME_ENVIRONMENT",
            "local".to_owned(),
        ))?;
        let database = parse_database_config(&lookup)?;
        let tile_manifest_storage = parse_tile_manifest_storage(&lookup, &mut missing);
        let ingest_admin = parse_ingest_admin_config(&lookup)?;
        let internal_service_auth = parse_internal_service_auth_config(&lookup)?;
        let processing_queue = parse_processing_queue_config(&lookup, &mut missing);

        let redis_url = read_optional(&lookup, "REDIS_URL");
        if rate_limit_backend == RateLimitBackend::Redis && redis_url.is_none() {
            missing.push(MissingVariable {
                name: "REDIS_URL",
                purpose: "Redis-compatible distributed rate-limit store URL required when RATE_LIMIT_BACKEND=redis",
            });
        } else if runtime_environment.requires_distributed_rate_limit()
            && rate_limit_backend != RateLimitBackend::Redis
        {
            missing.push(MissingVariable {
                name: "REDIS_URL",
                purpose:
                    "Redis-compatible distributed rate-limit store required outside local/dev profile",
            });
        }

        let tenant_id = read_optional(&lookup, "JWT_TENANT_ID");
        if runtime_environment.requires_tenant_bound_auth() && tenant_id.is_none() {
            missing.push(MissingVariable {
                name: "JWT_TENANT_ID",
                purpose: "Microsoft Entra tenant id required outside local/dev profile",
            });
        }
        if runtime_environment.requires_tenant_bound_auth()
            && ingest_admin.is_some()
            && internal_service_auth.is_none()
        {
            missing.push(MissingVariable {
                name: "INTERNAL_SERVICE_AUTH_TOKEN",
                purpose: "service-to-service token required for api-gateway admin calls to internal services outside local/dev profile",
            });
        }

        if !missing.is_empty() {
            return Err(ConfigError::MissingRequired { variables: missing });
        }

        if let Some(tenant_id) = tenant_id.as_deref() {
            validate_tenant_id(tenant_id)?;
        }
        if let Some(redis_url) = redis_url.as_deref() {
            validate_redis_url(redis_url)?;
        }

        if runtime_environment.requires_tenant_bound_auth() {
            validate_tenant_bound_url(
                "JWT_ISSUER",
                jwt_issuer
                    .as_deref()
                    .expect("missing required config returned above"),
            )?;
            validate_tenant_bound_url(
                "JWKS_URL",
                jwks_url
                    .as_deref()
                    .expect("missing required config returned above"),
            )?;
        }

        let port = parse_port(optional_or_default(
            &lookup,
            "PORT",
            DEFAULT_PORT.to_string(),
        ))?;
        let rust_log = optional_or_default(&lookup, "RUST_LOG", DEFAULT_RUST_LOG.to_owned());
        validate_rust_log(&rust_log)?;

        Ok(Self {
            port,
            rust_log,
            runtime_environment,
            auth: AuthConfig {
                issuer: jwt_issuer.expect("missing required config returned above"),
                audience: jwt_audience.expect("missing required config returned above"),
                jwks_url: jwks_url.expect("missing required config returned above"),
                tenant_id,
                admin_role_claim: optional_or_default(
                    &lookup,
                    "ADMIN_ROLE_CLAIM",
                    DEFAULT_ADMIN_ROLE_CLAIM.to_owned(),
                ),
                admin_required_role: optional_or_default(
                    &lookup,
                    "ADMIN_REQUIRED_ROLE",
                    DEFAULT_ADMIN_REQUIRED_ROLE.to_owned(),
                ),
                jwks_cache_ttl: parse_positive_duration_seconds(
                    "JWKS_CACHE_TTL_SECONDS",
                    optional_or_default(
                        &lookup,
                        "JWKS_CACHE_TTL_SECONDS",
                        DEFAULT_JWKS_CACHE_TTL_SECONDS.to_string(),
                    ),
                )?,
                jwt_clock_skew: parse_positive_duration_seconds(
                    "JWT_CLOCK_SKEW_SECONDS",
                    optional_or_default(
                        &lookup,
                        "JWT_CLOCK_SKEW_SECONDS",
                        DEFAULT_JWT_CLOCK_SKEW_SECONDS.to_string(),
                    ),
                )?,
                max_admin_token_lifetime: parse_positive_duration_seconds(
                    "JWT_MAX_ADMIN_TOKEN_LIFETIME_SECONDS",
                    optional_or_default(
                        &lookup,
                        "JWT_MAX_ADMIN_TOKEN_LIFETIME_SECONDS",
                        DEFAULT_JWT_MAX_ADMIN_TOKEN_LIFETIME_SECONDS.to_string(),
                    ),
                )?,
            },
            rate_limit: RateLimitConfig {
                backend: rate_limit_backend,
                redis_url,
                distributed_required: runtime_environment.requires_distributed_rate_limit(),
            },
            database,
            tile_manifest_storage,
            ingest_admin,
            internal_service_auth,
            processing_queue,
            tile_latest_cache_control: optional_or_default(
                &lookup,
                "TILE_LATEST_CACHE_CONTROL",
                DEFAULT_TILE_LATEST_CACHE_CONTROL.to_owned(),
            ),
            http_retry: parse_http_retry_config(&lookup)?,
            max_url_length_bytes: parse_usize(
                "MAX_URL_LENGTH_BYTES",
                optional_or_default(
                    &lookup,
                    "MAX_URL_LENGTH_BYTES",
                    DEFAULT_MAX_URL_LENGTH_BYTES.to_string(),
                ),
            )?,
            admin_max_body_bytes: parse_u64(
                "ADMIN_MAX_BODY_BYTES",
                optional_or_default(
                    &lookup,
                    "ADMIN_MAX_BODY_BYTES",
                    DEFAULT_ADMIN_MAX_BODY_BYTES.to_string(),
                ),
            )?,
            public_timeout: parse_positive_duration_seconds(
                "PUBLIC_ROUTE_TIMEOUT_SECONDS",
                optional_or_default(
                    &lookup,
                    "PUBLIC_ROUTE_TIMEOUT_SECONDS",
                    DEFAULT_PUBLIC_TIMEOUT_SECONDS.to_string(),
                ),
            )?,
            admin_timeout: parse_positive_duration_seconds(
                "ADMIN_ROUTE_TIMEOUT_SECONDS",
                optional_or_default(
                    &lookup,
                    "ADMIN_ROUTE_TIMEOUT_SECONDS",
                    DEFAULT_ADMIN_TIMEOUT_SECONDS.to_string(),
                ),
            )?,
            health_timeout: parse_positive_duration_seconds(
                "HEALTH_ROUTE_TIMEOUT_SECONDS",
                optional_or_default(
                    &lookup,
                    "HEALTH_ROUTE_TIMEOUT_SECONDS",
                    DEFAULT_HEALTH_TIMEOUT_SECONDS.to_string(),
                ),
            )?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub issuer: String,
    pub audience: String,
    pub jwks_url: String,
    pub tenant_id: Option<String>,
    pub admin_role_claim: String,
    pub admin_required_role: String,
    pub jwks_cache_ttl: Duration,
    pub jwt_clock_skew: Duration,
    pub max_admin_token_lifetime: Duration,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub backend: RateLimitBackend,
    pub redis_url: Option<String>,
    pub distributed_required: bool,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub database_url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone)]
pub struct TileManifestStorageConfig {
    pub azure_storage_account: String,
    pub azure_storage_access_key: String,
    pub azure_storage_emulator_host: Option<String>,
    pub processed_tiles_container: String,
}

#[derive(Debug, Clone)]
pub struct IngestAdminConfig {
    pub base_url: String,
}

#[derive(Debug, Clone)]
pub struct InternalServiceAuthConfig {
    pub header_name: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct ProcessingQueueConfig {
    pub azure_storage_account: String,
    pub azure_storage_access_key: String,
    pub azure_storage_emulator_host: Option<String>,
    pub queue_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitBackend {
    Memory,
    Redis,
}

impl RateLimitBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Redis => "redis",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEnvironment {
    Local,
    Dev,
    Staging,
    Prod,
}

impl RuntimeEnvironment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Prod => "prod",
        }
    }

    pub fn requires_distributed_rate_limit(self) -> bool {
        matches!(self, Self::Staging | Self::Prod)
    }

    pub fn requires_tenant_bound_auth(self) -> bool {
        matches!(self, Self::Staging | Self::Prod)
    }
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
        .map_err(|_| invalid_value("PORT", &value, "expected a TCP port between 1 and 65535"))?;

    if port == 0 {
        return Err(invalid_value(
            "PORT",
            &value,
            "expected a TCP port between 1 and 65535",
        ));
    }

    Ok(port)
}

fn parse_usize(variable: &'static str, value: String) -> Result<usize, ConfigError> {
    value.parse::<usize>().map_err(|_| {
        invalid_value(
            variable,
            &value,
            "expected a positive integer that fits in usize",
        )
    })
}

fn parse_u64(variable: &'static str, value: String) -> Result<u64, ConfigError> {
    value
        .parse::<u64>()
        .map_err(|_| invalid_value(variable, &value, "expected a positive integer"))
}

fn parse_positive_duration_seconds(
    variable: &'static str,
    value: String,
) -> Result<Duration, ConfigError> {
    let seconds = parse_u64(variable, value.clone())?;
    if seconds == 0 {
        return Err(invalid_value(
            variable,
            &value,
            "expected at least 1 second",
        ));
    }

    Ok(Duration::from_secs(seconds))
}

fn parse_positive_duration_millis(
    variable: &'static str,
    value: String,
) -> Result<Duration, ConfigError> {
    let millis = parse_u64(variable, value.clone())?;
    if millis == 0 {
        return Err(invalid_value(
            variable,
            &value,
            "expected at least 1 millisecond",
        ));
    }

    Ok(Duration::from_millis(millis))
}

fn parse_http_retry_config<F>(lookup: &F) -> Result<RetryConfig, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    let max_attempts = optional_or_default(
        lookup,
        "HTTP_RETRY_MAX_ATTEMPTS",
        DEFAULT_HTTP_RETRY_MAX_ATTEMPTS.to_string(),
    )
    .parse::<u32>()
    .map_err(|_| {
        invalid_value(
            "HTTP_RETRY_MAX_ATTEMPTS",
            &optional_or_default(
                lookup,
                "HTTP_RETRY_MAX_ATTEMPTS",
                DEFAULT_HTTP_RETRY_MAX_ATTEMPTS.to_string(),
            ),
            "expected a positive integer",
        )
    })?;

    if max_attempts == 0 {
        return Err(invalid_value(
            "HTTP_RETRY_MAX_ATTEMPTS",
            "0",
            "expected at least 1 attempt",
        ));
    }

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
        return Err(invalid_value(
            "HTTP_RETRY_BASE_DELAY_MS",
            &base_delay.as_millis().to_string(),
            "expected a value less than or equal to HTTP_RETRY_MAX_DELAY_MS",
        ));
    }

    Ok(RetryConfig {
        max_attempts,
        base_delay,
        max_delay,
    })
}

fn parse_database_config<F>(lookup: &F) -> Result<Option<DatabaseConfig>, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(database_url) = read_optional(lookup, "DATABASE_URL") else {
        return Ok(None);
    };

    let max_connections = optional_or_default(
        lookup,
        "DATABASE_MAX_CONNECTIONS",
        DEFAULT_DATABASE_MAX_CONNECTIONS.to_string(),
    )
    .parse::<u32>()
    .map_err(|_| {
        invalid_value(
            "DATABASE_MAX_CONNECTIONS",
            &optional_or_default(
                lookup,
                "DATABASE_MAX_CONNECTIONS",
                DEFAULT_DATABASE_MAX_CONNECTIONS.to_string(),
            ),
            "expected a positive integer",
        )
    })?;

    if max_connections == 0 {
        return Err(invalid_value(
            "DATABASE_MAX_CONNECTIONS",
            "0",
            "expected at least 1 connection",
        ));
    }

    Ok(Some(DatabaseConfig {
        database_url,
        max_connections,
    }))
}

fn parse_ingest_admin_config<F>(lookup: &F) -> Result<Option<IngestAdminConfig>, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(base_url) = read_optional(lookup, "INGEST_SERVICE_BASE_URL") else {
        return Ok(None);
    };

    let parsed = url::Url::parse(&base_url).map_err(|_| {
        invalid_value(
            "INGEST_SERVICE_BASE_URL",
            &base_url,
            "expected an absolute HTTP or HTTPS URL",
        )
    })?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(invalid_value(
            "INGEST_SERVICE_BASE_URL",
            &base_url,
            "expected an HTTP or HTTPS URL",
        ));
    }

    Ok(Some(IngestAdminConfig { base_url }))
}

fn parse_internal_service_auth_config<F>(
    lookup: &F,
) -> Result<Option<InternalServiceAuthConfig>, ConfigError>
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
    reqwest::header::HeaderName::from_bytes(header_name.as_bytes()).map_err(|_| {
        ConfigError::InvalidValue {
            variable: "INTERNAL_SERVICE_AUTH_HEADER",
            value: header_name.clone(),
            expected: "expected a valid HTTP header name",
        }
    })?;

    Ok(Some(InternalServiceAuthConfig { header_name, token }))
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

fn parse_tile_manifest_storage<F>(
    lookup: &F,
    missing: &mut Vec<MissingVariable>,
) -> Option<TileManifestStorageConfig>
where
    F: Fn(&str) -> Option<String>,
{
    let azure_storage_account = read_optional(lookup, "AZURE_STORAGE_ACCOUNT");
    let azure_storage_access_key = read_optional(lookup, "AZURE_STORAGE_ACCESS_KEY");
    let azure_storage_emulator_host = read_optional(lookup, "AZURE_STORAGE_EMULATOR_HOST");
    let processed_tiles_container = read_optional(lookup, "PROCESSED_TILES_CONTAINER");
    let has_any_storage_setting = azure_storage_account.is_some()
        || azure_storage_access_key.is_some()
        || azure_storage_emulator_host.is_some()
        || processed_tiles_container.is_some();

    if !has_any_storage_setting {
        return None;
    }

    if azure_storage_account.is_none() {
        missing.push(MissingVariable {
            name: "AZURE_STORAGE_ACCOUNT",
            purpose: "storage account that contains processed tile manifests",
        });
    }

    if azure_storage_access_key.is_none() {
        missing.push(MissingVariable {
            name: "AZURE_STORAGE_ACCESS_KEY",
            purpose: "storage account key used to read processed tile manifests",
        });
    }

    Some(TileManifestStorageConfig {
        azure_storage_account: azure_storage_account?,
        azure_storage_access_key: azure_storage_access_key?,
        azure_storage_emulator_host,
        processed_tiles_container: processed_tiles_container
            .unwrap_or_else(|| DEFAULT_PROCESSED_TILES_CONTAINER.to_owned()),
    })
}

fn parse_processing_queue_config<F>(
    lookup: &F,
    missing: &mut Vec<MissingVariable>,
) -> Option<ProcessingQueueConfig>
where
    F: Fn(&str) -> Option<String>,
{
    let azure_storage_account = read_optional(lookup, "AZURE_STORAGE_ACCOUNT");
    let azure_storage_access_key = read_optional(lookup, "AZURE_STORAGE_ACCESS_KEY");
    let azure_storage_emulator_host = read_optional(lookup, "AZURE_STORAGE_EMULATOR_HOST");
    let queue_name = read_optional(lookup, "AZURE_QUEUE_NAME");
    let has_any_queue_setting = azure_storage_account.is_some()
        || azure_storage_access_key.is_some()
        || azure_storage_emulator_host.is_some()
        || queue_name.is_some();

    if !has_any_queue_setting {
        return None;
    }

    if azure_storage_account.is_none() {
        missing.push(MissingVariable {
            name: "AZURE_STORAGE_ACCOUNT",
            purpose: "storage account that contains the processing queue",
        });
    }

    if azure_storage_access_key.is_none() {
        missing.push(MissingVariable {
            name: "AZURE_STORAGE_ACCESS_KEY",
            purpose: "storage account key used to write processing queue messages",
        });
    }

    Some(ProcessingQueueConfig {
        azure_storage_account: azure_storage_account?,
        azure_storage_access_key: azure_storage_access_key?,
        azure_storage_emulator_host,
        queue_name: queue_name.unwrap_or_else(|| DEFAULT_QUEUE_NAME.to_owned()),
    })
}

fn parse_rate_limit_backend(value: String) -> Result<RateLimitBackend, ConfigError> {
    match value.as_str() {
        "memory" => Ok(RateLimitBackend::Memory),
        "redis" => Ok(RateLimitBackend::Redis),
        _ => Err(invalid_value(
            "RATE_LIMIT_BACKEND",
            &value,
            "expected one of: memory, redis",
        )),
    }
}

fn parse_runtime_environment(value: String) -> Result<RuntimeEnvironment, ConfigError> {
    match value.as_str() {
        "local" => Ok(RuntimeEnvironment::Local),
        "dev" => Ok(RuntimeEnvironment::Dev),
        "staging" => Ok(RuntimeEnvironment::Staging),
        "prod" => Ok(RuntimeEnvironment::Prod),
        _ => Err(invalid_value(
            "RUNTIME_ENVIRONMENT",
            &value,
            "expected one of: local, dev, staging, prod",
        )),
    }
}

fn validate_rust_log(value: &str) -> Result<(), ConfigError> {
    tracing_subscriber::EnvFilter::try_new(value)
        .map(|_| ())
        .map_err(|_| invalid_value("RUST_LOG", value, "expected a valid tracing filter"))
}

fn validate_tenant_id(value: &str) -> Result<(), ConfigError> {
    if matches!(value, "common" | "organizations" | "consumers")
        || value.contains('/')
        || value.trim().is_empty()
    {
        return Err(invalid_value(
            "JWT_TENANT_ID",
            value,
            "expected a concrete Microsoft Entra tenant id, not common, organizations, or consumers",
        ));
    }

    Ok(())
}

fn validate_tenant_bound_url(variable: &'static str, value: &str) -> Result<(), ConfigError> {
    let lower = value.to_ascii_lowercase();
    if lower.contains("/common/")
        || lower.contains("/organizations/")
        || lower.contains("/consumers/")
    {
        return Err(invalid_value(
            variable,
            value,
            "expected a tenant-specific Microsoft Entra URL for staging and prod",
        ));
    }

    Ok(())
}

fn validate_redis_url(value: &str) -> Result<(), ConfigError> {
    let url = url::Url::parse(value).map_err(|_| ConfigError::InvalidSecretValue {
        variable: "REDIS_URL",
        expected: "expected a redis:// or rediss:// URL with a host",
    })?;
    if !matches!(url.scheme(), "redis" | "rediss") || url.host_str().is_none() {
        return Err(ConfigError::InvalidSecretValue {
            variable: "REDIS_URL",
            expected: "expected a redis:// or rediss:// URL with a host",
        });
    }

    Ok(())
}

fn invalid_value(variable: &'static str, value: &str, expected: &'static str) -> ConfigError {
    ConfigError::InvalidValue {
        variable,
        value: value.to_owned(),
        expected,
    }
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, RateLimitBackend};

    fn config_with(overrides: &[(&str, &str)]) -> Result<AppConfig, super::ConfigError> {
        AppConfig::from_lookup(|name| {
            overrides
                .iter()
                .find_map(|(key, value)| (*key == name).then(|| (*value).to_owned()))
                .or_else(|| match name {
                    "JWT_ISSUER" => Some("https://login.microsoftonline.com/test/v2.0".to_owned()),
                    "JWT_AUDIENCE" => Some("api://lumenhorizon-admin".to_owned()),
                    "JWKS_URL" => Some(
                        "https://login.microsoftonline.com/test/discovery/v2.0/keys".to_owned(),
                    ),
                    _ => None,
                })
        })
    }

    #[test]
    fn parses_defaults_for_local_gateway() {
        let config = config_with(&[]).unwrap();

        assert_eq!(config.port, 8080);
        assert!(config.tile_manifest_storage.is_none());
        assert_eq!(config.http_retry.max_attempts, 3);
        assert_eq!(config.auth.admin_role_claim, "roles");
        assert_eq!(config.auth.admin_required_role, "lumenhorizon.admin");
        assert!(config.auth.tenant_id.is_none());
        assert_eq!(config.rate_limit.backend, RateLimitBackend::Memory);
        assert!(!config.rate_limit.distributed_required);
        assert!(config.internal_service_auth.is_none());
    }

    #[test]
    fn staging_requires_distributed_rate_limit_configuration() {
        let error = config_with(&[("RUNTIME_ENVIRONMENT", "staging")])
            .unwrap_err()
            .to_string();

        assert!(error.contains("JWT_TENANT_ID"));
        assert!(error.contains("REDIS_URL"));
        assert!(error.contains("Microsoft Entra tenant id"));
        assert!(error.contains("Redis-compatible distributed rate-limit store"));
    }

    #[test]
    fn redis_backend_requires_redis_url() {
        let error = config_with(&[("RATE_LIMIT_BACKEND", "redis")])
            .unwrap_err()
            .to_string();

        assert!(error.contains("REDIS_URL"));
        assert!(error.contains("RATE_LIMIT_BACKEND=redis"));
    }

    #[test]
    fn invalid_redis_url_does_not_echo_secret() {
        let secret_url = "https://:super-secret@example.com";
        let error = config_with(&[("RATE_LIMIT_BACKEND", "redis"), ("REDIS_URL", secret_url)])
            .unwrap_err()
            .to_string();

        assert!(error.contains("REDIS_URL"));
        assert!(!error.contains(secret_url));
        assert!(!error.contains("super-secret"));
    }

    #[test]
    fn staging_accepts_tenant_bound_auth_and_redis_backend() {
        let config = config_with(&[
            ("RUNTIME_ENVIRONMENT", "staging"),
            ("RATE_LIMIT_BACKEND", "redis"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("JWT_TENANT_ID", "11111111-1111-1111-1111-111111111111"),
        ])
        .unwrap();

        assert_eq!(
            config.auth.tenant_id.as_deref(),
            Some("11111111-1111-1111-1111-111111111111")
        );
        assert_eq!(config.rate_limit.backend, RateLimitBackend::Redis);
        assert!(config.rate_limit.distributed_required);
    }

    #[test]
    fn staging_ingest_admin_requires_internal_service_auth() {
        let error = config_with(&[
            ("RUNTIME_ENVIRONMENT", "staging"),
            ("RATE_LIMIT_BACKEND", "redis"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("JWT_TENANT_ID", "11111111-1111-1111-1111-111111111111"),
            ("INGEST_SERVICE_BASE_URL", "https://ingest.internal"),
        ])
        .unwrap_err()
        .to_string();

        assert!(error.contains("INTERNAL_SERVICE_AUTH_TOKEN"));
        assert!(error.contains("service-to-service token"));
    }

    #[test]
    fn loads_internal_service_auth_without_echoing_secret() {
        let config = config_with(&[
            (
                "INTERNAL_SERVICE_AUTH_HEADER",
                "x-lumenhorizon-internal-token",
            ),
            (
                "INTERNAL_SERVICE_AUTH_TOKEN",
                "test-internal-service-token-value-123",
            ),
        ])
        .unwrap();

        let auth = config.internal_service_auth.unwrap();
        assert_eq!(auth.header_name, "x-lumenhorizon-internal-token");
        assert_eq!(auth.token, "test-internal-service-token-value-123");

        let error = config_with(&[("INTERNAL_SERVICE_AUTH_TOKEN", "short-secret")])
            .unwrap_err()
            .to_string();
        assert!(error.contains("INTERNAL_SERVICE_AUTH_TOKEN"));
        assert!(!error.contains("short-secret"));
    }

    #[test]
    fn staging_rejects_multi_tenant_auth_urls() {
        let error = config_with(&[
            ("RUNTIME_ENVIRONMENT", "staging"),
            ("RATE_LIMIT_BACKEND", "redis"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("JWT_TENANT_ID", "11111111-1111-1111-1111-111111111111"),
            (
                "JWT_ISSUER",
                "https://login.microsoftonline.com/common/v2.0",
            ),
        ])
        .unwrap_err()
        .to_string();

        assert!(error.contains("JWT_ISSUER"));
        assert!(error.contains("tenant-specific Microsoft Entra URL"));
    }

    #[test]
    fn rejects_multi_tenant_tenant_id_placeholders() {
        let error = config_with(&[("JWT_TENANT_ID", "common")])
            .unwrap_err()
            .to_string();

        assert!(error.contains("JWT_TENANT_ID"));
        assert!(error.contains("concrete Microsoft Entra tenant id"));
    }

    #[test]
    fn loads_tile_manifest_storage_when_configured() {
        let config = config_with(&[
            ("AZURE_STORAGE_ACCOUNT", "devstoreaccount1"),
            (
                "AZURE_STORAGE_ACCESS_KEY",
                "dGVzdC1zdG9yYWdlLWFjY291bnQta2V5",
            ),
            ("AZURE_STORAGE_EMULATOR_HOST", "127.0.0.1"),
            ("PROCESSED_TILES_CONTAINER", "processed-tiles"),
        ])
        .unwrap();

        let storage = config.tile_manifest_storage.unwrap();
        assert_eq!(storage.azure_storage_account, "devstoreaccount1");
        assert_eq!(
            storage.azure_storage_emulator_host.as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(storage.processed_tiles_container, "processed-tiles");
    }

    #[test]
    fn rejects_partial_tile_manifest_storage_configuration() {
        let error = config_with(&[("AZURE_STORAGE_ACCOUNT", "devstoreaccount1")])
            .unwrap_err()
            .to_string();

        assert!(error.contains("AZURE_STORAGE_ACCESS_KEY"));
        assert!(error.contains("processed tile manifests"));
    }
}
