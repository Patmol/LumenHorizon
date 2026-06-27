use std::path::{Path, PathBuf};

use chrono::{NaiveDate, Utc};
use image::{codecs::png::PngEncoder, ColorType, ImageEncoder, Rgba, RgbaImage};
use sha2::{Digest, Sha256};
use shared::processing_message::ProcessingProduct;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    db::{self, MosaicSourceRecord},
    generate::{
        self, plan_tile_generation_for_bounds, raster_window_for_tile,
        render_tile_window_from_granule, GeneratedTileSet,
    },
    hdf_cli::{self, RasterShape},
    manifest::{ManifestError, SourceGranule, TileManifest, TileManifestInput},
    publish::{self, RenderedTile},
    science::{self, DatasetMapping},
    storage::{BlobStorageClient, StorageError},
    tiles::{
        bounds_for_tiles, clip_bounds, summarize_viirs_coverage, tile_bounds, viirs_tile_bounds,
        viirs_tiles_for_bounds, GeographicBounds, TileCoord, TileMathError, ViirsCoverageSummary,
        ViirsTileCoord,
    },
    ui,
};

#[derive(Debug, Error)]
pub enum MosaicError {
    #[error(transparent)]
    Database(#[from] db::DbError),

    #[error(transparent)]
    Generate(#[from] generate::GenerateError),

    #[error(transparent)]
    HdfCli(#[from] hdf_cli::HdfCliError),

    #[error(transparent)]
    Manifest(#[from] ManifestError),

    #[error(transparent)]
    Publish(#[from] publish::PublishError),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    TileMath(#[from] TileMathError),

    #[error("mosaic error: unsupported product '{product}'")]
    UnsupportedProduct { product: String },

    #[error("mosaic error: found {found} processed source granule(s) for {product} {dataset_date}; at least 2 are required")]
    TooFewSources {
        product: String,
        dataset_date: NaiveDate,
        found: usize,
    },

    #[error("mosaic error: refusing to promote incomplete public latest for {product} {dataset_date}: {present_tile_count}/{expected_tile_count} expected VIIRS tile(s) present; missing tiles: {missing_tiles}")]
    IncompletePublicCoverage {
        product: String,
        dataset_date: NaiveDate,
        expected_tile_count: usize,
        present_tile_count: usize,
        missing_tiles: String,
    },

    #[error("mosaic error: no processed source date found for {product} with at least 2 eligible granule tile set(s)")]
    NoEligibleDatasetDate { product: String },

    #[error("mosaic error: failed to create workspace '{path}': {source}")]
    CreateWorkspace {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("mosaic error: failed to decode rendered tile z{z}/{x}/{y}: {source}")]
    DecodeTile {
        z: u8,
        x: u32,
        y: u32,
        source: image::ImageError,
    },

    #[error("mosaic error: failed to encode rendered tile z{z}/{x}/{y}: {source}")]
    EncodeTile {
        z: u8,
        x: u32,
        y: u32,
        source: image::ImageError,
    },

    #[error("mosaic error: rendered tile z{z}/{x}/{y} had dimensions {width}x{height}, expected {expected}x{expected}")]
    TileDimensionMismatch {
        z: u8,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        expected: u32,
    },
}

struct MosaicWorkspace {
    root: PathBuf,
}

impl MosaicWorkspace {
    fn new(correlation_id: Uuid) -> Self {
        Self {
            root: std::env::temp_dir().join(format!("lumenhorizon-mosaic-{correlation_id}")),
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn granule_path(&self, ingest_id: Uuid) -> PathBuf {
        self.root.join(format!("{ingest_id}.h5"))
    }
}

struct MosaicSource {
    record: MosaicSourceRecord,
    local_path: PathBuf,
    source_bounds: GeographicBounds,
    generation_bounds: GeographicBounds,
    raster_shape: RasterShape,
}

pub(crate) async fn publish_mosaic(
    config: &AppConfig,
    product: &str,
    dataset_date: Option<NaiveDate>,
    promote_public_latest: bool,
    allow_incomplete_public_latest: bool,
    correlation_id: Uuid,
) -> Result<(), MosaicError> {
    let workspace = MosaicWorkspace::new(correlation_id);
    let result = publish_mosaic_with_workspace(
        config,
        product,
        dataset_date,
        promote_public_latest,
        allow_incomplete_public_latest,
        correlation_id,
        &workspace,
    )
    .await;

    cleanup_workspace(&workspace, correlation_id).await;

    result
}

async fn publish_mosaic_with_workspace(
    config: &AppConfig,
    product: &str,
    dataset_date: Option<NaiveDate>,
    promote_public_latest: bool,
    allow_incomplete_public_latest: bool,
    correlation_id: Uuid,
    workspace: &MosaicWorkspace,
) -> Result<(), MosaicError> {
    let product_kind =
        ProcessingProduct::parse(product).map_err(|_| MosaicError::UnsupportedProduct {
            product: product.to_owned(),
        })?;
    let mapping = science::dataset_mapping_for_product(product_kind);
    let pool = db::connect(&config.database_url).await?;
    let dataset_date = resolve_mosaic_dataset_date(config, &pool, product, dataset_date).await?;

    tokio::fs::create_dir_all(workspace.root())
        .await
        .map_err(|source| MosaicError::CreateWorkspace {
            path: workspace.root().to_path_buf(),
            source,
        })?;

    ui::status(format_args!(
        "selecting processed {product} source granules for {dataset_date}"
    ));
    let records = db::select_mosaic_sources(
        &pool,
        product,
        dataset_date,
        &config.tile_classification_version,
        &config.tile_render_version,
    )
    .await?;
    ui::success(format_args!(
        "selected {} processed source granule(s) for {product} {dataset_date}",
        records.len()
    ));

    let coverage = mosaic_coverage_summary(config, &records)?;
    log_mosaic_coverage(product, dataset_date, &coverage);
    enforce_public_coverage_policy(
        product,
        dataset_date,
        promote_public_latest,
        allow_incomplete_public_latest,
        &coverage,
    )?;

    if records.len() < 2 {
        return Err(MosaicError::TooFewSources {
            product: product.to_owned(),
            dataset_date,
            found: records.len(),
        });
    }

    hdf_cli::verify_gdalinfo_available()?;
    let blob_client = BlobStorageClient::new(config)?;
    let sources =
        download_mosaic_sources(config, &blob_client, mapping, records, workspace).await?;

    if sources.len() < 2 {
        return Err(MosaicError::TooFewSources {
            product: product.to_owned(),
            dataset_date,
            found: sources.len(),
        });
    }

    let tile_set =
        generate_mosaic_tile_set(config, product, dataset_date, mapping, &sources, coverage)?;

    ui::status(format_args!(
        "publishing mosaic tile set {} to '{}'",
        tile_set.manifest.tile_set_id, config.processed_tiles_container
    ));
    let publication = publish::publish_generated_tile_set(
        config,
        &pool,
        &blob_client,
        &tile_set,
        publish::PublicationMode::ProductMosaic {
            product,
            promote_public_latest,
        },
        Utc::now(),
    )
    .await?;

    ui::success(format_args!(
        "published mosaic {}; product latest updated: {}; public latest updated: {}",
        tile_set.manifest.tile_set_id,
        publication.product_latest_pointer.is_some(),
        publication.public_latest_pointer.is_some()
    ));
    tracing::info!(
        command_correlation_id = %correlation_id,
        product,
        dataset_date = %dataset_date,
        tile_set_id = tile_set.manifest.tile_set_id,
        source_granule_count = tile_set.manifest.source_granules.len(),
        tile_count = tile_set.manifest.tile_count,
        product_latest_promoted = publication.product_latest_pointer.is_some(),
        public_latest_promoted = publication.public_latest_pointer.is_some(),
        "published product/date mosaic tile set"
    );

    Ok(())
}

async fn resolve_mosaic_dataset_date(
    config: &AppConfig,
    pool: &sqlx::PgPool,
    product: &str,
    dataset_date: Option<NaiveDate>,
) -> Result<NaiveDate, MosaicError> {
    if let Some(dataset_date) = dataset_date {
        return Ok(dataset_date);
    }

    ui::status(format_args!(
        "resolving latest processed dataset date for {product} mosaic"
    ));
    let latest_date = db::select_latest_mosaic_dataset_date(
        pool,
        product,
        &config.tile_classification_version,
        &config.tile_render_version,
    )
    .await?
    .ok_or_else(|| MosaicError::NoEligibleDatasetDate {
        product: product.to_owned(),
    })?;

    ui::success(format_args!(
        "resolved latest processed dataset date for {product}: {latest_date}"
    ));

    Ok(latest_date)
}

async fn download_mosaic_sources(
    config: &AppConfig,
    blob_client: &BlobStorageClient,
    mapping: &DatasetMapping,
    records: Vec<MosaicSourceRecord>,
    workspace: &MosaicWorkspace,
) -> Result<Vec<MosaicSource>, MosaicError> {
    let configured_bounds = GeographicBounds::from(config.tile_bounds);
    let mut sources = Vec::new();
    let progress = ui::progress("preparing mosaic source granules", records.len());
    let mut skipped_out_of_bounds = 0usize;

    for record in records {
        let source_bounds = viirs_tile_bounds(record.tile_h, record.tile_v)?;
        let Some(generation_bounds) = clip_bounds(source_bounds, configured_bounds) else {
            progress.set_message(format!(
                "skipped {} h{:02}v{:02}: outside tile bounds",
                record.product, record.tile_h, record.tile_v
            ));
            progress.inc(1);
            skipped_out_of_bounds += 1;
            continue;
        };

        let local_path = workspace.granule_path(record.ingest_id);
        progress.set_message(format!(
            "downloading {} h{:02}v{:02} ({})",
            record.product, record.tile_h, record.tile_v, record.ingest_id
        ));
        blob_client
            .download_raw_blob_to_path(&config.raw_viirs_container, &record.blob_path, &local_path)
            .await?;

        progress.set_message(format!(
            "inspecting {} h{:02}v{:02}",
            record.product, record.tile_h, record.tile_v
        ));
        let raster_shape = hdf_cli::radiance_shape(&local_path, mapping)?;
        sources.push(MosaicSource {
            record,
            local_path,
            source_bounds,
            generation_bounds,
            raster_shape,
        });
        progress.inc(1);
    }

    progress.finish(format!(
        "prepared {} mosaic source(s), skipped {} outside bounds",
        sources.len(),
        skipped_out_of_bounds
    ));

    Ok(sources)
}

fn generate_mosaic_tile_set(
    config: &AppConfig,
    product: &str,
    dataset_date: NaiveDate,
    mapping: &DatasetMapping,
    sources: &[MosaicSource],
    coverage: ViirsCoverageSummary,
) -> Result<GeneratedTileSet, MosaicError> {
    let mosaic_bounds = mosaic_generation_bounds(sources);
    let plan = plan_tile_generation_for_bounds(
        mosaic_bounds,
        config.tile_min_zoom,
        config.tile_max_native_zoom,
    )?;

    ui::status(format_args!(
        "planned {} mosaic tile(s) for {product} {dataset_date}",
        plan.tile_count
    ));

    let mut tiles = Vec::new();
    let progress = ui::progress("rendering mosaic tiles", plan.tile_count as usize);
    for range in &plan.ranges {
        for x in range.min_x..=range.max_x {
            for y in range.min_y..=range.max_y {
                let coord = TileCoord { z: range.z, x, y };
                progress.set_message(format!("rendering z{}/x{}/y{}", coord.z, coord.x, coord.y));
                if let Some(tile) = render_mosaic_tile(config, mapping, sources, coord)? {
                    tiles.push(tile);
                }
                progress.inc(1);
            }
        }
    }
    progress.finish(format!("rendered {} non-empty mosaic tile(s)", tiles.len()));

    let coverage_bounds =
        coverage_bounds_for_tiles(&tiles, config.tile_max_native_zoom, mosaic_bounds)?;
    let source_granules = sources
        .iter()
        .map(|source| SourceGranule {
            ingest_id: source.record.ingest_id,
            product: source.record.product.clone(),
            blob_path: source.record.blob_path.clone(),
        })
        .collect::<Vec<_>>();
    let tile_set_id = mosaic_tile_set_id(config, product, dataset_date, sources);

    let manifest = TileManifest::from_config(
        config,
        TileManifestInput {
            tile_set_id,
            dataset_date,
            generated_at: Utc::now(),
            processor_version: processor_version(),
            bounds: coverage_bounds,
            tile_count: tiles.len() as u32,
            source_granules,
            coverage: Some(coverage),
        },
    )?;

    Ok(GeneratedTileSet {
        plan,
        tiles,
        manifest,
    })
}

fn mosaic_coverage_summary(
    config: &AppConfig,
    records: &[MosaicSourceRecord],
) -> Result<ViirsCoverageSummary, MosaicError> {
    let expected_tiles = viirs_tiles_for_bounds(GeographicBounds::from(config.tile_bounds))?;
    let present_tiles = records.iter().map(|record| ViirsTileCoord {
        tile_h: record.tile_h,
        tile_v: record.tile_v,
    });

    Ok(summarize_viirs_coverage(expected_tiles, present_tiles))
}

fn log_mosaic_coverage(product: &str, dataset_date: NaiveDate, coverage: &ViirsCoverageSummary) {
    if coverage.complete {
        tracing::info!(
            product,
            dataset_date = %dataset_date,
            expected_tile_count = coverage.expected_tile_count,
            present_tile_count = coverage.present_tile_count,
            coverage_fraction = coverage.coverage_fraction,
            "mosaic sources cover all expected in-bounds VIIRS tiles"
        );
    } else {
        tracing::warn!(
            product,
            dataset_date = %dataset_date,
            expected_tile_count = coverage.expected_tile_count,
            present_tile_count = coverage.present_tile_count,
            coverage_fraction = coverage.coverage_fraction,
            missing_tiles = %format_viirs_tiles(&coverage.missing_tiles),
            missing_columns = ?coverage.missing_columns,
            missing_rows = ?coverage.missing_rows,
            "mosaic sources are missing expected in-bounds VIIRS tiles"
        );
    }
}

fn enforce_public_coverage_policy(
    product: &str,
    dataset_date: NaiveDate,
    promote_public_latest: bool,
    allow_incomplete_public_latest: bool,
    coverage: &ViirsCoverageSummary,
) -> Result<(), MosaicError> {
    if !promote_public_latest || coverage.complete {
        return Ok(());
    }

    let missing_tiles = format_viirs_tiles(&coverage.missing_tiles);
    if allow_incomplete_public_latest {
        ui::warn(format_args!(
            "promoting incomplete public latest for {product} {dataset_date}; missing tiles: {missing_tiles}"
        ));
        tracing::warn!(
            product,
            dataset_date = %dataset_date,
            expected_tile_count = coverage.expected_tile_count,
            present_tile_count = coverage.present_tile_count,
            coverage_fraction = coverage.coverage_fraction,
            missing_tiles = %missing_tiles,
            missing_columns = ?coverage.missing_columns,
            missing_rows = ?coverage.missing_rows,
            "public latest promotion explicitly allowed despite incomplete mosaic coverage"
        );
        return Ok(());
    }

    Err(MosaicError::IncompletePublicCoverage {
        product: product.to_owned(),
        dataset_date,
        expected_tile_count: coverage.expected_tile_count,
        present_tile_count: coverage.present_tile_count,
        missing_tiles,
    })
}

fn format_viirs_tiles(tiles: &[ViirsTileCoord]) -> String {
    tiles
        .iter()
        .map(|coord| format!("h{:02}v{:02}", coord.tile_h, coord.tile_v))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_mosaic_tile(
    config: &AppConfig,
    mapping: &DatasetMapping,
    sources: &[MosaicSource],
    coord: TileCoord,
) -> Result<Option<RenderedTile>, MosaicError> {
    let tile_bounds = tile_bounds(coord)?;
    let expected_size = u32::from(config.tile_size);
    let mut image = RgbaImage::from_pixel(expected_size, expected_size, Rgba([0, 0, 0, 0]));

    for source in sources {
        if clip_bounds(tile_bounds, source.source_bounds).is_none() {
            continue;
        }

        let window = raster_window_for_tile(
            coord,
            source.source_bounds,
            source.raster_shape,
            config.tile_size,
        )?;
        let rendered = render_tile_window_from_granule(
            &source.local_path,
            mapping,
            coord,
            window,
            config.tile_size,
        )?;
        if !rendered.has_renderable_evidence() {
            continue;
        }

        composite_rendered_tile(&mut image, &rendered, expected_size)?;
    }

    let renderable_pixel_count = image.pixels().filter(|pixel| pixel.0[3] > 0).count() as u32;
    if renderable_pixel_count == 0 {
        return Ok(None);
    }

    Ok(Some(RenderedTile {
        coord,
        png_bytes: encode_rgba_png(coord, &image)?,
        renderable_pixel_count,
    }))
}

fn composite_rendered_tile(
    target: &mut RgbaImage,
    rendered: &RenderedTile,
    expected_size: u32,
) -> Result<(), MosaicError> {
    let overlay = image::load_from_memory(&rendered.png_bytes)
        .map_err(|source| MosaicError::DecodeTile {
            z: rendered.coord.z,
            x: rendered.coord.x,
            y: rendered.coord.y,
            source,
        })?
        .to_rgba8();

    if overlay.width() != expected_size || overlay.height() != expected_size {
        return Err(MosaicError::TileDimensionMismatch {
            z: rendered.coord.z,
            x: rendered.coord.x,
            y: rendered.coord.y,
            width: overlay.width(),
            height: overlay.height(),
            expected: expected_size,
        });
    }

    for (x, y, pixel) in overlay.enumerate_pixels() {
        if pixel.0[3] > 0 {
            target.put_pixel(x, y, *pixel);
        }
    }

    Ok(())
}

fn encode_rgba_png(coord: TileCoord, image: &RgbaImage) -> Result<Vec<u8>, MosaicError> {
    let mut png = Vec::new();
    let encoder = PngEncoder::new(&mut png);
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ColorType::Rgba8.into(),
        )
        .map_err(|source| MosaicError::EncodeTile {
            z: coord.z,
            x: coord.x,
            y: coord.y,
            source,
        })?;

    Ok(png)
}

fn mosaic_generation_bounds(sources: &[MosaicSource]) -> GeographicBounds {
    sources
        .iter()
        .map(|source| source.generation_bounds)
        .reduce(union_bounds)
        .expect("mosaic source precondition requires at least one source")
}

fn union_bounds(left: GeographicBounds, right: GeographicBounds) -> GeographicBounds {
    GeographicBounds {
        west: left.west.min(right.west),
        south: left.south.min(right.south),
        east: left.east.max(right.east),
        north: left.north.max(right.north),
    }
}

fn coverage_bounds_for_tiles(
    tiles: &[RenderedTile],
    max_native_zoom: u8,
    generation_bounds: GeographicBounds,
) -> Result<GeographicBounds, MosaicError> {
    let native_tiles = tiles
        .iter()
        .filter(|tile| tile.coord.z == max_native_zoom)
        .map(|tile| tile.coord)
        .collect::<Vec<_>>();

    let coords = if native_tiles.is_empty() {
        tiles.iter().map(|tile| tile.coord).collect::<Vec<_>>()
    } else {
        native_tiles
    };

    bounds_for_tiles(coords)?
        .ok_or_else(|| generate::GenerateError::NoRenderableTiles { generation_bounds }.into())
}

fn mosaic_tile_set_id(
    config: &AppConfig,
    product: &str,
    dataset_date: NaiveDate,
    sources: &[MosaicSource],
) -> String {
    let mut hash = Sha256::new();
    hash.update(product.as_bytes());
    hash.update(dataset_date.to_string().as_bytes());
    hash.update(config.tile_classification_version.as_bytes());
    hash.update(config.tile_render_version.as_bytes());
    for source in sources {
        hash.update(source.record.tile_set_id.as_bytes());
    }
    let digest = hash.finalize();
    let suffix = hex::encode(&digest[..4]);

    format!(
        "{}-{}-{}-mosaic-{}",
        dataset_date,
        config.tile_classification_version,
        product.to_ascii_lowercase(),
        suffix
    )
}

async fn cleanup_workspace(workspace: &MosaicWorkspace, correlation_id: Uuid) {
    match tokio::fs::remove_dir_all(workspace.root()).await {
        Ok(()) => tracing::debug!(
            command_correlation_id = %correlation_id,
            local_workspace = %workspace.root().display(),
            "removed local mosaic workspace"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(
            command_correlation_id = %correlation_id,
            local_workspace = %workspace.root().display(),
            error = %error,
            "failed to remove local mosaic workspace"
        ),
    }
}

fn processor_version() -> String {
    format!("processing-svc:{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::TileBounds, db::MosaicSourceRecord, hdf_cli::RasterShape};
    use chrono::TimeZone;

    fn test_config() -> AppConfig {
        AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some("test-key".to_owned()),
            "TILE_MIN_ZOOM" => Some("3".to_owned()),
            "TILE_MAX_NATIVE_ZOOM" => Some("3".to_owned()),
            "TILE_BOUNDS" => Some("-125,24,-66,50".to_owned()),
            "TILE_SIZE" => Some("2".to_owned()),
            _ => None,
        })
        .unwrap()
    }

    fn source(tile_h: i16, tile_v: i16, tile_set_id: &str) -> MosaicSource {
        let source_bounds = viirs_tile_bounds(tile_h, tile_v).unwrap();
        MosaicSource {
            record: MosaicSourceRecord {
                ingest_id: Uuid::new_v4(),
                blob_path: format!("VNP46A2/2026-05-21/h{tile_h:02}v{tile_v:02}.h5"),
                product: "VNP46A2".to_owned(),
                granule_date: Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap(),
                tile_h,
                tile_v,
                tile_set_id: tile_set_id.to_owned(),
            },
            local_path: PathBuf::from("/tmp/test.h5"),
            source_bounds,
            generation_bounds: clip_bounds(
                source_bounds,
                GeographicBounds::from(TileBounds {
                    west: -125.0,
                    south: 24.0,
                    east: -66.0,
                    north: 50.0,
                }),
            )
            .unwrap(),
            raster_shape: RasterShape {
                width: 1000,
                height: 1000,
            },
        }
    }

    fn source_record(tile_h: i16, tile_v: i16, tile_set_id: &str) -> MosaicSourceRecord {
        source(tile_h, tile_v, tile_set_id).record
    }

    #[test]
    fn mosaic_bounds_unions_source_generation_bounds() {
        let bounds = mosaic_generation_bounds(&[source(5, 6, "west"), source(11, 6, "east")]);

        assert_eq!(
            bounds,
            GeographicBounds {
                west: -125.0,
                south: 24.0,
                east: -66.0,
                north: 30.0,
            }
        );
    }

    #[test]
    fn mosaic_tile_set_id_is_stable_for_same_sources() {
        let config = test_config();
        let sources = [source(5, 6, "tile-set-a"), source(11, 6, "tile-set-b")];
        let dataset_date = NaiveDate::from_ymd_opt(2026, 5, 21).unwrap();

        let first = mosaic_tile_set_id(&config, "VNP46A2", dataset_date, &sources);
        let second = mosaic_tile_set_id(&config, "VNP46A2", dataset_date, &sources);

        assert_eq!(first, second);
        assert!(first.starts_with("2026-05-21-radiance-dark-sky-v1-vnp46a2-mosaic-"));
    }

    #[test]
    fn mosaic_coverage_summary_reports_missing_columns() {
        let records = [
            source_record(6, 6, "h06v06"),
            source_record(7, 4, "h07v04"),
            source_record(7, 5, "h07v05"),
            source_record(9, 4, "h09v04"),
            source_record(9, 5, "h09v05"),
            source_record(9, 6, "h09v06"),
            source_record(10, 5, "h10v05"),
            source_record(11, 6, "h11v06"),
        ];

        let coverage = mosaic_coverage_summary(&test_config(), &records).unwrap();

        assert_eq!(coverage.expected_tile_count, 21);
        assert_eq!(coverage.present_tile_count, 8);
        assert!(!coverage.complete);
        assert_eq!(coverage.missing_columns, vec![5, 8]);
        assert!(coverage.missing_rows.is_empty());
        assert_eq!(
            format_viirs_tiles(&coverage.missing_tiles[..3]),
            "h05v04,h05v05,h05v06"
        );
    }

    #[test]
    fn public_latest_gate_blocks_incomplete_coverage_without_override() {
        let records = [
            source_record(6, 6, "h06v06"),
            source_record(11, 6, "h11v06"),
        ];
        let coverage = mosaic_coverage_summary(&test_config(), &records).unwrap();
        let dataset_date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();

        let error = enforce_public_coverage_policy("VNP46A3", dataset_date, true, false, &coverage)
            .unwrap_err();

        assert!(matches!(
            error,
            MosaicError::IncompletePublicCoverage {
                product,
                dataset_date: date,
                expected_tile_count: 21,
                present_tile_count: 2,
                missing_tiles,
            } if product == "VNP46A3" && date == dataset_date && missing_tiles.contains("h05v04")
        ));
        assert!(
            enforce_public_coverage_policy("VNP46A3", dataset_date, false, false, &coverage,)
                .is_ok()
        );
        assert!(
            enforce_public_coverage_policy("VNP46A3", dataset_date, true, true, &coverage,).is_ok()
        );
    }
}
