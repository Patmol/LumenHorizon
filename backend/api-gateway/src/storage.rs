use std::{future::Future, pin::Pin, time::Duration};

use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde_json::Value;
use shared::{
    azure_storage,
    http_retry::{send_with_retry, RetryConfig, RetryContext, RetryIdempotency},
};

use crate::config::TileManifestStorageConfig;

pub type StorageFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StorageError>> + Send + 'a>>;

pub trait TileManifestStorage: Send + Sync {
    fn latest_manifest(&self) -> StorageFuture<'_, Value>;

    fn manifest_by_id<'a>(&'a self, tile_set_id: &'a str) -> StorageFuture<'a, Value>;
}

const USER_AGENT: &str = "LumenHorizon api-gateway/0.1";
const LATEST_MANIFEST_BLOB_PATH: &str = "manifests/latest.json";
const MAX_TILE_SET_ID_LENGTH: usize = 160;

#[derive(Debug, Clone)]
pub struct TileManifestStorageClient {
    http: Client,
    account: String,
    access_key: String,
    endpoint: Url,
    container_name: String,
    retry: RetryConfig,
}

impl TileManifestStorageClient {
    pub fn new(
        config: &TileManifestStorageConfig,
        timeout: Duration,
        retry: RetryConfig,
    ) -> Result<Self, StorageError> {
        let endpoint = azure_storage::service_endpoint(
            &config.azure_storage_account,
            config.azure_storage_emulator_host.as_deref(),
            10000,
            "blob",
        )?;
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(timeout)
            .build()
            .map_err(StorageError::BuildClient)?;

        Ok(Self {
            http,
            account: config.azure_storage_account.clone(),
            access_key: config.azure_storage_access_key.clone(),
            endpoint,
            container_name: config.processed_tiles_container.clone(),
            retry,
        })
    }

    pub async fn latest_manifest(&self) -> Result<Value, StorageError> {
        let pointer = self.latest_pointer().await?;
        validate_manifest_blob_path(&pointer.manifest_blob_path)?;

        self.read_json_blob(&pointer.manifest_blob_path).await
    }

    pub async fn manifest_by_id(&self, tile_set_id: &str) -> Result<Value, StorageError> {
        let blob_path = manifest_blob_path(tile_set_id)?;

        self.read_json_blob(&blob_path).await
    }

    async fn latest_pointer(&self) -> Result<LatestManifestPointer, StorageError> {
        let value = self.read_json_blob(LATEST_MANIFEST_BLOB_PATH).await?;

        serde_json::from_value(value).map_err(StorageError::ParseLatestPointer)
    }

    async fn read_json_blob(&self, blob_path: &str) -> Result<Value, StorageError> {
        let url = self.blob_url(blob_path)?;
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
                operation: "tile_manifest_download",
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
        serde_json::from_slice(&bytes).map_err(|source| StorageError::ParseBlobJson {
            blob_path: blob_path.to_owned(),
            source,
        })
    }

    fn blob_url(&self, blob_path: &str) -> Result<Url, StorageError> {
        azure_storage::blob_url(&self.endpoint, &self.container_name, blob_path)
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
}

impl TileManifestStorage for TileManifestStorageClient {
    fn latest_manifest(&self) -> StorageFuture<'_, Value> {
        Box::pin(TileManifestStorageClient::latest_manifest(self))
    }

    fn manifest_by_id<'a>(&'a self, tile_set_id: &'a str) -> StorageFuture<'a, Value> {
        Box::pin(TileManifestStorageClient::manifest_by_id(self, tile_set_id))
    }
}

#[derive(Debug, Deserialize)]
struct LatestManifestPointer {
    manifest_blob_path: String,
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    AzureStorage(#[from] azure_storage::AzureStorageError),
    #[error("storage error: blob read returned {status} for '{blob_path}': {body}")]
    BlobStatus {
        blob_path: String,
        status: StatusCode,
        body: String,
    },
    #[error("storage error: failed to build HTTP client: {0}")]
    BuildClient(reqwest::Error),
    #[error("storage error: invalid tile set id")]
    InvalidTileSetId,
    #[error("storage error: invalid latest manifest pointer")]
    InvalidLatestManifestPointer,
    #[error("storage error: failed to parse blob '{blob_path}' as JSON: {source}")]
    ParseBlobJson {
        blob_path: String,
        source: serde_json::Error,
    },
    #[error("storage error: failed to parse latest manifest pointer: {0}")]
    ParseLatestPointer(serde_json::Error),
    #[error("storage error: failed to read blob response body: {0}")]
    ReadBlobBody(reqwest::Error),
    #[error("storage error: request failed: {0}")]
    Request(reqwest::Error),
}

impl StorageError {
    pub fn is_blob_not_found(&self) -> bool {
        matches!(
            self,
            Self::BlobStatus {
                status: StatusCode::NOT_FOUND,
                ..
            }
        )
    }
}

fn manifest_blob_path(tile_set_id: &str) -> Result<String, StorageError> {
    validate_tile_set_id(tile_set_id)?;
    Ok(format!("manifests/{tile_set_id}.json"))
}

pub(crate) fn validate_tile_set_id(tile_set_id: &str) -> Result<(), StorageError> {
    let valid = !tile_set_id.trim().is_empty()
        && tile_set_id.len() <= MAX_TILE_SET_ID_LENGTH
        && tile_set_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && !tile_set_id.contains("..")
        && !tile_set_id.starts_with("processed-tiles");

    valid.then_some(()).ok_or(StorageError::InvalidTileSetId)
}

fn validate_manifest_blob_path(blob_path: &str) -> Result<(), StorageError> {
    let valid = blob_path.starts_with("manifests/")
        && blob_path.ends_with(".json")
        && blob_path != LATEST_MANIFEST_BLOB_PATH
        && !blob_path.contains("..")
        && !blob_path.contains('\\')
        && !blob_path.trim().is_empty();

    valid
        .then_some(())
        .ok_or(StorageError::InvalidLatestManifestPointer)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use shared::http_retry::RetryConfig;

    use super::{manifest_blob_path, validate_manifest_blob_path, TileManifestStorageClient};
    use crate::config::TileManifestStorageConfig;

    const TEST_STORAGE_ACCESS_KEY: &str = "dGVzdC1zdG9yYWdlLWFjY291bnQta2V5";

    fn client() -> TileManifestStorageClient {
        TileManifestStorageClient::new(
            &TileManifestStorageConfig {
                azure_storage_account: "devstoreaccount1".to_owned(),
                azure_storage_access_key: TEST_STORAGE_ACCESS_KEY.to_owned(),
                azure_storage_emulator_host: Some("127.0.0.1".to_owned()),
                processed_tiles_container: "processed-tiles".to_owned(),
            },
            Duration::from_secs(5),
            RetryConfig {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                max_delay: Duration::from_millis(1),
            },
        )
        .unwrap()
    }

    #[test]
    fn builds_manifest_blob_path_for_tile_set() {
        assert_eq!(
            manifest_blob_path("2026-05-21-radiance-dark-sky-v1-a1b2c3d4").unwrap(),
            "manifests/2026-05-21-radiance-dark-sky-v1-a1b2c3d4.json"
        );
    }

    #[test]
    fn rejects_invalid_tile_set_ids() {
        assert!(manifest_blob_path("").is_err());
        assert!(manifest_blob_path("../secret").is_err());
        assert!(manifest_blob_path("processed-tiles/foo").is_err());
    }

    #[test]
    fn builds_emulator_blob_url() {
        let url = client().blob_url("manifests/latest.json").unwrap();

        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:10000/devstoreaccount1/processed-tiles/manifests/latest.json"
        );
    }

    #[test]
    fn rejects_latest_pointer_that_points_to_itself() {
        assert!(validate_manifest_blob_path("manifests/latest.json").is_err());
        assert!(validate_manifest_blob_path("manifests/tile-set-1.json").is_ok());
    }
}
