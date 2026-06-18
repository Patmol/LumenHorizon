mod blob;
mod queue;

use std::time::Duration;

pub(crate) use blob::{BlobStorageClient, DeleteBlobOutcome};
pub use queue::QueueClient;
pub(crate) use queue::ReceivedQueueMessage;

use shared::azure_storage;

const USER_AGENT: &str = "LumenHorizon processing-svc/0.1";

pub(crate) fn build_http_client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .build()
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    AzureStorage(#[from] azure_storage::AzureStorageError),
    #[error("storage error: blob download returned {status} for '{blob_path}': {body}")]
    BlobStatus {
        blob_path: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("storage error: blob delete returned {status} for '{blob_path}': {body}")]
    BlobDeleteStatus {
        blob_path: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("storage error: blob list returned {status} for prefix '{prefix}': {body}")]
    BlobListStatus {
        prefix: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("storage error: failed to build HTTP client: {0}")]
    BuildClient(reqwest::Error),
    #[error("storage error: endpoint cannot be used as base URL")]
    CannotBeBaseUrl,
    #[error("storage error: failed to parse queue response: {0}")]
    ParseQueueResponse(quick_xml::DeError),
    #[error("storage error: failed to parse blob list response: {0}")]
    ParseBlobListResponse(quick_xml::DeError),
    #[error("storage error: processed blob upload returned {status} for '{blob_path}': {body}")]
    ProcessedBlobUploadStatus {
        blob_path: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("storage error: queue operation returned {status} for '{queue_name}': {body}")]
    QueueStatus {
        queue_name: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("storage error: failed to read blob response body: {0}")]
    ReadBlobBody(reqwest::Error),
    #[error("storage error: failed to read queue response body: {0}")]
    ReadBody(reqwest::Error),
    #[error("storage error: request failed: {0}")]
    Request(reqwest::Error),
    #[error("storage error: failed to write downloaded blob '{blob_path}' to '{path}': {source}")]
    WriteBlobFile {
        blob_path: String,
        path: std::path::PathBuf,
        source: std::io::Error,
    },
}
