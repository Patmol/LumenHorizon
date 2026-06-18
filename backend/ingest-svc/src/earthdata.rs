use reqwest::StatusCode;

use crate::{
    clients::{build_http_client, send_with_retry, RetryContext, RetryIdempotency},
    config::{AppConfig, RetryConfig},
    models::GranuleCandidate,
};

#[derive(Debug, Clone)]
pub struct EarthdataClient {
    http: reqwest::Client,
    bearer_token: String,
    retry: RetryConfig,
}

impl EarthdataClient {
    pub fn new(config: &AppConfig) -> Result<Self, EarthdataError> {
        let bearer_token = config
            .earthdata_bearer_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .filter(|token| !token.eq_ignore_ascii_case("replace-me"))
            .ok_or(EarthdataError::MissingToken)?
            .to_owned();

        let http =
            build_http_client(config.http_request_timeout).map_err(EarthdataError::BuildClient)?;

        Ok(Self {
            http,
            bearer_token,
            retry: config.http_retry,
        })
    }

    pub async fn download(&self, granule: &GranuleCandidate) -> Result<Vec<u8>, EarthdataError> {
        tracing::info!(
            product = granule.product,
            granule_title = granule.title,
            granule_date = %granule.granule_date,
            tile_h = granule.tile.h,
            tile_v = granule.tile.v,
            "downloading Earthdata granule"
        );

        let request = self
            .http
            .get(&granule.data_href)
            .bearer_auth(&self.bearer_token);

        let response = send_with_retry(
            request,
            self.retry,
            RetryContext {
                dependency: "earthdata",
                operation: "granule_download",
                idempotency: RetryIdempotency::Idempotent,
            },
        )
        .await
        .map_err(EarthdataError::Request)?;

        let status = response.status();

        if !status.is_success() {
            return Err(EarthdataError::Status {
                granule_title: granule.title.clone(),
                status,
            });
        }

        let bytes = response.bytes().await.map_err(EarthdataError::ReadBody)?;

        if bytes.is_empty() {
            return Err(EarthdataError::EmptyBody {
                granule_title: granule.title.clone(),
            });
        }

        tracing::info!(
            product = granule.product,
            granule_title = granule.title,
            downloaded_bytes = bytes.len(),
            "Earthdata granule downloaded"
        );

        Ok(bytes.to_vec())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EarthdataError {
    #[error("Earthdata download error: failed to build HTTP client: {0}")]
    BuildClient(reqwest::Error),
    #[error("Earthdata download error: downloaded empty body granule {granule_title}")]
    EmptyBody { granule_title: String },
    #[error("Earthdata download error: EARTHDATA_BEARER_TOKEN is required for real ingest")]
    MissingToken,
    #[error("Earthdata download error: failed to read response body: {0}")]
    ReadBody(reqwest::Error),
    #[error("Earthdata download error: request failed {0}")]
    Request(reqwest::Error),
    #[error("Earthdata download error: upstream returned {status} for granule {granule_title}")]
    Status {
        granule_title: String,
        status: StatusCode,
    },
}

#[cfg(test)]
mod tests {
    use super::{EarthdataClient, EarthdataError};
    use crate::config::AppConfig;

    const TEST_STORAGE_ACCESS_KEY: &str = "dGVzdC1zdG9yYWdlLWFjY291bnQta2V5";

    fn config_with_token(token: Option<&str>) -> AppConfig {
        AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some(TEST_STORAGE_ACCESS_KEY.to_owned()),
            "EARTHDATA_BEARER_TOKEN" => token.map(str::to_owned),
            _ => None,
        })
        .unwrap()
    }

    #[test]
    fn rejects_missing_token() {
        let error = EarthdataClient::new(&config_with_token(None)).unwrap_err();

        assert!(matches!(error, EarthdataError::MissingToken));
    }

    #[test]
    fn rejects_placeholder_token() {
        let error = EarthdataClient::new(&config_with_token(Some("replace-me"))).unwrap_err();

        assert!(matches!(error, EarthdataError::MissingToken));
    }

    #[test]
    fn accepts_real_token() {
        EarthdataClient::new(&config_with_token(Some("real-token"))).unwrap();
    }
}
