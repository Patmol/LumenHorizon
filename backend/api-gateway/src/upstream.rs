use std::time::Duration;

use reqwest::{
    header::{HeaderName, HeaderValue},
    Client, StatusCode, Url,
};
use serde_json::Value;
use shared::{
    azure_storage,
    http_retry::{send_with_retry, RetryConfig, RetryContext, RetryIdempotency},
};
use url::Url as ExternalUrl;
use uuid::Uuid;

use crate::config::{IngestAdminConfig, InternalServiceAuthConfig, ProcessingQueueConfig};

const USER_AGENT: &str = "LumenHorizon api-gateway/0.1";

#[derive(Debug, Clone)]
pub struct IngestAdminClient {
    http: Client,
    base_url: ExternalUrl,
    internal_auth: Option<InternalServiceAuth>,
    retry: RetryConfig,
}

impl IngestAdminClient {
    pub fn new(
        config: &IngestAdminConfig,
        internal_auth: Option<&InternalServiceAuthConfig>,
        timeout: Duration,
        retry: RetryConfig,
    ) -> Result<Self, UpstreamError> {
        let http = build_http_client(timeout)?;
        let base_url = ExternalUrl::parse(&config.base_url).map_err(UpstreamError::InvalidUrl)?;
        let internal_auth = internal_auth
            .map(InternalServiceAuth::try_from_config)
            .transpose()?;

        Ok(Self {
            http,
            base_url,
            internal_auth,
            retry,
        })
    }

    pub async fn trigger_ingest(&self, request_id: Uuid) -> Result<Value, UpstreamError> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| UpstreamError::CannotBeBaseUrl)?
            .extend(["admin", "ingest", "trigger"]);

        let mut request = self
            .http
            .post(url)
            .header("x-request-id", request_id.to_string())
            .header("content-type", "application/json")
            .body("{}");
        if let Some(internal_auth) = &self.internal_auth {
            request = request.header(
                internal_auth.header_name.clone(),
                internal_auth.header_value.clone(),
            );
        }

        let response = send_with_retry(
            request,
            self.retry,
            RetryContext {
                dependency: "ingest_svc",
                operation: "admin_ingest_trigger",
                idempotency: RetryIdempotency::AtLeastOnce,
            },
        )
        .await
        .map_err(UpstreamError::Request)?;

        let status = response.status();
        let body = response.text().await.map_err(UpstreamError::ReadBody)?;

        if !status.is_success() {
            return Err(UpstreamError::Status {
                dependency: "ingest_svc",
                status,
                body,
            });
        }

        serde_json::from_str(&body).map_err(UpstreamError::ParseJson)
    }
}

#[derive(Debug, Clone)]
pub struct ProcessingQueueClient {
    http: Client,
    account: String,
    access_key: String,
    endpoint: Url,
    queue_name: String,
    retry: RetryConfig,
}

impl ProcessingQueueClient {
    pub fn new(
        config: &ProcessingQueueConfig,
        timeout: Duration,
        retry: RetryConfig,
    ) -> Result<Self, UpstreamError> {
        let endpoint = azure_storage::service_endpoint(
            &config.azure_storage_account,
            config.azure_storage_emulator_host.as_deref(),
            10001,
            "queue",
        )?;
        let http = build_http_client(timeout)?;

        azure_storage::validate_queue_name(&config.queue_name)?;

        Ok(Self {
            http,
            account: config.azure_storage_account.clone(),
            access_key: config.azure_storage_access_key.clone(),
            endpoint,
            queue_name: config.queue_name.clone(),
            retry,
        })
    }

    pub async fn enqueue_processing_message(
        &self,
        message_text: &str,
    ) -> Result<(), UpstreamError> {
        let url = self.queue_messages_url()?;
        let body = azure_storage::queue_message_body(message_text);
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let content_type = "application/xml";
        let authorization = self.authorization_header(
            "POST",
            url.path(),
            Some(body.len()),
            Some(content_type),
            &[],
            &x_ms_date,
        )?;

        let request = self
            .http
            .post(url)
            .header("authorization", authorization)
            .header("x-ms-date", x_ms_date)
            .header("x-ms-version", "2023-11-03")
            .header("content-type", content_type)
            .header("content-length", body.len().to_string())
            .body(body);

        let response = send_with_retry(
            request,
            self.retry,
            RetryContext {
                dependency: "azure_queue_storage",
                operation: "processing_message_requeue",
                idempotency: RetryIdempotency::AtLeastOnce,
            },
        )
        .await
        .map_err(UpstreamError::Request)?;

        let status = response.status();
        let body = response.text().await.map_err(UpstreamError::ReadBody)?;

        if !status.is_success() {
            return Err(UpstreamError::Status {
                dependency: "azure_queue_storage",
                status,
                body,
            });
        }

        Ok(())
    }

    async fn check(&self) -> Result<(), UpstreamError> {
        let url = self.queue_metadata_url()?;
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let canonicalized_query = ["comp:metadata"];
        let authorization = self.authorization_header(
            "GET",
            url.path(),
            None,
            None,
            &canonicalized_query,
            &x_ms_date,
        )?;

        let request = self
            .http
            .get(url)
            .header("authorization", authorization)
            .header("x-ms-date", x_ms_date)
            .header("x-ms-version", "2023-11-03");

        let response = send_with_retry(
            request,
            self.retry,
            RetryContext {
                dependency: "azure_queue_storage",
                operation: "processing_queue_health_check",
                idempotency: RetryIdempotency::Idempotent,
            },
        )
        .await
        .map_err(UpstreamError::Request)?;

        let status = response.status();
        let body = response.text().await.map_err(UpstreamError::ReadBody)?;

        if !status.is_success() {
            return Err(UpstreamError::Status {
                dependency: "azure_queue_storage",
                status,
                body,
            });
        }

        Ok(())
    }

    pub async fn health_check(&self) -> Result<(), UpstreamError> {
        self.check().await
    }

    fn queue_messages_url(&self) -> Result<Url, UpstreamError> {
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .map_err(|_| UpstreamError::CannotBeBaseUrl)?
            .extend([self.queue_name.as_str(), "messages"]);
        Ok(url)
    }

    fn queue_metadata_url(&self) -> Result<Url, UpstreamError> {
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .map_err(|_| UpstreamError::CannotBeBaseUrl)?
            .push(self.queue_name.as_str());
        url.query_pairs_mut().append_pair("comp", "metadata");
        Ok(url)
    }

    fn authorization_header(
        &self,
        method: &str,
        request_path: &str,
        content_length: Option<usize>,
        content_type: Option<&str>,
        canonicalized_query: &[&str],
        x_ms_date: &str,
    ) -> Result<String, UpstreamError> {
        azure_storage::shared_key_authorization_header(azure_storage::SharedKeyRequest {
            account: &self.account,
            access_key: &self.access_key,
            method,
            request_path,
            content_length,
            content_type,
            canonicalized_query,
            additional_canonicalized_headers: &[],
            x_ms_date,
        })
        .map_err(UpstreamError::AzureStorage)
    }
}

#[derive(Debug, Clone)]
struct InternalServiceAuth {
    header_name: HeaderName,
    header_value: HeaderValue,
}

impl InternalServiceAuth {
    fn try_from_config(config: &InternalServiceAuthConfig) -> Result<Self, UpstreamError> {
        let header_name = HeaderName::from_bytes(config.header_name.as_bytes())
            .map_err(UpstreamError::InvalidInternalAuthHeader)?;
        let header_value = HeaderValue::from_str(&config.token)
            .map_err(|_| UpstreamError::InvalidInternalAuthToken)?;

        Ok(Self {
            header_name,
            header_value,
        })
    }
}

fn build_http_client(timeout: Duration) -> Result<Client, UpstreamError> {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .build()
        .map_err(UpstreamError::BuildClient)
}

#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error(transparent)]
    AzureStorage(#[from] azure_storage::AzureStorageError),
    #[error("upstream error: endpoint cannot be used as base URL")]
    CannotBeBaseUrl,
    #[error("upstream error: failed to build HTTP client: {0}")]
    BuildClient(reqwest::Error),
    #[error("upstream error: invalid URL: {0}")]
    InvalidUrl(url::ParseError),
    #[error("upstream error: invalid internal service auth header")]
    InvalidInternalAuthHeader(reqwest::header::InvalidHeaderName),
    #[error("upstream error: invalid internal service auth token")]
    InvalidInternalAuthToken,
    #[error("upstream error: failed to parse JSON response: {0}")]
    ParseJson(serde_json::Error),
    #[error("upstream error: failed to read response body: {0}")]
    ReadBody(reqwest::Error),
    #[error("upstream error: request failed: {0}")]
    Request(reqwest::Error),
    #[error("upstream error: {dependency} returned {status}: {body}")]
    Status {
        dependency: &'static str,
        status: StatusCode,
        body: String,
    },
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::post,
        Json, Router,
    };
    use serde_json::json;
    use shared::http_retry::RetryConfig;
    use tokio::net::TcpListener;
    use uuid::Uuid;

    use crate::config::{IngestAdminConfig, InternalServiceAuthConfig};

    use super::IngestAdminClient;

    #[tokio::test]
    async fn ingest_admin_client_sends_internal_auth_header() {
        let seen_header = Arc::new(Mutex::new(None));
        let base_url = spawn_ingest_admin_server(Arc::clone(&seen_header)).await;
        let auth_value = ["test", "internal", "auth", "value"].join("-");
        let client = IngestAdminClient::new(
            &IngestAdminConfig { base_url },
            Some(&InternalServiceAuthConfig {
                header_name: "x-lumenhorizon-internal-token".to_owned(),
                token: auth_value.clone(),
            }),
            Duration::from_secs(5),
            RetryConfig {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                max_delay: Duration::from_millis(1),
            },
        )
        .unwrap();

        client.trigger_ingest(Uuid::new_v4()).await.unwrap();

        assert_eq!(
            seen_header.lock().unwrap().as_deref(),
            Some(auth_value.as_str())
        );
    }

    async fn spawn_ingest_admin_server(seen_header: Arc<Mutex<Option<String>>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/admin/ingest/trigger", post(capture_internal_auth))
            .with_state(seen_header);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("{}://{address}", "http")
    }

    async fn capture_internal_auth(
        State(seen_header): State<Arc<Mutex<Option<String>>>>,
        headers: HeaderMap,
    ) -> impl IntoResponse {
        *seen_header.lock().unwrap() = headers
            .get("x-lumenhorizon-internal-token")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        (StatusCode::OK, Json(json!({ "accepted": true })))
    }
}
