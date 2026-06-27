use chrono::{DateTime, Utc};
use shared::processing_message::{product_definition, ProductCadence};
use sqlx::PgPool;
use thiserror::Error;

use crate::{
    config::AppConfig,
    db::{self, DbError},
    generate::GeneratedTileSet,
    manifest::{LatestManifestPointer, ManifestError, TileManifest},
    storage::{BlobStorageClient, StorageError},
    tiles::{tile_blob_path, TileCoord, TileMathError},
    ui,
};

const JSON_CONTENT_TYPE: &str = "application/json";
const PNG_CONTENT_TYPE: &str = "image/png";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationMode<'a> {
    IntermediateGranule {
        product: &'a str,
    },
    ProductMosaic {
        product: &'a str,
        promote_public_latest: bool,
    },
}

impl<'a> PublicationMode<'a> {
    fn product(self) -> &'a str {
        match self {
            Self::IntermediateGranule { product } | Self::ProductMosaic { product, .. } => product,
        }
    }

    fn tile_set_kind(self) -> db::TileSetKind {
        match self {
            Self::IntermediateGranule { .. } => db::TileSetKind::Granule,
            Self::ProductMosaic { .. } => db::TileSetKind::Mosaic,
        }
    }

    fn promotes_product_latest(self) -> bool {
        matches!(self, Self::ProductMosaic { .. })
    }

    fn promotes_public_latest(self) -> bool {
        matches!(
            self,
            Self::ProductMosaic {
                promote_public_latest: true,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishTileSetOutcome {
    pub public_latest_pointer: Option<LatestManifestPointer>,
    pub product_latest_pointer: Option<LatestManifestPointer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedTile {
    pub coord: TileCoord,
    pub png_bytes: Vec<u8>,
    pub renderable_pixel_count: u32,
}

impl RenderedTile {
    pub fn has_renderable_evidence(&self) -> bool {
        self.renderable_pixel_count > 0
    }
}

#[derive(Debug, Error)]
pub enum PublishError {
    #[error(transparent)]
    Database(#[from] DbError),

    #[error(transparent)]
    Manifest(#[from] ManifestError),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    TileMath(#[from] TileMathError),

    #[error("unsupported tile publication product '{product}'")]
    UnsupportedProduct { product: String },
}

pub async fn publish_generated_tile_set(
    config: &AppConfig,
    pool: &PgPool,
    blob_client: &BlobStorageClient,
    tile_set: &GeneratedTileSet,
    mode: PublicationMode<'_>,
    published_at: DateTime<Utc>,
) -> Result<PublishTileSetOutcome, PublishError> {
    publish_tile_set(
        config,
        pool,
        blob_client,
        &tile_set.manifest,
        &tile_set.tiles,
        mode,
        published_at,
    )
    .await
}

pub async fn publish_tile_set(
    config: &AppConfig,
    pool: &PgPool,
    blob_client: &BlobStorageClient,
    manifest: &TileManifest,
    tiles: &[RenderedTile],
    mode: PublicationMode<'_>,
    published_at: DateTime<Utc>,
) -> Result<PublishTileSetOutcome, PublishError> {
    ui::status(format_args!(
        "uploading {} tile blob(s) for {}",
        tiles.len(),
        manifest.tile_set_id
    ));
    let progress = ui::progress("uploading tile blobs", tiles.len());

    for tile in tiles {
        let blob_path = tile_blob_path(&manifest.tile_set_id, tile.coord)?;

        blob_client
            .upload_processed_blob(
                &config.processed_tiles_container,
                &blob_path,
                &tile.png_bytes,
                PNG_CONTENT_TYPE,
                &config.tile_immutable_cache_control,
            )
            .await?;

        progress.set_message(format!("latest {blob_path}"));
        progress.inc(1);
    }
    progress.finish(format!("uploaded {} tile blob(s)", tiles.len()));

    publish_tile_manifest(config, pool, blob_client, manifest, mode, published_at).await
}

pub async fn publish_tile_manifest(
    config: &AppConfig,
    pool: &PgPool,
    blob_client: &BlobStorageClient,
    manifest: &TileManifest,
    mode: PublicationMode<'_>,
    published_at: DateTime<Utc>,
) -> Result<PublishTileSetOutcome, PublishError> {
    let manifest_json = manifest.to_pretty_json()?;
    let manifest_blob_path = manifest.manifest_blob_path()?;
    let product = mode.product();
    let cadence = product_cadence(product)?;

    ui::status(format_args!("uploading manifest {}", manifest_blob_path));
    blob_client
        .upload_processed_blob(
            &config.processed_tiles_container,
            &manifest_blob_path,
            manifest_json.as_bytes(),
            JSON_CONTENT_TYPE,
            &config.tile_immutable_cache_control,
        )
        .await?;

    ui::status(format_args!(
        "recording tile set {} in PostgreSQL",
        manifest.tile_set_id
    ));
    db::insert_tile_set_with_metadata(
        pool,
        db::TileSetInsert {
            manifest,
            product: Some(product),
            cadence: Some(cadence),
            kind: mode.tile_set_kind(),
        },
    )
    .await?;

    let product_latest_pointer = if mode.promotes_product_latest() {
        ui::status(format_args!(
            "promoting {} as latest {} tile set",
            manifest.tile_set_id, product
        ));
        db::promote_product_latest_tile_set(pool, &manifest.tile_set_id).await?;

        let latest_pointer = manifest.latest_pointer(published_at)?;
        upload_latest_pointer(
            config,
            blob_client,
            &product_latest_manifest_blob_path(product),
            &latest_pointer,
        )
        .await?;

        Some(latest_pointer)
    } else {
        None
    };

    let public_latest_pointer = if mode.promotes_public_latest() {
        ui::status(format_args!(
            "promoting {} as public latest tile set",
            manifest.tile_set_id
        ));
        db::promote_latest_tile_set(pool, &manifest.tile_set_id).await?;

        let latest_pointer = manifest.latest_pointer(published_at)?;
        upload_latest_pointer(
            config,
            blob_client,
            LatestManifestPointer::blob_path(),
            &latest_pointer,
        )
        .await?;

        Some(latest_pointer)
    } else {
        None
    };

    Ok(PublishTileSetOutcome {
        public_latest_pointer,
        product_latest_pointer,
    })
}

async fn upload_latest_pointer(
    config: &AppConfig,
    blob_client: &BlobStorageClient,
    blob_path: &str,
    latest_pointer: &LatestManifestPointer,
) -> Result<(), PublishError> {
    let latest_json = latest_pointer.to_pretty_json()?;

    ui::status(format_args!(
        "uploading latest manifest pointer {blob_path}"
    ));
    blob_client
        .upload_processed_blob(
            &config.processed_tiles_container,
            blob_path,
            latest_json.as_bytes(),
            JSON_CONTENT_TYPE,
            &config.tile_latest_cache_control,
        )
        .await?;

    Ok(())
}

fn product_cadence(product: &str) -> Result<ProductCadence, PublishError> {
    product_definition(product)
        .map(|definition| definition.cadence)
        .ok_or_else(|| PublishError::UnsupportedProduct {
            product: product.to_owned(),
        })
}

fn product_latest_manifest_blob_path(product: &str) -> String {
    format!("manifests/latest/{product}.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generate::{GeneratedTileSet, TileGenerationPlan},
        manifest::{TileManifest, TileManifestInput},
        tiles::{tile_blob_path, GeographicBounds, TileRange},
    };

    #[test]
    fn rendered_tile_uses_container_relative_blob_path() {
        let tile = RenderedTile {
            coord: TileCoord { z: 5, x: 8, y: 12 },
            png_bytes: vec![137, 80, 78, 71],
            renderable_pixel_count: 1,
        };

        let path = tile_blob_path("2026-05-21-radiance-dark-sky-v1-a1b2c3d4", tile.coord).unwrap();

        assert_eq!(
            path,
            "tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4/5/8/12.png"
        );
        assert_eq!(tile.png_bytes, vec![137, 80, 78, 71]);
    }

    #[test]
    fn generated_tile_set_exposes_manifest_and_tiles_for_publication() {
        let config = crate::config::AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some("test-key".to_owned()),
            "TILE_MIN_ZOOM" => Some("3".to_owned()),
            "TILE_MAX_NATIVE_ZOOM" => Some("3".to_owned()),
            "TILE_BOUNDS" => Some("-90,30,-80,40".to_owned()),
            "TILE_SIZE" => Some("2".to_owned()),
            _ => None,
        })
        .unwrap();

        let manifest = TileManifest::from_config(
            &config,
            TileManifestInput {
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-a1b2c3d4".to_owned(),
                dataset_date: chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                generated_at: chrono::Utc::now(),
                processor_version: "processing-svc:test-sha".to_owned(),
                bounds: GeographicBounds {
                    west: -90.0,
                    south: 30.0,
                    east: -80.0,
                    north: 40.0,
                },
                tile_count: 1,
                source_granules: Vec::new(),
                coverage: None,
            },
        )
        .unwrap();

        let tile_set = GeneratedTileSet {
            plan: TileGenerationPlan {
                ranges: vec![TileRange {
                    z: 3,
                    min_x: 2,
                    max_x: 2,
                    min_y: 3,
                    max_y: 3,
                }],
                tile_count: 1,
            },
            tiles: vec![RenderedTile {
                coord: TileCoord { z: 3, x: 2, y: 3 },
                png_bytes: b"fake-png".to_vec(),
                renderable_pixel_count: 1,
            }],
            manifest,
        };

        assert_eq!(
            tile_set.manifest.tile_set_id,
            "2026-05-21-radiance-dark-sky-v1-a1b2c3d4"
        );
        assert_eq!(tile_set.tiles.len(), 1);
        assert_eq!(tile_set.tiles[0].coord, TileCoord { z: 3, x: 2, y: 3 });
    }

    #[test]
    fn generated_tiles_and_latest_pointer_are_manifest_consistent() {
        let config = crate::config::AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some("test-key".to_owned()),
            "TILE_MIN_ZOOM" => Some("3".to_owned()),
            "TILE_MAX_NATIVE_ZOOM" => Some("3".to_owned()),
            "TILE_BOUNDS" => Some("-90,30,-80,40".to_owned()),
            "TILE_SIZE" => Some("2".to_owned()),
            _ => None,
        })
        .unwrap();

        let manifest = TileManifest::from_config(
            &config,
            TileManifestInput {
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-a1b2c3d4".to_owned(),
                dataset_date: chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                generated_at: chrono::Utc::now(),
                processor_version: "processing-svc:test-sha".to_owned(),
                bounds: GeographicBounds {
                    west: -90.0,
                    south: 30.0,
                    east: -80.0,
                    north: 40.0,
                },
                tile_count: 2,
                source_granules: Vec::new(),
                coverage: None,
            },
        )
        .unwrap();
        let tiles = [
            RenderedTile {
                coord: TileCoord { z: 3, x: 2, y: 3 },
                png_bytes: b"fake-png-a".to_vec(),
                renderable_pixel_count: 1,
            },
            RenderedTile {
                coord: TileCoord { z: 3, x: 2, y: 4 },
                png_bytes: b"fake-png-b".to_vec(),
                renderable_pixel_count: 1,
            },
        ];

        let tile_paths = tiles
            .iter()
            .map(|tile| tile_blob_path(&manifest.tile_set_id, tile.coord).unwrap())
            .collect::<Vec<_>>();
        let latest_pointer = manifest.latest_pointer(chrono::Utc::now()).unwrap();

        assert_eq!(
            tile_paths,
            vec![
                "tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4/3/2/3.png",
                "tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4/3/2/4.png",
            ]
        );
        assert_eq!(
            manifest.manifest_blob_path().unwrap(),
            "manifests/2026-05-21-radiance-dark-sky-v1-a1b2c3d4.json"
        );
        assert_eq!(
            latest_pointer.manifest_blob_path,
            manifest.manifest_blob_path().unwrap()
        );
        assert_eq!(
            latest_pointer.manifest_sha256,
            manifest.checksums.manifest_sha256
        );
        assert_eq!(manifest.tile_count as usize, tiles.len());
    }

    #[test]
    fn publication_modes_expose_latest_promotion_intent() {
        let intermediate = PublicationMode::IntermediateGranule { product: "VNP46A2" };
        let mosaic = PublicationMode::ProductMosaic {
            product: "VNP46A2",
            promote_public_latest: true,
        };

        assert_eq!(intermediate.product(), "VNP46A2");
        assert_eq!(intermediate.tile_set_kind(), db::TileSetKind::Granule);
        assert!(!intermediate.promotes_product_latest());
        assert!(!intermediate.promotes_public_latest());
        assert_eq!(mosaic.product(), "VNP46A2");
        assert_eq!(mosaic.tile_set_kind(), db::TileSetKind::Mosaic);
        assert!(mosaic.promotes_product_latest());
        assert!(mosaic.promotes_public_latest());
        assert_eq!(
            product_latest_manifest_blob_path("VNP46A2"),
            "manifests/latest/VNP46A2.json"
        );
    }
}
