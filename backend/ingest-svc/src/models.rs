use chrono::{DateTime, Utc};
use serde::Serialize;
pub use shared::processing_message::{
    ProcessingMessage, ProcessingMessageValidationError as ProcessingMessageError, ProductCadence,
};
use uuid::Uuid;

pub const RAW_VIIRS_CONTAINER: &str = "raw-viirs";

pub const INGEST_STATUS_DOWNLOADING: &str = "downloading";
pub const INGEST_STATUS_DOWNLOADED: &str = "downloaded";
pub const INGEST_STATUS_VALIDATED: &str = "validated";
pub const INGEST_STATUS_ENQUEUED: &str = "enqueued";
pub const INGEST_STATUS_REJECTED: &str = "rejected";
pub const INGEST_STATUS_FAILED: &str = "failed";
pub const INGEST_STATUS_RECOVERY_PENDING: &str = "recovery_pending";
pub const INGEST_STATUS_REPLAY_PENDING: &str = "replay_pending";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GranuleCandidate {
    pub product: String,
    pub title: String,
    pub producer_granule_id: String,
    pub data_href: String,
    pub granule_date: DateTime<Utc>,
    pub tile: TileCoordinate,
}

impl GranuleCandidate {
    pub fn raw_blob_path(&self) -> String {
        format!(
            "{}/{}/h{:02}v{:02}.h5",
            self.product,
            self.granule_date.format("%Y-%m-%d"),
            self.tile.h,
            self.tile.v
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TileCoordinate {
    pub h: u8,
    pub v: u8,
}

impl TileCoordinate {
    pub fn parse_from(value: &str) -> Option<Self> {
        let bytes = value.as_bytes();

        if bytes.len() < 6 {
            return None;
        }

        for index in 0..=bytes.len() - 6 {
            if bytes[index] == b'h'
                && bytes[index + 1].is_ascii_digit()
                && bytes[index + 2].is_ascii_digit()
                && bytes[index + 3] == b'v'
                && bytes[index + 4].is_ascii_digit()
                && bytes[index + 5].is_ascii_digit()
            {
                return Some(Self {
                    h: parse_two_digits(bytes[index + 1], bytes[index + 2]),
                    v: parse_two_digits(bytes[index + 4], bytes[index + 5]),
                });
            }
        }

        None
    }
}

pub fn processing_message_from_granule(
    ingest_id: Uuid,
    granule: &GranuleCandidate,
    blob_path: &str,
) -> Result<ProcessingMessage, ProcessingMessageError> {
    ProcessingMessage::new(
        ingest_id,
        blob_path,
        granule.product.clone(),
        granule.granule_date,
        i16::from(granule.tile.h),
        i16::from(granule.tile.v),
    )
}

fn parse_two_digits(tens: u8, ones: u8) -> u8 {
    (tens - b'0') * 10 + (ones - b'0')
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid blob path '{blob_path}': {reason}")]
pub struct BlobPathValidationError {
    pub blob_path: String,
    pub reason: &'static str,
}

pub(crate) fn validate_raw_blob_path(blob_path: &str) -> Result<(), BlobPathValidationError> {
    if blob_path.is_empty() {
        return Err(BlobPathValidationError {
            blob_path: blob_path.to_owned(),
            reason: "path must not be empty",
        });
    }

    if blob_path.starts_with('/') {
        return Err(BlobPathValidationError {
            blob_path: blob_path.to_owned(),
            reason: "path must be relative",
        });
    }

    if blob_path.starts_with("https://") || blob_path.starts_with("http://") {
        return Err(BlobPathValidationError {
            blob_path: blob_path.to_owned(),
            reason: "path must not be a URL",
        });
    }

    if blob_path.starts_with(&format!("{RAW_VIIRS_CONTAINER}/")) {
        return Err(BlobPathValidationError {
            blob_path: blob_path.to_owned(),
            reason: "path must not include the raw blob container prefix",
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use shared::processing_message::{
        product_definition, VERIFIED_DAILY_PRODUCTS, VERIFIED_MONTHLY_PRODUCTS,
    };
    use uuid::Uuid;

    use super::{
        processing_message_from_granule, GranuleCandidate, ProcessingMessageError, ProductCadence,
        TileCoordinate, INGEST_STATUS_DOWNLOADED, INGEST_STATUS_DOWNLOADING,
        INGEST_STATUS_ENQUEUED, INGEST_STATUS_FAILED, INGEST_STATUS_RECOVERY_PENDING,
        INGEST_STATUS_REJECTED, INGEST_STATUS_REPLAY_PENDING, INGEST_STATUS_VALIDATED,
        RAW_VIIRS_CONTAINER,
    };

    fn candidate_with_tile(h: u8, v: u8) -> GranuleCandidate {
        GranuleCandidate {
            product: "VNP46A2".to_owned(),
            title: "VNP46A2.A2024142.h11v06.002.2024143000000.h5".to_owned(),
            producer_granule_id: "VNP46A2.A2024142.h11v06.002.2024143000000.h5".to_owned(),
            data_href: "https://archive.example.test/file.h5".to_owned(),
            granule_date: Utc.with_ymd_and_hms(2024, 5, 21, 0, 0, 0).unwrap(),
            tile: TileCoordinate { h, v },
        }
    }

    #[test]
    fn verified_products_match_black_marble_scope() {
        assert_eq!(VERIFIED_DAILY_PRODUCTS, ["VNP46A2", "VJ146A2"]);
        assert_eq!(VERIFIED_MONTHLY_PRODUCTS, ["VNP46A3"]);
        assert_eq!(
            ProductCadence::Daily.default_products(),
            VERIFIED_DAILY_PRODUCTS
        );
        assert_eq!(
            ProductCadence::Monthly.default_products(),
            VERIFIED_MONTHLY_PRODUCTS
        );
    }

    #[test]
    fn product_definitions_record_cadence_and_mapping_guardrails() {
        let daily = product_definition("VJ146A2").unwrap();
        let monthly = product_definition("VNP46A3").unwrap();

        assert_eq!(daily.cadence, ProductCadence::Daily);
        assert!(daily.science_mapping_verified);
        assert_eq!(monthly.cadence, ProductCadence::Monthly);
        assert!(monthly.science_mapping_verified);
        assert!(product_definition("VNP46A1").is_none());
    }

    #[test]
    fn parses_first_tile_coordinate() {
        assert_eq!(
            TileCoordinate::parse_from("VNP46A2.A2024142.h11v06.002.2024143000000.h5"),
            Some(TileCoordinate { h: 11, v: 6 })
        );
    }

    #[test]
    fn parses_tile_coordinate_from_url() {
        assert_eq!(
            TileCoordinate::parse_from(
                "https://archive.example.test/VJ146A2.A2024142.h08v05.002.h5"
            ),
            Some(TileCoordinate { h: 8, v: 5 })
        );
    }

    #[test]
    fn rejects_values_without_tile_coordinate() {
        assert_eq!(TileCoordinate::parse_from("VNP46A2.A2024142.002.h5"), None);
    }

    #[test]
    fn formats_raw_blob_path_without_container_prefix() {
        let candidate = candidate_with_tile(11, 6);

        assert_eq!(candidate.raw_blob_path(), "VNP46A2/2024-05-21/h11v06.h5")
    }

    #[test]
    fn raw_blob_path_is_relative_to_container() {
        let path = candidate_with_tile(11, 6).raw_blob_path();

        assert!(!path.starts_with('/'));
        assert!(!path.starts_with("http://"));
        assert!(!path.starts_with("https://"));
        assert!(!path.starts_with("raw-viirs/"));
    }

    #[test]
    fn raw_blob_path_preserves_two_digit_tiles() {
        let path = candidate_with_tile(8, 5).raw_blob_path();

        assert_eq!(path, "VNP46A2/2024-05-21/h08v05.h5")
    }

    #[test]
    fn ingest_status_constants_match_database_values() {
        assert_eq!(INGEST_STATUS_DOWNLOADED, "downloaded");
        assert_eq!(INGEST_STATUS_DOWNLOADING, "downloading");
        assert_eq!(INGEST_STATUS_VALIDATED, "validated");
        assert_eq!(INGEST_STATUS_ENQUEUED, "enqueued");
        assert_eq!(INGEST_STATUS_REJECTED, "rejected");
        assert_eq!(INGEST_STATUS_FAILED, "failed");
        assert_eq!(INGEST_STATUS_RECOVERY_PENDING, "recovery_pending");
        assert_eq!(INGEST_STATUS_REPLAY_PENDING, "replay_pending");
        assert_eq!(RAW_VIIRS_CONTAINER, "raw-viirs");
    }

    #[test]
    fn builds_processing_message_from_granule() {
        let ingest_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let candidate = candidate_with_tile(11, 6);
        let blob_path = candidate.raw_blob_path();

        let message = processing_message_from_granule(ingest_id, &candidate, &blob_path).unwrap();

        assert_eq!(message.ingest_id, ingest_id);
        assert_eq!(message.blob_path, "VNP46A2/2024-05-21/h11v06.h5");
        assert_eq!(message.product, "VNP46A2");
        assert_eq!(message.granule_date, candidate.granule_date);
        assert_eq!(message.tile_h, 11);
        assert_eq!(message.tile_v, 6);
    }

    #[test]
    fn serializes_processing_message_contract() {
        let ingest_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let candidate = candidate_with_tile(11, 6);
        let blob_path = candidate.raw_blob_path();
        let message = processing_message_from_granule(ingest_id, &candidate, &blob_path).unwrap();

        let value = serde_json::to_value(message).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "ingest_id": "11111111-1111-4111-8111-111111111111",
                "blob_path": "VNP46A2/2024-05-21/h11v06.h5",
                "product": "VNP46A2",
                "granule_date": "2024-05-21T00:00:00Z",
                "tile_h": 11,
                "tile_v": 6
            })
        );
    }

    #[test]
    fn rejects_processing_message_with_container_prefixed_blob_path() {
        let candidate = candidate_with_tile(11, 6);

        let error = processing_message_from_granule(
            Uuid::new_v4(),
            &candidate,
            "raw-viirs/VNP46A2/2024-05-21/h11v06.h5",
        )
        .unwrap_err();

        assert!(matches!(error, ProcessingMessageError::InvalidBlobPath(_)));
    }

    #[test]
    fn rejects_processing_message_with_absolute_blob_path() {
        let candidate = candidate_with_tile(11, 6);

        let error = processing_message_from_granule(
            Uuid::new_v4(),
            &candidate,
            "/VNP46A2/2024-05-21/h11v06.h5",
        )
        .unwrap_err();

        assert!(matches!(error, ProcessingMessageError::InvalidBlobPath(_)));
    }

    #[test]
    fn rejects_processing_message_with_url_blob_path() {
        let candidate = candidate_with_tile(11, 6);

        let error = processing_message_from_granule(
            Uuid::new_v4(),
            &candidate,
            "https://example.test/VNP46A2/2024-05-21/h11v06.h5",
        )
        .unwrap_err();

        assert!(matches!(error, ProcessingMessageError::InvalidBlobPath(_)));
    }

    #[test]
    fn rejects_processing_message_when_blob_path_tile_is_missing() {
        let candidate = candidate_with_tile(11, 6);

        let error = processing_message_from_granule(
            Uuid::new_v4(),
            &candidate,
            "VNP46A2/2024-05-21/no-tile.h5",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ProcessingMessageError::MissingTileInBlobPath(_)
        ));
    }

    #[test]
    fn rejects_processing_message_when_blob_path_tile_does_not_match_granule() {
        let candidate = candidate_with_tile(11, 6);

        let error = processing_message_from_granule(
            Uuid::new_v4(),
            &candidate,
            "VNP46A2/2024-05-21/h08v05.h5",
        )
        .unwrap_err();

        assert!(matches!(error, ProcessingMessageError::TileMismatch { .. }));
    }

    #[test]
    fn rejects_processing_message_with_unsupported_product() {
        let mut candidate = candidate_with_tile(11, 6);
        candidate.product = "UNKNOWN".to_owned();

        let error = processing_message_from_granule(
            Uuid::new_v4(),
            &candidate,
            "UNKNOWN/2024-05-21/h11v06.h5",
        )
        .unwrap_err();

        assert_eq!(
            error,
            ProcessingMessageError::UnsupportedProduct("UNKNOWN".to_owned())
        );
    }
}
