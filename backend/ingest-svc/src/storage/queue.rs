use shared::azure_storage;
use url::Url;

use crate::{
    clients::{build_http_client, send_with_retry, RetryContext, RetryIdempotency},
    config::{AppConfig, RetryConfig},
    models::ProcessingMessage,
};

use super::shared::StorageError;

#[derive(Debug, Clone)]
pub struct QueueClient {
    http: reqwest::Client,
    account: String,
    access_key: String,
    endpoint: Url,
    retry: RetryConfig,
}

impl QueueClient {
    pub fn new(config: &AppConfig) -> Result<Self, StorageError> {
        let http =
            build_http_client(config.http_request_timeout).map_err(StorageError::BuildClient)?;

        let endpoint = queue_service_endpoint(config)?;

        Ok(Self {
            http,
            account: config.azure_storage_account.clone(),
            access_key: config.azure_storage_access_key.clone(),
            endpoint,
            retry: config.http_retry,
        })
    }

    pub(crate) fn queue_messages_url(&self, queue_name: &str) -> Result<Url, StorageError> {
        validate_queue_name(queue_name)?;

        let mut url = self.endpoint.clone();

        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| StorageError::CannotBeBaseUrl)?;

            segments.push(queue_name);
            segments.push("messages");
        }

        Ok(url)
    }

    pub async fn check_queue_access(&self, queue_name: &str) -> Result<(), StorageError> {
        let url = self.queue_metadata_url(queue_name)?;
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let authorization =
            self.readiness_authorization_header("GET", url.path(), &["comp:metadata"], &x_ms_date)?;

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
                operation: "queue_readiness",
                idempotency: RetryIdempotency::Idempotent,
            },
        )
        .await
        .map_err(StorageError::Request)?;

        let status = response.status();

        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_owned());

            return Err(StorageError::QueueReadinessStatus {
                queue_name: queue_name.to_owned(),
                status,
                body,
            });
        }

        Ok(())
    }

    fn queue_metadata_url(&self, queue_name: &str) -> Result<Url, StorageError> {
        validate_queue_name(queue_name)?;

        let mut url = self.endpoint.clone();

        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| StorageError::CannotBeBaseUrl)?;

            segments.push(queue_name);
        }

        url.query_pairs_mut().append_pair("comp", "metadata");

        Ok(url)
    }

    pub async fn enqueue_processing_message(
        &self,
        queue_name: &str,
        message: &ProcessingMessage,
    ) -> Result<(), StorageError> {
        let url = self.queue_messages_url(queue_name)?;
        let message_text =
            serde_json::to_string(message).map_err(StorageError::SerializeMessage)?;
        let body = azure_storage::queue_message_body(&message_text);
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let content_type = "application/xml";
        let authorization =
            self.authorization_header("POST", url.path(), body.len(), content_type, &x_ms_date)?;

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
                operation: "processing_message_enqueue",
                idempotency: RetryIdempotency::AtLeastOnce,
            },
        )
        .await
        .map_err(StorageError::Request)?;

        let status = response.status();

        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_owned());

            return Err(StorageError::QueueStatus {
                queue_name: queue_name.to_owned(),
                status,
                body,
            });
        }

        tracing::info!(
            queue_name,
            ingest_id = %message.ingest_id,
            blob_path = message.blob_path,
            "processing message enqueued"
        );

        Ok(())
    }

    fn authorization_header(
        &self,
        method: &str,
        request_path: &str,
        content_length: usize,
        content_type: &str,
        x_ms_date: &str,
    ) -> Result<String, StorageError> {
        azure_storage::shared_key_authorization_header(azure_storage::SharedKeyRequest {
            account: &self.account,
            access_key: &self.access_key,
            method,
            request_path,
            content_length: Some(content_length),
            content_type: Some(content_type),
            canonicalized_query: &[],
            additional_canonicalized_headers: &[],
            x_ms_date,
        })
        .map_err(StorageError::AzureStorage)
    }

    fn readiness_authorization_header(
        &self,
        method: &str,
        request_path: &str,
        canonicalized_query: &[&str],
        x_ms_date: &str,
    ) -> Result<String, StorageError> {
        azure_storage::shared_key_authorization_header(azure_storage::SharedKeyRequest {
            account: &self.account,
            access_key: &self.access_key,
            method,
            request_path,
            content_length: None,
            content_type: None,
            canonicalized_query,
            additional_canonicalized_headers: &[],
            x_ms_date,
        })
        .map_err(StorageError::AzureStorage)
    }
}

fn queue_service_endpoint(config: &AppConfig) -> Result<Url, StorageError> {
    azure_storage::service_endpoint(
        &config.azure_storage_account,
        config.azure_storage_emulator_host.as_deref(),
        10001,
        "queue",
    )
    .map_err(StorageError::AzureStorage)
}

fn validate_queue_name(queue_name: &str) -> Result<(), StorageError> {
    azure_storage::validate_queue_name(queue_name).map_err(StorageError::AzureStorage)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use shared::azure_storage;
    use uuid::Uuid;

    use super::QueueClient;
    use crate::{config::AppConfig, models::ProcessingMessage, storage::StorageError};

    const TEST_STORAGE_ACCESS_KEY: &str = "dGVzdC1zdG9yYWdlLWFjY291bnQta2V5";

    fn config_with_emulator_host(host: Option<&str>) -> AppConfig {
        AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some(TEST_STORAGE_ACCESS_KEY.to_owned()),
            "AZURE_STORAGE_EMULATOR_HOST" => host.map(str::to_owned),
            _ => None,
        })
        .unwrap()
    }

    fn processing_message() -> ProcessingMessage {
        ProcessingMessage {
            ingest_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            blob_path: "VNP46A2/2024-05-21/h11v06.h5".to_owned(),
            product: "VNP46A2".to_owned(),
            granule_date: Utc.with_ymd_and_hms(2024, 5, 21, 0, 0, 0).unwrap(),
            tile_h: 11,
            tile_v: 6,
        }
    }

    #[test]
    fn builds_azurite_queue_messages_url() {
        let client = QueueClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let url = client.queue_messages_url("viirs-processing").unwrap();

        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:10001/devstoreaccount1/viirs-processing/messages"
        );
    }

    #[test]
    fn builds_cloud_queue_messages_url() {
        let mut config = config_with_emulator_host(None);
        config.azure_storage_account = "lumenhorizonstorage".to_owned();
        let client = QueueClient::new(&config).unwrap();

        let url = client.queue_messages_url("viirs-processing").unwrap();

        assert_eq!(
            url.as_str(),
            "https://lumenhorizonstorage.queue.core.windows.net/viirs-processing/messages"
        );
    }

    #[test]
    fn rejects_invalid_queue_name() {
        let client = QueueClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let error = client.queue_messages_url("Invalid_Queue").unwrap_err();

        assert!(matches!(error, StorageError::AzureStorage(_)));
    }

    #[test]
    fn escapes_queue_message_xml_text() {
        assert_eq!(
            azure_storage::escape_xml_text("{\"value\":\"a&b<c>d\"}"),
            "{&quot;value&quot;:&quot;a&amp;b&lt;c&gt;d&quot;}"
        );
    }

    #[test]
    fn wraps_processing_message_as_queue_xml() {
        let json = serde_json::to_string(&processing_message()).unwrap();
        let body = azure_storage::queue_message_body(&json);

        assert!(body.starts_with("<QueueMessage><MessageText>"));
        assert!(body.ends_with("</MessageText></QueueMessage>"));
        assert!(body.contains("&quot;ingest_id&quot;"));
        assert!(body.contains("VNP46A2/2024-05-21/h11v06.h5"));
    }

    #[test]
    fn builds_azurite_queue_metadata_url() {
        let client = QueueClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let url = client.queue_metadata_url("viirs-processing").unwrap();

        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:10001/devstoreaccount1/viirs-processing?comp=metadata"
        );
    }
}
