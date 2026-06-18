use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const HTTP_SCHEME_PREFIX: &str = concat!("http", "://");
const HTTPS_SCHEME_PREFIX: &str = "https://";

pub const VERIFIED_DAILY_PRODUCTS: &[&str] = &["VNP46A2", "VJ146A2"];
pub const VERIFIED_MONTHLY_PRODUCTS: &[&str] = &["VNP46A3"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProductCadence {
    Daily,
    Monthly,
}

impl ProductCadence {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Monthly => "monthly",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "daily" => Some(Self::Daily),
            "monthly" => Some(Self::Monthly),
            _ => None,
        }
    }

    pub const fn default_products(self) -> &'static [&'static str] {
        match self {
            Self::Daily => VERIFIED_DAILY_PRODUCTS,
            Self::Monthly => VERIFIED_MONTHLY_PRODUCTS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingProduct {
    Vnp46A2,
    Vj146A2,
    Vnp46A3,
}

impl ProcessingProduct {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vnp46A2 => "VNP46A2",
            Self::Vj146A2 => "VJ146A2",
            Self::Vnp46A3 => "VNP46A3",
        }
    }

    pub fn parse(product: &str) -> Result<Self, ProcessingMessageValidationError> {
        match product {
            "VNP46A2" => Ok(Self::Vnp46A2),
            "VJ146A2" => Ok(Self::Vj146A2),
            "VNP46A3" => Ok(Self::Vnp46A3),
            _ => Err(ProcessingMessageValidationError::UnsupportedProduct(
                product.to_owned(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductDefinition {
    pub product: ProcessingProduct,
    pub short_name: &'static str,
    pub cadence: ProductCadence,
    pub science_mapping_verified: bool,
}

pub const PRODUCT_DEFINITIONS: &[ProductDefinition] = &[
    ProductDefinition {
        product: ProcessingProduct::Vnp46A2,
        short_name: "VNP46A2",
        cadence: ProductCadence::Daily,
        science_mapping_verified: true,
    },
    ProductDefinition {
        product: ProcessingProduct::Vj146A2,
        short_name: "VJ146A2",
        cadence: ProductCadence::Daily,
        science_mapping_verified: true,
    },
    ProductDefinition {
        product: ProcessingProduct::Vnp46A3,
        short_name: "VNP46A3",
        cadence: ProductCadence::Monthly,
        science_mapping_verified: true,
    },
];

pub fn product_definition(short_name: &str) -> Option<&'static ProductDefinition> {
    PRODUCT_DEFINITIONS
        .iter()
        .find(|definition| definition.short_name == short_name)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessingMessage {
    pub ingest_id: Uuid,
    pub blob_path: String,
    pub product: String,
    pub granule_date: DateTime<Utc>,
    pub tile_h: i16,
    pub tile_v: i16,
}

#[derive(Debug, Deserialize)]
struct RawProcessingMessage {
    ingest_id: Uuid,
    blob_path: String,
    product: String,
    granule_date: DateTime<Utc>,
    tile_h: i16,
    tile_v: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProcessingMessageValidationError {
    #[error(
        "invalid processing message blob_path '{0}': must be relative to raw-viirs and must not be absolute, URL-based, or prefixed with raw-viirs/"
    )]
    InvalidBlobPath(String),
    #[error("invalid processing message JSON: {0}")]
    InvalidJson(String),
    #[error("processing message blob_path '{0}' does not contain an hXXvYY tile")]
    MissingTileInBlobPath(String),
    #[error(
        "processing message tile mismatch: blob path has h{blob_tile_h:02}v{blob_tile_v:02}, message has h{message_tile_h:02}v{message_tile_v:02}"
    )]
    TileMismatch {
        blob_tile_h: i16,
        blob_tile_v: i16,
        message_tile_h: i16,
        message_tile_v: i16,
    },
    #[error("unsupported processing message product '{0}'")]
    UnsupportedProduct(String),
}

impl ProcessingMessage {
    pub fn new(
        ingest_id: Uuid,
        blob_path: impl Into<String>,
        product: impl Into<String>,
        granule_date: DateTime<Utc>,
        tile_h: i16,
        tile_v: i16,
    ) -> Result<Self, ProcessingMessageValidationError> {
        let blob_path = blob_path.into();
        let product = product.into();

        validate_blob_path(&blob_path)?;
        validate_product(&product)?;

        let (blob_tile_h, blob_tile_v) = parse_first_tile(&blob_path).ok_or_else(|| {
            ProcessingMessageValidationError::MissingTileInBlobPath(blob_path.clone())
        })?;

        if blob_tile_h != tile_h || blob_tile_v != tile_v {
            return Err(ProcessingMessageValidationError::TileMismatch {
                blob_tile_h,
                blob_tile_v,
                message_tile_h: tile_h,
                message_tile_v: tile_v,
            });
        }

        Ok(Self {
            ingest_id,
            blob_path,
            product,
            granule_date,
            tile_h,
            tile_v,
        })
    }

    pub fn parse_json(json: &str) -> Result<Self, ProcessingMessageValidationError> {
        let raw: RawProcessingMessage = serde_json::from_str(json)
            .map_err(|error| ProcessingMessageValidationError::InvalidJson(error.to_string()))?;

        Self::new(
            raw.ingest_id,
            raw.blob_path,
            raw.product,
            raw.granule_date,
            raw.tile_h,
            raw.tile_v,
        )
    }

    pub fn product_kind(&self) -> Result<ProcessingProduct, ProcessingMessageValidationError> {
        ProcessingProduct::parse(&self.product)
    }
}

fn validate_blob_path(path: &str) -> Result<(), ProcessingMessageValidationError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with(HTTP_SCHEME_PREFIX)
        || path.starts_with(HTTPS_SCHEME_PREFIX)
        || path.starts_with("raw-viirs/")
    {
        return Err(ProcessingMessageValidationError::InvalidBlobPath(
            path.to_owned(),
        ));
    }

    Ok(())
}

fn validate_product(product: &str) -> Result<(), ProcessingMessageValidationError> {
    ProcessingProduct::parse(product).map(|_| ())
}

fn parse_first_tile(path: &str) -> Option<(i16, i16)> {
    for window in path.as_bytes().windows(6) {
        if window[0] == b'h'
            && window[3] == b'v'
            && window[1].is_ascii_digit()
            && window[2].is_ascii_digit()
            && window[4].is_ascii_digit()
            && window[5].is_ascii_digit()
        {
            let tile_h = parse_two_digits(window[1], window[2]);
            let tile_v = parse_two_digits(window[4], window[5]);

            return Some((tile_h, tile_v));
        }
    }

    None
}

fn parse_two_digits(tens: u8, ones: u8) -> i16 {
    i16::from(tens - b'0') * 10 + i16::from(ones - b'0')
}

#[cfg(test)]
mod tests {
    use super::{ProcessingMessage, ProcessingMessageValidationError, ProcessingProduct};

    const VALID_MESSAGE: &str = r#"{
        "ingest_id": "018f7fd2-01d0-7cc7-9b37-00f86c3fd98b",
        "blob_path": "VNP46A2/2026-05-21/h11v06.h5",
        "product": "VNP46A2",
        "granule_date": "2026-05-21T00:00:00Z",
        "tile_h": 11,
        "tile_v": 6
    }"#;

    #[test]
    fn parses_valid_message() {
        let message = ProcessingMessage::parse_json(VALID_MESSAGE).unwrap();

        assert_eq!(message.product, "VNP46A2");
        assert_eq!(message.blob_path, "VNP46A2/2026-05-21/h11v06.h5");
        assert_eq!(message.tile_h, 11);
        assert_eq!(message.tile_v, 6);
    }

    #[test]
    fn parses_supported_products() {
        for product in ["VNP46A2", "VJ146A2", "VNP46A3"] {
            let json = VALID_MESSAGE.replace("VNP46A2", product);

            let message = ProcessingMessage::parse_json(&json).unwrap();

            assert_eq!(message.product, product);
            assert_eq!(message.product_kind().unwrap().as_str(), product);
        }
    }

    #[test]
    fn exposes_typed_product_kind() {
        let message = ProcessingMessage::parse_json(VALID_MESSAGE).unwrap();

        assert_eq!(message.product_kind().unwrap(), ProcessingProduct::Vnp46A2);
        assert_eq!(message.product_kind().unwrap().as_str(), "VNP46A2");
    }

    #[test]
    fn rejects_container_prefixed_blob_path() {
        let error = ProcessingMessage::parse_json(&VALID_MESSAGE.replace(
            "VNP46A2/2026-05-21/h11v06.h5",
            "raw-viirs/VNP46A2/2026-05-21/h11v06.h5",
        ))
        .unwrap_err();

        assert!(matches!(
            error,
            ProcessingMessageValidationError::InvalidBlobPath(_)
        ));
    }

    #[test]
    fn rejects_absolute_blob_path() {
        let error = ProcessingMessage::parse_json(&VALID_MESSAGE.replace(
            "VNP46A2/2026-05-21/h11v06.h5",
            "/VNP46A2/2026-05-21/h11v06.h5",
        ))
        .unwrap_err();

        assert!(matches!(
            error,
            ProcessingMessageValidationError::InvalidBlobPath(_)
        ));
    }

    #[test]
    fn rejects_unsupported_product() {
        let error = ProcessingMessage::parse_json(&VALID_MESSAGE.replace("VNP46A2", "UNKNOWN"))
            .unwrap_err();

        assert!(matches!(
            error,
            ProcessingMessageValidationError::UnsupportedProduct(product) if product == "UNKNOWN"
        ));
    }

    #[test]
    fn rejects_tile_mismatch() {
        let error = ProcessingMessage::parse_json(
            &VALID_MESSAGE.replace("\"tile_h\": 11", "\"tile_h\": 12"),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ProcessingMessageValidationError::TileMismatch {
                blob_tile_h: 11,
                blob_tile_v: 6,
                message_tile_h: 12,
                message_tile_v: 6,
            }
        ));
    }

    #[test]
    fn rejects_missing_tile_in_blob_path() {
        let error = ProcessingMessage::parse_json(&VALID_MESSAGE.replace(
            "VNP46A2/2026-05-21/h11v06.h5",
            "VNP46A2/2026-05-21/no-tile.h5",
        ))
        .unwrap_err();

        assert!(matches!(
            error,
            ProcessingMessageValidationError::MissingTileInBlobPath(_)
        ));
    }
}
