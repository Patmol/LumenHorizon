use shared::azure_storage;

use crate::models::BlobPathValidationError;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    AzureStorage(#[from] azure_storage::AzureStorageError),
    #[error("storage error: raw blob container readiness check returned {status}: {body}")]
    BlobReadinessStatus {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("storage error: failed to build HTTP client: {0}")]
    BuildClient(reqwest::Error),
    #[error("storage error: endpoint cannot be used as base URL")]
    CannotBeBaseUrl,
    #[error("storage error: invalid blob path '{blob_path}': {reason}")]
    InvalidBlobPath {
        blob_path: String,
        reason: &'static str,
    },
    #[error("storage error: blob download returned {status} for '{blob_path}': {body}")]
    DownloadStatus {
        blob_path: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("storage error: queue readiness check returned {status} for '{queue_name}': {body}")]
    QueueReadinessStatus {
        queue_name: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("storage error: queue enqueue returned {status} for '{queue_name}': {body}")]
    QueueStatus {
        queue_name: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("storage error: request failed: {0}")]
    Request(reqwest::Error),
    #[error("storage error: failed to read blob body: {0}")]
    ReadBody(reqwest::Error),
    #[error("storage error: failed to serialize processing message: {0}")]
    SerializeMessage(serde_json::Error),
    #[error("storage error: blob upload returned {status} for '{blob_path}': {body}")]
    UploadStatus {
        blob_path: String,
        status: reqwest::StatusCode,
        body: String,
    },
}

impl From<BlobPathValidationError> for StorageError {
    fn from(error: BlobPathValidationError) -> Self {
        Self::InvalidBlobPath {
            blob_path: error.blob_path,
            reason: error.reason,
        }
    }
}
