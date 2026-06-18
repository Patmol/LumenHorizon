use shared::azure_storage;
use url::Url;

use crate::{
    clients::{build_http_client, send_with_retry, RetryContext, RetryIdempotency},
    config::{AppConfig, RetryConfig},
    models::{validate_raw_blob_path, RAW_VIIRS_CONTAINER},
};

use super::shared::StorageError;

#[derive(Debug, Clone)]
pub struct BlobStorageClient {
    http: reqwest::Client,
    account: String,
    access_key: String,
    endpoint: Url,
    retry: RetryConfig,
}

impl BlobStorageClient {
    pub fn new(config: &AppConfig) -> Result<Self, StorageError> {
        let http =
            build_http_client(config.http_request_timeout).map_err(StorageError::BuildClient)?;

        let endpoint = blob_service_endpoint(config)?;

        Ok(Self {
            http,
            account: config.azure_storage_account.clone(),
            access_key: config.azure_storage_access_key.clone(),
            endpoint,
            retry: config.http_retry,
        })
    }

    pub(crate) fn raw_blob_url(&self, blob_path: &str) -> Result<Url, StorageError> {
        validate_raw_blob_path(blob_path)?;

        azure_storage::blob_url(&self.endpoint, RAW_VIIRS_CONTAINER, blob_path)
            .map_err(StorageError::from)
    }

    pub async fn check_raw_container_access(&self) -> Result<(), StorageError> {
        let url = self.raw_container_list_url()?;
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let authorization = self.get_authorization_header(
            "GET",
            url.path(),
            &["comp:list", "maxresults:1", "restype:container"],
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
                dependency: "azure_blob_storage",
                operation: "raw_container_readiness",
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

            return Err(StorageError::BlobReadinessStatus { status, body });
        }

        Ok(())
    }

    fn raw_container_list_url(&self) -> Result<Url, StorageError> {
        let mut url = self.endpoint.clone();

        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| StorageError::CannotBeBaseUrl)?;

            segments.push(RAW_VIIRS_CONTAINER);
        }

        url.query_pairs_mut()
            .append_pair("restype", "container")
            .append_pair("comp", "list")
            .append_pair("maxresults", "1");

        Ok(url)
    }

    pub async fn upload_raw_blob(&self, blob_path: &str, bytes: &[u8]) -> Result<(), StorageError> {
        let url = self.raw_blob_url(blob_path)?;
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let authorization =
            self.authorization_header("PUT", url.path(), bytes.len(), &x_ms_date)?;

        let request = self
            .http
            .put(url)
            .header("authorization", authorization)
            .header("x-ms-date", x_ms_date)
            .header("x-ms-version", "2023-11-03")
            .header("x-ms-blob-type", "BlockBlob")
            .header("content-length", bytes.len().to_string())
            .body(bytes.to_vec());

        let response = send_with_retry(
            request,
            self.retry,
            RetryContext {
                dependency: "azure_blob_storage",
                operation: "raw_blob_upload",
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

            return Err(StorageError::UploadStatus {
                blob_path: blob_path.to_owned(),
                status,
                body,
            });
        }

        tracing::info!(
            blob_path,
            uploaded_bytes = bytes.len(),
            "raw VIIRS blob uploaded"
        );

        Ok(())
    }

    pub async fn download_raw_blob(&self, blob_path: &str) -> Result<Vec<u8>, StorageError> {
        let url = self.raw_blob_url(blob_path)?;
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let authorization = self.get_authorization_header("GET", url.path(), &[], &x_ms_date)?;

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
                dependency: "azure_blob_storage",
                operation: "raw_blob_download",
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

            return Err(StorageError::DownloadStatus {
                blob_path: blob_path.to_owned(),
                status,
                body,
            });
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(StorageError::ReadBody)
    }

    fn authorization_header(
        &self,
        method: &str,
        request_path: &str,
        content_length: usize,
        x_ms_date: &str,
    ) -> Result<String, StorageError> {
        azure_storage::shared_key_authorization_header(azure_storage::SharedKeyRequest {
            account: &self.account,
            access_key: &self.access_key,
            method,
            request_path,
            content_length: Some(content_length),
            content_type: None,
            canonicalized_query: &[],
            additional_canonicalized_headers: &["x-ms-blob-type:BlockBlob"],
            x_ms_date,
        })
        .map_err(StorageError::AzureStorage)
    }

    fn get_authorization_header(
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

    #[cfg(test)]
    fn string_to_sign(
        &self,
        method: &str,
        request_path: &str,
        content_length: usize,
        x_ms_date: &str,
    ) -> String {
        azure_storage::storage_string_to_sign_with_headers(
            azure_storage::StorageStringToSignRequest {
                account: &self.account,
                method,
                request_path,
                content_length: Some(content_length),
                content_type: None,
                canonicalized_query: &[],
                additional_canonicalized_headers: &["x-ms-blob-type:BlockBlob"],
                x_ms_date,
            },
        )
    }
}

fn blob_service_endpoint(config: &AppConfig) -> Result<Url, StorageError> {
    azure_storage::service_endpoint(
        &config.azure_storage_account,
        config.azure_storage_emulator_host.as_deref(),
        10000,
        "blob",
    )
    .map_err(StorageError::AzureStorage)
}

#[cfg(test)]
mod tests {
    use shared::azure_storage;

    use super::{BlobStorageClient, StorageError};
    use crate::config::AppConfig;

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
    fn builds_azurite_blob_url() {
        let client = BlobStorageClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let url = client.raw_blob_url("VNP46A2/2024-05-21/h11v06.h5").unwrap();

        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:10000/devstoreaccount1/raw-viirs/VNP46A2/2024-05-21/h11v06.h5"
        );
    }

    #[test]
    fn builds_cloud_blob_url() {
        let mut config = config_with_emulator_host(None);
        config.azure_storage_account = "lumenhorizonstorage".to_owned();

        let client = BlobStorageClient::new(&config).unwrap();

        let url = client.raw_blob_url("VNP46A2/2024-05-21/h11v06.h5").unwrap();

        assert_eq!(
            url.as_str(),
            "https://lumenhorizonstorage.blob.core.windows.net/raw-viirs/VNP46A2/2024-05-21/h11v06.h5"
        );
    }

    #[test]
    fn rejects_blob_path_with_container_prefix() {
        let client = BlobStorageClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let error = client
            .raw_blob_url("raw-viirs/VNP46A2/2024-05-21/h11v06.h5")
            .unwrap_err();

        assert!(matches!(error, StorageError::InvalidBlobPath { .. }));
    }

    #[test]
    fn rejects_absolute_blob_path() {
        let client = BlobStorageClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let error = client
            .raw_blob_url("/VNP46A2/2024-05-21/h11v06.h5")
            .unwrap_err();

        assert!(matches!(error, StorageError::InvalidBlobPath { .. }));
    }

    #[test]
    fn rejects_url_blob_path() {
        let client = BlobStorageClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let error = client
            .raw_blob_url("https://example.test/blob.h5")
            .unwrap_err();

        assert!(matches!(error, StorageError::InvalidBlobPath { .. }));
    }

    #[test]
    fn string_to_sign_uses_azurite_request_path_under_raw_container() {
        let client = BlobStorageClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();
        let url = client.raw_blob_url("VNP46A2/2024-05-21/h11v06.h5").unwrap();

        let value = client.string_to_sign("PUT", url.path(), 123, "Sun, 24 May 2026 12:00:00 GMT");

        assert!(value
            .contains("/devstoreaccount1/devstoreaccount1/raw-viirs/VNP46A2/2024-05-21/h11v06.h5"));
        assert!(!value.contains("/devstoreaccount1/raw-viirs/raw-viirs/"));
    }

    #[test]
    fn string_to_sign_uses_cloud_request_path_under_raw_container() {
        let mut config = config_with_emulator_host(None);
        config.azure_storage_account = "lumenhorizonstorage".to_owned();
        let client = BlobStorageClient::new(&config).unwrap();
        let url = client.raw_blob_url("VNP46A2/2024-05-21/h11v06.h5").unwrap();

        let value = client.string_to_sign("PUT", url.path(), 123, "Sun, 24 May 2026 12:00:00 GMT");

        assert!(value.contains("/lumenhorizonstorage/raw-viirs/VNP46A2/2024-05-21/h11v06.h5"));
        assert!(!value.contains("/lumenhorizonstorage/lumenhorizonstorage/"));
    }

    #[test]
    fn raw_container_list_url_targets_raw_container() {
        let client = BlobStorageClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let url = client.raw_container_list_url().unwrap();

        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:10000/devstoreaccount1/raw-viirs?restype=container&comp=list&maxresults=1"
        );
    }

    #[test]
    fn readiness_canonicalized_resource_includes_sorted_query_terms() {
        let value = azure_storage::canonicalized_resource(
            "devstoreaccount1",
            "/devstoreaccount1/raw-viirs",
            &["comp:list", "maxresults:1", "restype:container"],
        );

        assert_eq!(
            value,
            "/devstoreaccount1/devstoreaccount1/raw-viirs\ncomp:list\nmaxresults:1\nrestype:container"
        );
    }
}
