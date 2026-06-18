use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    tiles::{latest_manifest_blob_path, manifest_blob_path, GeographicBounds, TileMathError},
};

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("failed to serialize tile manifest")]
    Serialize(#[from] serde_json::Error),

    #[error(transparent)]
    TileMath(#[from] TileMathError),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TileManifest {
    pub tile_set_id: String,
    pub dataset_date: NaiveDate,
    pub generated_at: DateTime<Utc>,
    pub classification_version: String,
    pub render_version: String,
    pub processor_version: String,
    pub format: String,
    pub tile_size: u16,
    pub min_zoom: u8,
    pub max_native_zoom: u8,
    pub max_display_zoom: u8,
    pub bounds: ManifestBounds,
    pub tile_url_template: String,
    pub tile_count: u32,
    pub source_granules: Vec<SourceGranule>,
    pub checksums: ManifestChecksums,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LatestManifestPointer {
    pub tile_set_id: String,
    pub manifest_blob_path: String,
    pub manifest_sha256: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct ManifestBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceGranule {
    pub ingest_id: Uuid,
    pub product: String,
    pub blob_path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManifestChecksums {
    pub manifest_sha256: String,
}

pub struct TileManifestInput {
    pub tile_set_id: String,
    pub dataset_date: NaiveDate,
    pub generated_at: DateTime<Utc>,
    pub processor_version: String,
    pub bounds: GeographicBounds,
    pub tile_count: u32,
    pub source_granules: Vec<SourceGranule>,
}

impl TileManifest {
    pub fn from_config(
        config: &AppConfig,
        input: TileManifestInput,
    ) -> Result<Self, ManifestError> {
        let tile_url_template =
            build_tile_url_template(&config.tile_cdn_base_url, &input.tile_set_id)?;

        let mut manifest = Self {
            tile_set_id: input.tile_set_id,
            dataset_date: input.dataset_date,
            generated_at: input.generated_at,
            classification_version: config.tile_classification_version.clone(),
            render_version: config.tile_render_version.clone(),
            processor_version: input.processor_version,
            format: config.tile_format.clone(),
            tile_size: config.tile_size,
            min_zoom: config.tile_min_zoom,
            max_native_zoom: config.tile_max_native_zoom,
            max_display_zoom: config.tile_max_display_zoom,
            bounds: ManifestBounds::from(input.bounds),
            tile_url_template,
            tile_count: input.tile_count,
            source_granules: input.source_granules,
            checksums: ManifestChecksums {
                manifest_sha256: String::new(),
            },
        };

        manifest.checksums.manifest_sha256 = manifest.compute_manifest_sha256()?;

        Ok(manifest)
    }

    pub fn to_pretty_json(&self) -> Result<String, ManifestError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    fn compute_manifest_sha256(&self) -> Result<String, ManifestError> {
        let mut hashable_manifest = self.clone();
        hashable_manifest.checksums.manifest_sha256.clear();

        let json = serde_json::to_string(&hashable_manifest)?;
        let digest = Sha256::digest(json.as_bytes());

        Ok(hex::encode(digest))
    }

    pub fn manifest_blob_path(&self) -> Result<String, ManifestError> {
        Ok(manifest_blob_path(&self.tile_set_id)?)
    }

    pub fn latest_pointer(
        &self,
        updated_at: DateTime<Utc>,
    ) -> Result<LatestManifestPointer, ManifestError> {
        Ok(LatestManifestPointer {
            tile_set_id: self.tile_set_id.clone(),
            manifest_blob_path: self.manifest_blob_path()?,
            manifest_sha256: self.checksums.manifest_sha256.clone(),
            updated_at,
        })
    }
}

impl LatestManifestPointer {
    pub fn to_pretty_json(&self) -> Result<String, ManifestError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn blob_path() -> &'static str {
        latest_manifest_blob_path()
    }
}

impl From<GeographicBounds> for ManifestBounds {
    fn from(bounds: GeographicBounds) -> Self {
        Self {
            west: bounds.west,
            south: bounds.south,
            east: bounds.east,
            north: bounds.north,
        }
    }
}

pub fn build_tile_url_template(
    cdn_base_url: &str,
    tile_set_id: &str,
) -> Result<String, ManifestError> {
    manifest_blob_path(tile_set_id)?;

    Ok(format!(
        "{}/tiles/{tile_set_id}/{{z}}/{{x}}/{{y}}.png",
        cdn_base_url.trim_end_matches('/')
    ))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::config::TileBounds;

    fn test_config() -> AppConfig {
        AppConfig {
            rust_log: "processing_svc=debug".to_owned(),
            database_url: "postgres://localhost/lumenhorizon".to_owned(),
            azure_storage_account: "devstoreaccount1".to_owned(),
            azure_storage_access_key: "test-key".to_owned(),
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

    fn test_manifest_for_tile_set(tile_set_id: &str) -> TileManifest {
        TileManifest::from_config(
            &test_config(),
            TileManifestInput {
                tile_set_id: tile_set_id.to_owned(),
                dataset_date: NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                generated_at: Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
                processor_version: "processing-svc:test-sha".to_owned(),
                bounds: GeographicBounds {
                    west: -125.0,
                    south: 24.0,
                    east: -66.0,
                    north: 50.0,
                },
                tile_count: 12345,
                source_granules: vec![SourceGranule {
                    ingest_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
                    product: "VNP46A2".to_owned(),
                    blob_path: "VNP46A2/2026-05-21/h11v06.h5".to_owned(),
                }],
            },
        )
        .unwrap()
    }

    #[test]
    fn computes_stable_manifest_checksum_for_same_input() {
        let first = test_manifest_for_tile_set("2026-05-21-radiance-dark-sky-v1-a1b2c3d4");
        let second = test_manifest_for_tile_set("2026-05-21-radiance-dark-sky-v1-a1b2c3d4");

        assert_eq!(
            first.checksums.manifest_sha256,
            second.checksums.manifest_sha256
        );
        assert_eq!(first.checksums.manifest_sha256.len(), 64);
    }

    #[test]
    fn changes_manifest_checksum_when_tile_set_id_changes() {
        let first = test_manifest_for_tile_set("2026-05-21-radiance-dark-sky-v1-a1b2c3d4");
        let second = test_manifest_for_tile_set("2026-05-21-radiance-dark-sky-v1-deadbeef");

        assert_ne!(
            first.checksums.manifest_sha256,
            second.checksums.manifest_sha256
        );
    }

    #[test]
    fn pretty_json_contains_computed_manifest_checksum() {
        let manifest = test_manifest_for_tile_set("2026-05-21-radiance-dark-sky-v1-a1b2c3d4");
        let json = manifest.to_pretty_json().unwrap();

        assert!(json.contains(&manifest.checksums.manifest_sha256));
        assert!(!manifest.checksums.manifest_sha256.is_empty());
    }

    #[test]
    fn builds_tile_url_template_from_cdn_and_tile_set_id() {
        let template = build_tile_url_template(
            "https://tiles.lumenhorizon.com/",
            "2026-05-21-radiance-dark-sky-v1-a1b2c3d4",
        )
        .unwrap();

        assert_eq!(
            template,
            "https://tiles.lumenhorizon.com/tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4/{z}/{x}/{y}.png"
        );
    }

    #[test]
    fn rejects_container_prefixed_tile_set_id_in_url_template() {
        assert!(matches!(
            build_tile_url_template(
                "https://tiles.lumenhorizon.com",
                "processed-tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4",
            ),
            Err(ManifestError::TileMath(TileMathError::InvalidTileSetId))
        ));
    }

    #[test]
    fn serializes_manifest_contract_fields() {
        let manifest = TileManifest::from_config(
            &test_config(),
            TileManifestInput {
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-a1b2c3d4".to_owned(),
                dataset_date: NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                generated_at: Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
                processor_version: "processing-svc:test-sha".to_owned(),
                bounds: GeographicBounds {
                    west: -125.0,
                    south: 24.0,
                    east: -66.0,
                    north: 50.0,
                },
                tile_count: 12345,
                source_granules: vec![SourceGranule {
                    ingest_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
                    product: "VNP46A2".to_owned(),
                    blob_path: "VNP46A2/2026-05-21/h11v06.h5".to_owned(),
                }],
            },
        )
        .unwrap();

        assert_eq!(manifest.checksums.manifest_sha256.len(), 64);

        let json = manifest.to_pretty_json().unwrap();

        assert!(json.contains(r#""tile_set_id": "2026-05-21-radiance-dark-sky-v1-a1b2c3d4""#));
        assert!(json.contains(r#""classification_version": "radiance-dark-sky-v1""#));
        assert!(json.contains(r#""tile_url_template": "https://tiles.lumenhorizon.com/tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4/{z}/{x}/{y}.png""#));
        assert_eq!(
            manifest.manifest_blob_path().unwrap(),
            "manifests/2026-05-21-radiance-dark-sky-v1-a1b2c3d4.json"
        );
    }

    #[test]
    fn builds_latest_manifest_pointer_to_immutable_manifest() {
        let manifest = test_manifest_for_tile_set("2026-05-21-radiance-dark-sky-v1-a1b2c3d4");
        let updated_at = Utc.with_ymd_and_hms(2026, 5, 21, 10, 0, 0).unwrap();

        let pointer = manifest.latest_pointer(updated_at).unwrap();

        assert_eq!(pointer.tile_set_id, manifest.tile_set_id);
        assert_eq!(
            pointer.manifest_blob_path,
            "manifests/2026-05-21-radiance-dark-sky-v1-a1b2c3d4.json"
        );
        assert_eq!(pointer.manifest_sha256, manifest.checksums.manifest_sha256);
        assert_eq!(pointer.updated_at, updated_at);
    }

    #[test]
    fn latest_manifest_pointer_has_fixed_blob_path() {
        assert_eq!(LatestManifestPointer::blob_path(), "manifests/latest.json");
    }

    #[test]
    fn serializes_latest_manifest_pointer_contract_fields() {
        let manifest = test_manifest_for_tile_set("2026-05-21-radiance-dark-sky-v1-a1b2c3d4");
        let pointer = manifest
            .latest_pointer(Utc.with_ymd_and_hms(2026, 5, 21, 10, 0, 0).unwrap())
            .unwrap();

        let json = pointer.to_pretty_json().unwrap();

        assert!(json.contains(r#""tile_set_id": "2026-05-21-radiance-dark-sky-v1-a1b2c3d4""#));
        assert!(json.contains(
            r#""manifest_blob_path": "manifests/2026-05-21-radiance-dark-sky-v1-a1b2c3d4.json""#
        ));
        assert!(json.contains(&manifest.checksums.manifest_sha256));
    }
}
