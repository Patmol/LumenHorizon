use std::sync::Mutex;
use uuid::Uuid;

use crate::{
    config::{AppConfig, TileBounds},
    science::QualitySummary,
    storage::ReceivedQueueMessage,
    ServiceError,
};

use super::{
    metadata::cloud_rejection_reason,
    queue_worker::{
        is_missing_ingest_log_error, process_once_with_queue, should_deadletter,
        ProcessOnceOutcome, ProcessingQueue,
    },
};

struct FakeQueue {
    messages: Mutex<Vec<ReceivedQueueMessage>>,
    enqueued: Mutex<Vec<(String, String)>>,
    deleted: Mutex<Vec<(String, String, String)>>,
}

impl FakeQueue {
    fn with_message(message: ReceivedQueueMessage) -> Self {
        Self {
            messages: Mutex::new(vec![message]),
            enqueued: Mutex::new(Vec::new()),
            deleted: Mutex::new(Vec::new()),
        }
    }

    fn enqueued_messages(&self) -> Vec<(String, String)> {
        self.enqueued.lock().unwrap().clone()
    }

    fn deleted_messages(&self) -> Vec<(String, String, String)> {
        self.deleted.lock().unwrap().clone()
    }
}

impl ProcessingQueue for FakeQueue {
    async fn receive_messages(
        &self,
        queue_name: &str,
        max_messages: usize,
        visibility_timeout_seconds: u64,
    ) -> Result<Vec<ReceivedQueueMessage>, ServiceError> {
        assert_eq!(queue_name, "viirs-processing");
        assert_eq!(max_messages, 1);
        assert_eq!(visibility_timeout_seconds, 900);

        Ok(std::mem::take(&mut *self.messages.lock().unwrap()))
    }

    async fn delete_message(
        &self,
        queue_name: &str,
        message_id: &str,
        pop_receipt: &str,
    ) -> Result<(), ServiceError> {
        self.deleted.lock().unwrap().push((
            queue_name.to_owned(),
            message_id.to_owned(),
            pop_receipt.to_owned(),
        ));

        Ok(())
    }

    async fn enqueue_message(
        &self,
        queue_name: &str,
        message_text: &str,
    ) -> Result<(), ServiceError> {
        self.enqueued
            .lock()
            .unwrap()
            .push((queue_name.to_owned(), message_text.to_owned()));

        Ok(())
    }
}

fn test_config() -> AppConfig {
    AppConfig {
        rust_log: "processing_svc=debug".to_owned(),
        database_url: "postgres://localhost/lumenhorizon".to_owned(),
        azure_storage_account: "devstoreaccount1".to_owned(),
        azure_storage_access_key: "dGVzdC1zdG9yYWdlLWFjY291bnQta2V5".to_owned(),
        azure_storage_emulator_host: Some("127.0.0.1".to_owned()),
        azure_queue_name: "viirs-processing".to_owned(),
        azure_deadletter_queue_name: "viirs-processing-deadletter".to_owned(),
        raw_viirs_container: "raw-viirs".to_owned(),
        processed_tiles_container: "processed-tiles".to_owned(),
        max_cloud_fraction: 0.4,
        processing_visibility_timeout_seconds: 900,
        processing_max_dequeue_count: 5,
        processing_max_parallelism: 1,
        http_request_timeout: std::time::Duration::from_secs(30),
        http_retry: shared::http_retry::RetryConfig {
            max_attempts: 3,
            base_delay: std::time::Duration::from_millis(250),
            max_delay: std::time::Duration::from_millis(5_000),
        },
        tile_min_zoom: 3,
        tile_max_native_zoom: 10,
        tile_max_display_zoom: 12,
        tile_size: 256,
        tile_format: "png".to_owned(),
        tile_classification_version: "radiance-dark-sky-v1".to_owned(),
        tile_render_version: "tiles-v1".to_owned(),
        tile_cdn_base_url: "https://tiles.lumenhorizon.com".to_owned(),
        tile_bounds: TileBounds {
            west: -125.0,
            south: 24.0,
            east: -66.0,
            north: 50.0,
        },
        tile_immutable_cache_control: "public, max-age=31536000, immutable".to_owned(),
        tile_latest_cache_control: "public, max-age=300, must-revalidate".to_owned(),
        raw_granule_retention_days: 90,
        processed_tile_set_retention_days: 180,
        retention_protected_prior_tile_sets: 2,
        retention_batch_limit: 500,
        retention_tile_blob_limit: 5_000,
    }
}

fn malformed_message(dequeue_count: u32) -> ReceivedQueueMessage {
    ReceivedQueueMessage {
        message_id: "message-1".to_owned(),
        pop_receipt: "receipt-1".to_owned(),
        dequeue_count,
        message_text: "not-json".to_owned(),
    }
}

fn test_correlation_id() -> Uuid {
    Uuid::parse_str("00000000-0000-0000-0000-00000000c0de").unwrap()
}

#[test]
fn deadletters_at_max_dequeue_count() {
    assert!(should_deadletter(5, 5));
}

#[test]
fn deadletters_after_max_dequeue_count() {
    assert!(should_deadletter(6, 5));
}

#[test]
fn retries_before_max_dequeue_count() {
    assert!(!should_deadletter(4, 5));
}

#[test]
fn missing_ingest_log_error_is_terminal_for_queue_messages() {
    let ingest_id = Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap();
    let error = ServiceError::Database(crate::db::DbError::MissingIngestLog { ingest_id });

    assert!(is_missing_ingest_log_error(&error));
}

#[tokio::test]
async fn malformed_message_before_max_dequeue_is_left_for_visibility_retry() {
    let queue = FakeQueue::with_message(malformed_message(4));
    let error = process_once_with_queue(&test_config(), &queue, test_correlation_id())
        .await
        .unwrap_err();

    assert!(matches!(error, ServiceError::ProcessingMessage(_)));
    assert!(queue.enqueued_messages().is_empty());
    assert!(queue.deleted_messages().is_empty());
}

#[tokio::test]
async fn malformed_message_at_max_dequeue_moves_to_deadletter() {
    let queue = FakeQueue::with_message(malformed_message(5));
    let outcome = process_once_with_queue(&test_config(), &queue, test_correlation_id())
        .await
        .unwrap();

    assert_eq!(outcome, ProcessOnceOutcome::HandledMessage);
    assert_eq!(
        queue.enqueued_messages(),
        vec![(
            "viirs-processing-deadletter".to_owned(),
            "not-json".to_owned()
        )]
    );
    assert_eq!(
        queue.deleted_messages(),
        vec![(
            "viirs-processing".to_owned(),
            "message-1".to_owned(),
            "receipt-1".to_owned()
        )]
    );
}

#[test]
fn cloud_rejection_reason_returns_reason_above_threshold() {
    let summary = QualitySummary {
        total_pixel_count: 4,
        valid_pixel_count: 4,
        rejected_pixel_count: 0,
        cloud_contaminated_valid_pixel_count: 3,
        cloud_fraction: 0.75,
    };

    assert_eq!(
        cloud_rejection_reason(&summary, 0.5),
        Some("cloud fraction exceeds configured maximum")
    );
}

#[test]
fn cloud_rejection_reason_allows_fraction_at_threshold() {
    let summary = QualitySummary {
        total_pixel_count: 4,
        valid_pixel_count: 4,
        rejected_pixel_count: 0,
        cloud_contaminated_valid_pixel_count: 2,
        cloud_fraction: 0.5,
    };

    assert_eq!(cloud_rejection_reason(&summary, 0.5), None);
}
