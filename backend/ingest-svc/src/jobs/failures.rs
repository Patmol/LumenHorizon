use crate::{earthdata::EarthdataError, models::ProcessingMessageError, storage::StorageError};

use super::validation::RawGranuleValidationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IngestFailureContext {
    pub(super) phase: FailurePhase,
    pub(super) code: &'static str,
    pub(super) category: &'static str,
    pub(super) retry_eligible: bool,
    pub(super) http_status: Option<u16>,
    pub(super) message: String,
}

impl IngestFailureContext {
    fn new(
        phase: FailurePhase,
        code: &'static str,
        category: &'static str,
        retry_eligible: bool,
        http_status: Option<u16>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            phase,
            code,
            category,
            retry_eligible,
            http_status,
            message: message.into(),
        }
    }

    pub(super) fn database_message(&self) -> String {
        match self.http_status {
            Some(http_status) => format!(
                "phase={}; code={}; category={}; retry_eligible={}; http_status={}; message={}",
                self.phase.as_str(),
                self.code,
                self.category,
                self.retry_eligible,
                http_status,
                self.message
            ),
            None => format!(
                "phase={}; code={}; category={}; retry_eligible={}; message={}",
                self.phase.as_str(),
                self.code,
                self.category,
                self.retry_eligible,
                self.message
            ),
        }
    }

    pub(super) fn earthdata_download(error: &EarthdataError) -> Self {
        match error {
            EarthdataError::MissingToken => Self::new(
                FailurePhase::Download,
                "earthdata_missing_token",
                "configuration",
                false,
                None,
                "EARTHDATA_BEARER_TOKEN is missing or still set to a placeholder",
            ),
            EarthdataError::BuildClient(error) => Self::new(
                FailurePhase::Download,
                "earthdata_client_build_failed",
                "configuration",
                false,
                None,
                format!("failed to build Earthdata HTTP client: {error}"),
            ),
            EarthdataError::Request(error) => Self::new(
                FailurePhase::Download,
                "earthdata_request_failed",
                "network",
                true,
                None,
                format!("Earthdata request failed before a response was received: {error}"),
            ),
            EarthdataError::Status {
                granule_title,
                status,
            } => {
                let (category, retry_eligible) = classify_http_status(*status);
                Self::new(
                    FailurePhase::Download,
                    "earthdata_http_status",
                    category,
                    retry_eligible,
                    Some(status.as_u16()),
                    format!("Earthdata returned HTTP {status} for granule {granule_title}"),
                )
            }
            EarthdataError::ReadBody(error) => Self::new(
                FailurePhase::Download,
                "earthdata_body_read_failed",
                "network",
                true,
                None,
                format!("failed to read Earthdata response body: {error}"),
            ),
            EarthdataError::EmptyBody { granule_title } => Self::new(
                FailurePhase::Download,
                "earthdata_empty_body",
                "upstream",
                true,
                None,
                format!("Earthdata returned an empty body for granule {granule_title}"),
            ),
        }
    }

    pub(super) fn storage_upload(error: &StorageError) -> Self {
        storage_failure(FailurePhase::Upload, error)
    }

    pub(super) fn storage_download(error: &StorageError) -> Self {
        storage_failure(FailurePhase::RecoverDownload, error)
    }

    pub(super) fn queue_enqueue(error: &StorageError) -> Self {
        storage_failure(FailurePhase::Enqueue, error)
    }

    pub(super) fn raw_validation(error: &RawGranuleValidationError) -> Self {
        match error {
            RawGranuleValidationError::TooSmall {
                actual_size,
                minimum_size,
            } => Self::new(
                FailurePhase::Validation,
                "raw_granule_too_small",
                "validation",
                false,
                None,
                format!(
                    "raw granule was {actual_size} bytes; expected at least {minimum_size} bytes"
                ),
            ),
            RawGranuleValidationError::MissingHdf5Magic => Self::new(
                FailurePhase::Validation,
                "raw_granule_missing_hdf5_magic",
                "validation",
                false,
                None,
                "raw granule is missing the HDF5 magic number",
            ),
        }
    }

    pub(super) fn processing_message(error: &ProcessingMessageError) -> Self {
        match error {
            ProcessingMessageError::InvalidJson(error) => Self::new(
                FailurePhase::BuildProcessingMessage,
                "processing_message_invalid_json",
                "serialization",
                false,
                None,
                format!("invalid processing message JSON: {error}"),
            ),
            ProcessingMessageError::InvalidBlobPath(blob_path) => Self::new(
                FailurePhase::BuildProcessingMessage,
                "processing_message_invalid_blob_path",
                "validation",
                false,
                None,
                format!("invalid blob path '{blob_path}'"),
            ),
            ProcessingMessageError::UnsupportedProduct(product) => Self::new(
                FailurePhase::BuildProcessingMessage,
                "processing_message_unsupported_product",
                "validation",
                false,
                None,
                format!("unsupported product '{product}'"),
            ),
            ProcessingMessageError::MissingTileInBlobPath(blob_path) => Self::new(
                FailurePhase::BuildProcessingMessage,
                "processing_message_blob_path_tile_missing",
                "validation",
                false,
                None,
                format!("invalid blob path '{blob_path}': missing hXXvYY tile coordinate"),
            ),
            ProcessingMessageError::TileMismatch {
                blob_tile_h,
                blob_tile_v,
                message_tile_h,
                message_tile_v,
            } => Self::new(
                FailurePhase::BuildProcessingMessage,
                "processing_message_blob_path_tile_mismatch",
                "validation",
                false,
                None,
                format!(
                    "processing message tile mismatch: blob path has h{blob_tile_h:02}v{blob_tile_v:02}, message has h{message_tile_h:02}v{message_tile_v:02}"
                ),
            ),
        }
    }

    pub(super) fn database_status_update(
        phase: FailurePhase,
        code: &'static str,
        error: &sqlx::Error,
    ) -> Self {
        Self::new(
            phase,
            code,
            "database",
            true,
            None,
            format!("failed to update ingest_log status: {error}"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FailurePhase {
    RecordDownloading,
    Download,
    Upload,
    RecoverDownload,
    MarkDownloaded,
    Validation,
    MarkValidated,
    CreateOutbox,
    BuildProcessingMessage,
    Enqueue,
    MarkEnqueued,
    CompleteOutbox,
}

impl FailurePhase {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::RecordDownloading => "record_downloading",
            Self::Download => "download",
            Self::Upload => "upload",
            Self::RecoverDownload => "recover_download",
            Self::MarkDownloaded => "mark_downloaded",
            Self::Validation => "validation",
            Self::MarkValidated => "mark_validated",
            Self::CreateOutbox => "create_outbox",
            Self::BuildProcessingMessage => "build_processing_message",
            Self::Enqueue => "enqueue",
            Self::MarkEnqueued => "mark_enqueued",
            Self::CompleteOutbox => "complete_outbox",
        }
    }
}

fn storage_failure(phase: FailurePhase, error: &StorageError) -> IngestFailureContext {
    match error {
        StorageError::BuildClient(error) => IngestFailureContext::new(
            phase,
            "storage_client_build_failed",
            "configuration",
            false,
            None,
            format!("failed to build storage HTTP client: {error}"),
        ),
        StorageError::CannotBeBaseUrl => IngestFailureContext::new(
            phase,
            "storage_endpoint_not_base_url",
            "configuration",
            false,
            None,
            "storage endpoint cannot be used as a base URL",
        ),
        StorageError::InvalidBlobPath { blob_path, reason } => IngestFailureContext::new(
            phase,
            "storage_invalid_blob_path",
            "validation",
            false,
            None,
            format!("invalid blob path '{blob_path}': {reason}"),
        ),
        StorageError::Request(error) => IngestFailureContext::new(
            phase,
            "storage_request_failed",
            "network",
            true,
            None,
            format!("storage request failed before a response was received: {error}"),
        ),
        StorageError::ReadBody(error) => IngestFailureContext::new(
            phase,
            "storage_blob_read_failed",
            "network",
            true,
            None,
            format!("failed to read storage response body: {error}"),
        ),
        StorageError::SerializeMessage(error) => IngestFailureContext::new(
            phase,
            "storage_message_serialize_failed",
            "serialization",
            false,
            None,
            format!("failed to serialize processing message: {error}"),
        ),
        StorageError::UploadStatus {
            blob_path,
            status,
            body,
        } => {
            let (category, retry_eligible) = classify_http_status(*status);
            IngestFailureContext::new(
                phase,
                "storage_upload_http_status",
                category,
                retry_eligible,
                Some(status.as_u16()),
                format!("blob upload returned HTTP {status} for '{blob_path}': {body}"),
            )
        }
        StorageError::DownloadStatus {
            blob_path,
            status,
            body,
        } => {
            let (category, retry_eligible) = classify_http_status(*status);
            IngestFailureContext::new(
                phase,
                "storage_download_http_status",
                category,
                retry_eligible,
                Some(status.as_u16()),
                format!("blob download returned HTTP {status} for '{blob_path}': {body}"),
            )
        }
        StorageError::BlobReadinessStatus { status, body } => {
            let (category, retry_eligible) = classify_http_status(*status);
            IngestFailureContext::new(
                phase,
                "storage_blob_readiness_http_status",
                category,
                retry_eligible,
                Some(status.as_u16()),
                format!("raw blob container readiness check returned HTTP {status}: {body}"),
            )
        }
        StorageError::QueueStatus {
            queue_name,
            status,
            body,
        } => {
            let (category, retry_eligible) = classify_http_status(*status);
            IngestFailureContext::new(
                phase,
                "storage_queue_http_status",
                category,
                retry_eligible,
                Some(status.as_u16()),
                format!("queue enqueue returned HTTP {status} for '{queue_name}': {body}"),
            )
        }
        StorageError::QueueReadinessStatus {
            queue_name,
            status,
            body,
        } => {
            let (category, retry_eligible) = classify_http_status(*status);
            IngestFailureContext::new(
                phase,
                "storage_queue_readiness_http_status",
                category,
                retry_eligible,
                Some(status.as_u16()),
                format!("queue readiness check returned HTTP {status} for '{queue_name}': {body}"),
            )
        }
        StorageError::AzureStorage(error) => IngestFailureContext::new(
            phase,
            "storage_shared_helper_failed",
            "configuration",
            false,
            None,
            format!("shared Azure Storage helper failed: {error}"),
        ),
    }
}

fn classify_http_status(status: reqwest::StatusCode) -> (&'static str, bool) {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        ("auth", false)
    } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        ("rate_limit", true)
    } else if status.is_server_error() {
        ("upstream", true)
    } else {
        ("upstream", false)
    }
}

#[cfg(test)]
mod tests {
    use crate::{earthdata::EarthdataError, models::ProcessingMessageError, storage::StorageError};

    use super::super::validation::RawGranuleValidationError;
    use super::{FailurePhase, IngestFailureContext};

    #[test]
    fn classifies_missing_earthdata_token_as_configuration_error() {
        let failure = IngestFailureContext::earthdata_download(&EarthdataError::MissingToken);

        assert_eq!(failure.phase, FailurePhase::Download);
        assert_eq!(failure.code, "earthdata_missing_token");
        assert_eq!(failure.category, "configuration");
        assert!(!failure.retry_eligible);
        assert_eq!(failure.http_status, None);
        assert!(failure.database_message().contains("phase=download"));
        assert!(failure
            .database_message()
            .contains("code=earthdata_missing_token"));
    }

    #[test]
    fn classifies_earthdata_http_status_with_numeric_status() {
        let failure = IngestFailureContext::earthdata_download(&EarthdataError::Status {
            granule_title: "VNP46A2.A2024142.h11v06.002.h5".to_owned(),
            status: reqwest::StatusCode::UNAUTHORIZED,
        });

        assert_eq!(failure.phase, FailurePhase::Download);
        assert_eq!(failure.code, "earthdata_http_status");
        assert_eq!(failure.category, "auth");
        assert!(!failure.retry_eligible);
        assert_eq!(failure.http_status, Some(401));
        assert!(failure.database_message().contains("http_status=401"));
    }

    #[test]
    fn classifies_storage_upload_status_with_retry_signal() {
        let failure = IngestFailureContext::storage_upload(&StorageError::UploadStatus {
            blob_path: "VNP46A2/2024-05-21/h11v06.h5".to_owned(),
            status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
            body: "temporary outage".to_owned(),
        });

        assert_eq!(failure.phase, FailurePhase::Upload);
        assert_eq!(failure.code, "storage_upload_http_status");
        assert_eq!(failure.category, "upstream");
        assert!(failure.retry_eligible);
        assert_eq!(failure.http_status, Some(503));
    }

    #[test]
    fn classifies_queue_status_as_enqueue_phase() {
        let failure = IngestFailureContext::queue_enqueue(&StorageError::QueueStatus {
            queue_name: "viirs-processing".to_owned(),
            status: reqwest::StatusCode::TOO_MANY_REQUESTS,
            body: "slow down".to_owned(),
        });

        assert_eq!(failure.phase, FailurePhase::Enqueue);
        assert_eq!(failure.code, "storage_queue_http_status");
        assert_eq!(failure.category, "rate_limit");
        assert!(failure.retry_eligible);
        assert_eq!(failure.http_status, Some(429));
    }

    #[test]
    fn classifies_validation_rejection_as_non_retryable() {
        let failure =
            IngestFailureContext::raw_validation(&RawGranuleValidationError::MissingHdf5Magic);

        assert_eq!(failure.phase, FailurePhase::Validation);
        assert_eq!(failure.code, "raw_granule_missing_hdf5_magic");
        assert_eq!(failure.category, "validation");
        assert!(!failure.retry_eligible);
        assert_eq!(failure.http_status, None);
    }

    #[test]
    fn classifies_processing_message_legacy_blob_path_rejection() {
        let failure =
            IngestFailureContext::processing_message(&ProcessingMessageError::InvalidBlobPath(
                "raw-viirs/VNP46A2/2024-05-21/h11v06.h5".to_owned(),
            ));

        assert_eq!(failure.phase, FailurePhase::BuildProcessingMessage);
        assert_eq!(failure.code, "processing_message_invalid_blob_path");
        assert_eq!(failure.category, "validation");
        assert!(!failure.retry_eligible);
        assert!(failure
            .database_message()
            .contains("raw-viirs/VNP46A2/2024-05-21/h11v06.h5"));
    }
}
