use std::path::Path;

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
pub struct BlobStorageClient {
    http: reqwest::Client,
    account: String,
    access_key: String,
    endpoint: Url,
    retry: RetryConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteBlobOutcome {
    Deleted,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobListPage {
    pub blob_paths: Vec<String>,
    pub has_more: bool,
}

impl BlobStorageClient {
    pub fn new(config: &AppConfig) -> Result<Self, StorageError> {
        let endpoint = azure_storage::service_endpoint(
            &config.azure_storage_account,
            config.azure_storage_emulator_host.as_deref(),
            10000,
            "blob",
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

    pub async fn download_raw_blob_to_path(
        &self,
        container_name: &str,
        blob_path: &str,
        destination_path: &Path,
    ) -> Result<(), StorageError> {
        let url = self.raw_blob_url(container_name, blob_path)?;
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let authorization = self.authorization_header("GET", url.path(), &x_ms_date)?;

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
            let body = response.text().await.map_err(StorageError::ReadBlobBody)?;

            return Err(StorageError::BlobStatus {
                blob_path: blob_path.to_owned(),
                status,
                body,
            });
        }

        let bytes = response.bytes().await.map_err(StorageError::ReadBlobBody)?;

        tokio::fs::write(destination_path, bytes)
            .await
            .map_err(|source| StorageError::WriteBlobFile {
                blob_path: blob_path.to_owned(),
                path: destination_path.to_path_buf(),
                source,
            })?;

        Ok(())
    }

    pub(crate) fn raw_blob_url(
        &self,
        container_name: &str,
        blob_path: &str,
    ) -> Result<Url, StorageError> {
        azure_storage::blob_url(&self.endpoint, container_name, blob_path)
            .map_err(StorageError::from)
    }

    fn authorization_header(
        &self,
        method: &str,
        request_path: &str,
        x_ms_date: &str,
    ) -> Result<String, StorageError> {
        azure_storage::shared_key_authorization_header(azure_storage::SharedKeyRequest {
            account: &self.account,
            access_key: &self.access_key,
            method,
            request_path,
            content_length: None,
            content_type: None,
            canonicalized_query: &[],
            additional_canonicalized_headers: &[],
            x_ms_date,
        })
        .map_err(StorageError::AzureStorage)
    }

    pub async fn upload_processed_blob(
        &self,
        container_name: &str,
        blob_path: &str,
        bytes: &[u8],
        content_type: &str,
        cache_control: &str,
    ) -> Result<(), StorageError> {
        let url = azure_storage::blob_url(&self.endpoint, container_name, blob_path)
            .map_err(StorageError::from)?;
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let authorization = self.processed_blob_authorization_header(
            "PUT",
            url.path(),
            bytes.len(),
            content_type,
            cache_control,
            &x_ms_date,
        )?;

        let request = self
            .http
            .put(url)
            .header("authorization", authorization)
            .header("x-ms-date", x_ms_date)
            .header("x-ms-version", "2023-11-03")
            .header("x-ms-blob-type", "BlockBlob")
            .header("x-ms-blob-content-type", content_type)
            .header("x-ms-blob-cache-control", cache_control)
            .header("content-length", bytes.len().to_string())
            .body(bytes.to_vec());

        let response = send_with_retry(
            request,
            self.retry,
            RetryContext {
                dependency: "azure_blob_storage",
                operation: "processed_blob_upload",
                idempotency: RetryIdempotency::Idempotent,
            },
        )
        .await
        .map_err(StorageError::Request)?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.map_err(StorageError::ReadBlobBody)?;

            return Err(StorageError::ProcessedBlobUploadStatus {
                blob_path: blob_path.to_owned(),
                status,
                body,
            });
        }

        Ok(())
    }

    pub async fn delete_blob(
        &self,
        container_name: &str,
        blob_path: &str,
    ) -> Result<DeleteBlobOutcome, StorageError> {
        let url = azure_storage::blob_url(&self.endpoint, container_name, blob_path)
            .map_err(StorageError::from)?;
        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let authorization = self.authorization_header("DELETE", url.path(), &x_ms_date)?;

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
                dependency: "azure_blob_storage",
                operation: "blob_retention_delete",
                idempotency: RetryIdempotency::Idempotent,
            },
        )
        .await
        .map_err(StorageError::Request)?;

        let status = response.status();
        if status.is_success() {
            return Ok(DeleteBlobOutcome::Deleted);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(DeleteBlobOutcome::Missing);
        }

        let body = response.text().await.map_err(StorageError::ReadBlobBody)?;

        Err(StorageError::BlobDeleteStatus {
            blob_path: blob_path.to_owned(),
            status,
            body,
        })
    }

    pub async fn list_blobs_with_prefix(
        &self,
        container_name: &str,
        prefix: &str,
        max_results: u32,
    ) -> Result<BlobListPage, StorageError> {
        let mut url = self.container_url(container_name)?;
        url.query_pairs_mut()
            .append_pair("comp", "list")
            .append_pair("restype", "container")
            .append_pair("prefix", prefix)
            .append_pair("maxresults", &max_results.to_string());

        let x_ms_date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let canonicalized_query = [
            "comp:list".to_owned(),
            format!("maxresults:{max_results}"),
            format!("prefix:{prefix}"),
            "restype:container".to_owned(),
        ];
        let canonicalized_query = canonicalized_query
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let authorization =
            self.list_authorization_header("GET", url.path(), &canonicalized_query, &x_ms_date)?;

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
                operation: "blob_retention_list",
                idempotency: RetryIdempotency::Idempotent,
            },
        )
        .await
        .map_err(StorageError::Request)?;

        let status = response.status();
        let body = response.text().await.map_err(StorageError::ReadBlobBody)?;

        if !status.is_success() {
            return Err(StorageError::BlobListStatus {
                prefix: prefix.to_owned(),
                status,
                body,
            });
        }

        parse_blob_list_response(&body)
    }

    fn container_url(&self, container_name: &str) -> Result<Url, StorageError> {
        let mut url = self.endpoint.clone();

        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| StorageError::CannotBeBaseUrl)?;
            segments.push(container_name);
        }

        Ok(url)
    }

    fn processed_blob_authorization_header(
        &self,
        method: &str,
        request_path: &str,
        content_length: usize,
        content_type: &str,
        cache_control: &str,
        x_ms_date: &str,
    ) -> Result<String, StorageError> {
        let additional_headers = [
            format!("x-ms-blob-cache-control:{cache_control}"),
            format!("x-ms-blob-content-type:{content_type}"),
            "x-ms-blob-type:BlockBlob".to_owned(),
        ];
        let additional_headers = additional_headers
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        azure_storage::shared_key_authorization_header(azure_storage::SharedKeyRequest {
            account: &self.account,
            access_key: &self.access_key,
            method,
            request_path,
            content_length: Some(content_length),
            content_type: None,
            canonicalized_query: &[],
            additional_canonicalized_headers: &additional_headers,
            x_ms_date,
        })
        .map_err(StorageError::AzureStorage)
    }

    fn list_authorization_header(
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
    fn processed_blob_string_to_sign(
        &self,
        method: &str,
        request_path: &str,
        content_length: usize,
        content_type: &str,
        cache_control: &str,
        x_ms_date: &str,
    ) -> String {
        let additional_headers = [
            format!("x-ms-blob-cache-control:{cache_control}"),
            format!("x-ms-blob-content-type:{content_type}"),
            "x-ms-blob-type:BlockBlob".to_owned(),
        ];
        let additional_headers = additional_headers
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        azure_storage::storage_string_to_sign_with_headers(
            azure_storage::StorageStringToSignRequest {
                account: &self.account,
                method,
                request_path,
                content_length: Some(content_length),
                content_type: None,
                canonicalized_query: &[],
                additional_canonicalized_headers: &additional_headers,
                x_ms_date,
            },
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename = "EnumerationResults")]
struct BlobListResponse {
    #[serde(rename = "Blobs")]
    blobs: Option<BlobListEntries>,
    #[serde(rename = "NextMarker")]
    next_marker: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BlobListEntries {
    #[serde(rename = "Blob", default)]
    blobs: Vec<RawBlobEntry>,
}

#[derive(Debug, Deserialize)]
struct RawBlobEntry {
    #[serde(rename = "Name")]
    name: String,
}

fn parse_blob_list_response(body: &str) -> Result<BlobListPage, StorageError> {
    let response: BlobListResponse =
        quick_xml::de::from_str(body).map_err(StorageError::ParseBlobListResponse)?;

    let blob_paths = response
        .blobs
        .map(|entries| entries.blobs.into_iter().map(|blob| blob.name).collect())
        .unwrap_or_default();
    let has_more = response
        .next_marker
        .as_deref()
        .is_some_and(|marker| !marker.is_empty());

    Ok(BlobListPage {
        blob_paths,
        has_more,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_blob_list_response, BlobStorageClient};
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
    fn builds_emulator_raw_blob_url() {
        let client = BlobStorageClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let url = client
            .raw_blob_url("raw-viirs", "VNP46A2/2026-05-21/h11v06.h5")
            .unwrap();

        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:10000/devstoreaccount1/raw-viirs/VNP46A2/2026-05-21/h11v06.h5"
        );
    }

    #[test]
    fn builds_azure_raw_blob_url() {
        let client = BlobStorageClient::new(&config_with_emulator_host(None)).unwrap();

        let url = client
            .raw_blob_url("raw-viirs", "VNP46A2/2026-05-21/h11v06.h5")
            .unwrap();

        assert_eq!(
            url.as_str(),
            "https://devstoreaccount1.blob.core.windows.net/raw-viirs/VNP46A2/2026-05-21/h11v06.h5"
        );
    }

    #[test]
    fn processed_blob_upload_signature_includes_cache_and_content_type_headers() {
        let client = BlobStorageClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();

        let value = client.processed_blob_string_to_sign(
            "PUT",
            "/devstoreaccount1/processed-tiles/manifests/latest.json",
            123,
            "application/json",
            "public, max-age=300, must-revalidate",
            "Sun, 24 May 2026 12:00:00 GMT",
        );

        assert!(value.contains("x-ms-blob-cache-control:public, max-age=300, must-revalidate"));
        assert!(value.contains("x-ms-blob-content-type:application/json"));
        assert!(value.contains("x-ms-blob-type:BlockBlob"));
        assert!(value
            .contains("/devstoreaccount1/devstoreaccount1/processed-tiles/manifests/latest.json"));
    }

    #[test]
    fn parses_blob_list_response_names() {
        let body = r#"
            <EnumerationResults>
              <Blobs>
                <Blob><Name>tiles/set-a/3/1/2.png</Name></Blob>
                <Blob><Name>tiles/set-a/3/1/3.png</Name></Blob>
              </Blobs>
            </EnumerationResults>
        "#;

        let page = parse_blob_list_response(body).unwrap();

        assert_eq!(
            page.blob_paths,
            vec![
                "tiles/set-a/3/1/2.png".to_owned(),
                "tiles/set-a/3/1/3.png".to_owned()
            ]
        );
        assert!(!page.has_more);
    }

    #[test]
    fn parses_blob_list_response_continuation_marker() {
        let body = r#"
            <EnumerationResults>
              <Blobs>
                <Blob><Name>tiles/set-a/3/1/2.png</Name></Blob>
              </Blobs>
              <NextMarker>marker-1</NextMarker>
            </EnumerationResults>
        "#;

        let page = parse_blob_list_response(body).unwrap();

        assert_eq!(page.blob_paths, vec!["tiles/set-a/3/1/2.png".to_owned()]);
        assert!(page.has_more);
    }

    #[test]
    fn blob_list_signature_uses_ordered_query_lines() {
        let client = BlobStorageClient::new(&config_with_emulator_host(Some("127.0.0.1"))).unwrap();
        let url = client.container_url("processed-tiles").unwrap();
        let value = shared::azure_storage::storage_string_to_sign(
            &client.account,
            "GET",
            url.path(),
            None,
            None,
            &[
                "comp:list",
                "maxresults:500",
                "prefix:tiles/set-a/",
                "restype:container",
            ],
            "Sun, 24 May 2026 12:00:00 GMT",
        );

        assert!(value.contains("comp:list"));
        assert!(value.contains("maxresults:500"));
        assert!(value.contains("prefix:tiles/set-a/"));
        assert!(value.contains("restype:container"));
        assert!(value.contains("/devstoreaccount1/devstoreaccount1/processed-tiles"));
    }
}
