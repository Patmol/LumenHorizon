use serde::Deserialize;
use shared::{
    azure_storage,
    http_retry::{send_with_retry, RetryConfig, RetryContext, RetryIdempotency},
};
use url::Url;

use crate::{
    config::AppConfig,
    storage::{build_http_client, StorageError},
};

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
        let endpoint = azure_storage::service_endpoint(
            &config.azure_storage_account,
            config.azure_storage_emulator_host.as_deref(),
            10001,
            "queue",
        )?;

        let http =
            build_http_client(config.http_request_timeout).map_err(StorageError::BuildClient)?;

        Ok(Self {
            http,
            account: config.azure_storage_account.clone(),
            access_key: config.azure_storage_access_key.clone(),
            endpoint,
            retry: config.http_retry,
        })
    }

    pub async fn receive_messages(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout_seconds: u64,
    ) -> Result<Vec<ReceivedQueueMessage>, StorageError> {
        let mut url = self.queue_messages_url(queue_name)?;

        url.query_pairs_mut()
            .append_pair("numofmessages", &max_messages.to_string())
            .append_pair("visibilitytimeout", &visibility_timeout_seconds.to_string());

        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let canonicalized_query = [
            format!("numofmessages:{max_messages}"),
            format!("visibilitytimeout:{visibility_timeout_seconds}"),
        ];
        let canonicalized_query = canonicalized_query
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let authorization =
            self.authorization_header("GET", url.path(), &canonicalized_query, &x_ms_date)?;

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
                operation: "processing_message_receive",
                idempotency: RetryIdempotency::Idempotent,
            },
        )
        .await
        .map_err(StorageError::Request)?;

        let status = response.status();
        let body = response.text().await.map_err(StorageError::ReadBody)?;

        if !status.is_success() {
            return Err(StorageError::QueueStatus {
                queue_name: queue_name.to_owned(),
                status,
                body,
            });
        }

        parse_queue_messages_response(&body)
    }

    pub async fn delete_message(
        &self,
        queue_name: &str,
        message_id: &str,
        pop_receipt: &str,
    ) -> Result<(), StorageError> {
        let mut url = self.queue_message_url(queue_name, message_id)?;

        url.query_pairs_mut().append_pair("popreceipt", pop_receipt);

        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let canonicalized_query = [format!("popreceipt:{pop_receipt}")];
        let canonicalized_query = canonicalized_query
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let authorization =
            self.authorization_header("DELETE", url.path(), &canonicalized_query, &x_ms_date)?;

        let request = self
            .http
            .delete(url)
            .header("authorization", authorization)
            .header("x-ms-date", x_ms_date)
            .header("x-ms-version", "2023-11-03");

        let response = send_with_retry(
            request,
            self.retry,
            RetryContext {
                dependency: "azure_queue_storage",
                operation: "processing_message_delete",
                idempotency: RetryIdempotency::Idempotent,
            },
        )
        .await
        .map_err(StorageError::Request)?;

        let status = response.status();
        let body = response.text().await.map_err(StorageError::ReadBody)?;

        if !status.is_success() {
            return Err(StorageError::QueueStatus {
                queue_name: queue_name.to_owned(),
                status,
                body,
            });
        }

        Ok(())
    }

    pub async fn enqueue_message(
        &self,
        queue_name: &str,
        message_text: &str,
    ) -> Result<(), StorageError> {
        let url = self.queue_messages_url(queue_name)?;
        let body = azure_storage::queue_message_body(message_text);
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let content_type = "application/xml";
        let authorization = self.body_authorization_header(
            "POST",
            url.path(),
            body.len(),
            content_type,
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
                operation: "processing_deadletter_enqueue",
                idempotency: RetryIdempotency::AtLeastOnce,
            },
        )
        .await
        .map_err(StorageError::Request)?;

        let status = response.status();
        let body = response.text().await.map_err(StorageError::ReadBody)?;

        if !status.is_success() {
            return Err(StorageError::QueueStatus {
                queue_name: queue_name.to_owned(),
                status,
                body,
            });
        }

        Ok(())
    }

    pub(crate) fn queue_messages_url(&self, queue_name: &str) -> Result<Url, StorageError> {
        azure_storage::validate_queue_name(queue_name)?;

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

    pub(crate) fn queue_message_url(
        &self,
        queue_name: &str,
        message_id: &str,
    ) -> Result<Url, StorageError> {
        let mut url = self.queue_messages_url(queue_name)?;

        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| StorageError::CannotBeBaseUrl)?;

            segments.push(message_id);
        }

        Ok(url)
    }

    pub(crate) fn authorization_header(
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

    fn body_authorization_header(
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedQueueMessage {
    pub message_id: String,
    pub pop_receipt: String,
    pub dequeue_count: u32,
    pub message_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "QueueMessagesList")]
struct QueueMessagesResponse {
    #[serde(rename = "QueueMessage", default)]
    messages: Vec<RawQueueMessage>,
}

#[derive(Debug, Deserialize)]
struct RawQueueMessage {
    #[serde(rename = "MessageId")]
    message_id: String,
    #[serde(rename = "PopReceipt")]
    pop_receipt: String,
    #[serde(rename = "DequeueCount")]
    dequeue_count: u32,
    #[serde(rename = "MessageText")]
    message_text: String,
}

impl From<RawQueueMessage> for ReceivedQueueMessage {
    fn from(message: RawQueueMessage) -> Self {
        Self {
            message_id: message.message_id,
            pop_receipt: message.pop_receipt,
            dequeue_count: message.dequeue_count,
            message_text: message.message_text,
        }
    }
}

fn parse_queue_messages_response(body: &str) -> Result<Vec<ReceivedQueueMessage>, StorageError> {
    let response: QueueMessagesResponse =
        quick_xml::de::from_str(body).map_err(StorageError::ParseQueueResponse)?;

    Ok(response.messages.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::{parse_queue_messages_response, QueueClient};
    use crate::config::AppConfig;
    use shared::azure_storage;

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
    fn parses_queue_messages_response() {
        let body = r#"
            <QueueMessagesList>
                <QueueMessage>
                    <MessageId>message-1</MessageId>
                    <PopReceipt>receipt-1</PopReceipt>
                    <DequeueCount>2</DequeueCount>
                    <MessageText>{"ingest_id":"example"}</MessageText>
                </QueueMessage>
            </QueueMessagesList>
        "#;

        let messages = parse_queue_messages_response(body).unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message_id, "message-1");
        assert_eq!(messages[0].pop_receipt, "receipt-1");
        assert_eq!(messages[0].dequeue_count, 2);
        assert_eq!(messages[0].message_text, r#"{"ingest_id":"example"}"#);
    }

    #[test]
    fn parses_empty_queue_messages_response() {
        let messages = parse_queue_messages_response("<QueueMessagesList />").unwrap();

        assert!(messages.is_empty());
    }

    #[test]
    fn receive_query_signing_uses_expected_query_lines() {
        let client = QueueClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();
        let url = client.queue_messages_url("viirs-processing").unwrap();

        let value = azure_storage::storage_string_to_sign(
            &client.account,
            "GET",
            url.path(),
            None,
            None,
            &["numofmessages:1", "visibilitytimeout:900"],
            "Sun, 24 May 2026 12:00:00 GMT",
        );

        assert!(value.contains("numofmessages:1"));
        assert!(value.contains("visibilitytimeout:900"));
        assert!(value.contains("/devstoreaccount1/devstoreaccount1/viirs-processing/messages"));
    }

    #[test]
    fn builds_azurite_queue_message_url() {
        let client = QueueClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let url = client
            .queue_message_url("viirs-processing", "message-1")
            .unwrap();

        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:10001/devstoreaccount1/viirs-processing/messages/message-1"
        );
    }

    #[test]
    fn delete_query_signing_uses_pop_receipt() {
        let client = QueueClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();
        let url = client
            .queue_message_url("viirs-processing", "message-1")
            .unwrap();

        let value = azure_storage::storage_string_to_sign(
            &client.account,
            "DELETE",
            url.path(),
            None,
            None,
            &["popreceipt:receipt-1"],
            "Sun, 24 May 2026 12:00:00 GMT",
        );

        assert!(value.contains("popreceipt:receipt-1"));
        assert!(value
            .contains("/devstoreaccount1/devstoreaccount1/viirs-processing/messages/message-1"));
    }
}
