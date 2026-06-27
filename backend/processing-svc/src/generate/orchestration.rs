//! Tile-set orchestration.
//!
//! This module connects tile planning, per-tile rendering, and manifest
//! construction for one source granule.

use std::{future::Future, path::Path, sync::Arc};

use chrono::{DateTime, NaiveDate, Utc};
use tokio::task::JoinSet;

use crate::{
    config::AppConfig,
    hdf_cli::RasterShape,
    manifest::{SourceGranule, TileManifest, TileManifestInput},
    publish::RenderedTile,
    science::DatasetMapping,
    tiles::{bounds_for_tiles, clip_bounds, GeographicBounds, TileCoord},
    ui,
};

use super::{
    error::GenerateError,
    planning::plan_tile_generation_for_bounds,
    rendering::render_tile_window_from_granule,
    types::{GeneratedTileSet, TileGenerationPlan},
    window::{raster_window_for_tile, TileRasterWindow},
};

/// Generates rendered tiles and a manifest for one source granule.
///
/// The source raster bounds and shape are used to map each planned map tile into
/// the granule before rendering product-specific radiance samples.
pub(crate) struct GranuleTileSetRequest<'a> {
    pub(crate) config: &'a AppConfig,
    pub(crate) granule_path: &'a Path,
    pub(crate) mapping: &'a DatasetMapping,
    pub(crate) tile_set_id: String,
    pub(crate) dataset_date: NaiveDate,
    pub(crate) generated_at: DateTime<Utc>,
    pub(crate) processor_version: String,
    pub(crate) source_bounds: GeographicBounds,
    pub(crate) raster_shape: RasterShape,
    pub(crate) source_granules: Vec<SourceGranule>,
}

struct TileSetBuildRequest {
    tile_set_id: String,
    dataset_date: NaiveDate,
    generated_at: DateTime<Utc>,
    processor_version: String,
    generation_bounds: GeographicBounds,
    source_bounds: GeographicBounds,
    raster_shape: RasterShape,
    source_granules: Vec<SourceGranule>,
}

pub(crate) async fn generate_tile_set_for_granule_with_manifest(
    request: GranuleTileSetRequest<'_>,
) -> Result<GeneratedTileSet, GenerateError> {
    let GranuleTileSetRequest {
        config,
        granule_path,
        mapping,
        tile_set_id,
        dataset_date,
        generated_at,
        processor_version,
        source_bounds,
        raster_shape,
        source_granules,
    } = request;

    let configured_bounds = GeographicBounds::from(config.tile_bounds);
    let generation_bounds = clip_bounds(source_bounds, configured_bounds).ok_or(
        GenerateError::ConfiguredBoundsOutsideSource {
            source_bounds,
            configured_bounds,
        },
    )?;

    let granule_path = Arc::new(granule_path.to_path_buf());
    let mapping = *mapping;
    let tile_size = config.tile_size;

    generate_tile_set_with_renderer(
        config,
        TileSetBuildRequest {
            tile_set_id,
            dataset_date,
            generated_at,
            processor_version,
            generation_bounds,
            source_bounds,
            raster_shape,
            source_granules,
        },
        move |coord, window| {
            let granule_path = Arc::clone(&granule_path);

            async move {
                let rendered = tokio::task::spawn_blocking(move || {
                    render_tile_window_from_granule(
                        &granule_path,
                        &mapping,
                        coord,
                        window,
                        tile_size,
                    )
                })
                .await
                .map_err(|source| GenerateError::RenderWorker { source })??;

                Ok(rendered)
            }
        },
    )
    .await
}

/// Builds a tile set using an injected renderer for each planned tile.
///
/// The renderer callback keeps the orchestration independent from the source of
/// rendered pixels, which makes tests and alternate renderers share the same
/// planning and manifest path.
async fn generate_tile_set_with_renderer<RenderTile, RenderFuture>(
    config: &AppConfig,
    request: TileSetBuildRequest,
    render_tile: RenderTile,
) -> Result<GeneratedTileSet, GenerateError>
where
    RenderTile: Fn(TileCoord, TileRasterWindow) -> RenderFuture + Clone + Send + 'static,
    RenderFuture: Future<Output = Result<RenderedTile, GenerateError>> + Send + 'static,
{
    let TileSetBuildRequest {
        tile_set_id,
        dataset_date,
        generated_at,
        processor_version,
        generation_bounds,
        source_bounds,
        raster_shape,
        source_granules,
    } = request;

    let plan = plan_tile_generation_for_bounds(
        generation_bounds,
        config.tile_min_zoom,
        config.tile_max_native_zoom,
    )?;

    tracing::info!(
        tile_set_id = %tile_set_id,
        tile_count = plan.tile_count,
        tile_min_zoom = config.tile_min_zoom,
        tile_max_native_zoom = config.tile_max_native_zoom,
        source_bounds_west = source_bounds.west,
        source_bounds_south = source_bounds.south,
        source_bounds_east = source_bounds.east,
        source_bounds_north = source_bounds.north,
        generation_bounds_west = generation_bounds.west,
        generation_bounds_south = generation_bounds.south,
        generation_bounds_east = generation_bounds.east,
        generation_bounds_north = generation_bounds.north,
        "planned tile generation for source granule"
    );
    ui::status(format_args!(
        "planned {} tile(s) for {} across zoom {}..{}",
        plan.tile_count, tile_set_id, config.tile_min_zoom, config.tile_max_native_zoom
    ));

    let rendered_tiles = generate_tiles_for_plan_with_renderer(
        source_bounds,
        raster_shape,
        config.tile_size,
        &plan,
        config.processing_max_parallelism,
        render_tile,
    )
    .await?;
    let tiles = non_empty_tiles(rendered_tiles);
    let coverage_bounds =
        coverage_bounds_for_tiles(&tiles, config.tile_max_native_zoom, generation_bounds)?;

    let manifest = TileManifest::from_config(
        config,
        TileManifestInput {
            tile_set_id,
            dataset_date,
            generated_at,
            processor_version,
            bounds: coverage_bounds,
            tile_count: tiles.len() as u32,
            source_granules,
            coverage: None,
        },
    )?;

    Ok(GeneratedTileSet {
        plan,
        tiles,
        manifest,
    })
}

fn non_empty_tiles(tiles: Vec<RenderedTile>) -> Vec<RenderedTile> {
    tiles
        .into_iter()
        .filter(RenderedTile::has_renderable_evidence)
        .collect()
}

fn coverage_bounds_for_tiles(
    tiles: &[RenderedTile],
    max_native_zoom: u8,
    generation_bounds: GeographicBounds,
) -> Result<GeographicBounds, GenerateError> {
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

    bounds_for_tiles(coords)?.ok_or(GenerateError::NoRenderableTiles { generation_bounds })
}

/// Renders every coordinate in a tile-generation plan.
///
/// Tile ranges are inclusive. For each planned coordinate, the function maps
/// the tile bounds into the source raster before invoking the renderer.
async fn generate_tiles_for_plan_with_renderer<RenderTile, RenderFuture>(
    source_bounds: GeographicBounds,
    raster_shape: RasterShape,
    tile_size: u16,
    plan: &TileGenerationPlan,
    max_parallelism: usize,
    render_tile: RenderTile,
) -> Result<Vec<RenderedTile>, GenerateError>
where
    RenderTile: Fn(TileCoord, TileRasterWindow) -> RenderFuture + Clone + Send + 'static,
    RenderFuture: Future<Output = Result<RenderedTile, GenerateError>> + Send + 'static,
{
    let jobs = tile_render_jobs(source_bounds, raster_shape, tile_size, plan)?;
    let total = jobs.len();
    let progress = ui::progress("rendering tiles", total);
    let max_parallelism = max_parallelism.max(1);
    let mut join_set = JoinSet::new();
    let mut next_job = 0;
    let mut tiles = Vec::with_capacity(total);

    while next_job < jobs.len() || !join_set.is_empty() {
        while next_job < jobs.len() && join_set.len() < max_parallelism {
            let (index, coord, window) = jobs[next_job];
            let render_tile = render_tile.clone();

            join_set.spawn(async move {
                let rendered_tile = render_tile(coord, window).await?;

                Ok::<_, GenerateError>((index, rendered_tile))
            });

            next_job += 1;
        }

        if let Some(result) = join_set.join_next().await {
            let (index, tile) =
                result.map_err(|source| GenerateError::RenderWorker { source })??;

            progress.set_message(format!(
                "latest z{}/x{}/y{}",
                tile.coord.z, tile.coord.x, tile.coord.y
            ));
            progress.inc(1);
            tiles.push((index, tile));
        }
    }

    tiles.sort_by_key(|(index, _)| *index);
    progress.finish(format!("rendered {total} tile(s)"));

    Ok(tiles.into_iter().map(|(_, tile)| tile).collect())
}

fn tile_render_jobs(
    source_bounds: GeographicBounds,
    raster_shape: RasterShape,
    tile_size: u16,
    plan: &TileGenerationPlan,
) -> Result<Vec<(usize, TileCoord, TileRasterWindow)>, GenerateError> {
    let mut jobs = Vec::with_capacity(plan.tile_count as usize);

    for range in &plan.ranges {
        for x in range.min_x..=range.max_x {
            for y in range.min_y..=range.max_y {
                let coord = TileCoord { z: range.z, x, y };
                let window = raster_window_for_tile(coord, source_bounds, raster_shape, tile_size)?;
                jobs.push((jobs.len(), coord, window));
            }
        }
    }

    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use uuid::Uuid;

    use super::*;
    use crate::manifest::SourceGranule;
    use crate::tiles::{tile_bounds, TileRange};

    fn test_config() -> AppConfig {
        crate::config::AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some("test-key".to_owned()),
            "TILE_MIN_ZOOM" => Some("3".to_owned()),
            "TILE_MAX_NATIVE_ZOOM" => Some("4".to_owned()),
            "TILE_BOUNDS" => Some("-90,30,-80,40".to_owned()),
            "TILE_SIZE" => Some("2".to_owned()),
            _ => None,
        })
        .unwrap()
    }

    fn test_config_for(bounds: &str, min_zoom: u8, max_native_zoom: u8) -> AppConfig {
        crate::config::AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some("test-key".to_owned()),
            "TILE_MIN_ZOOM" => Some(min_zoom.to_string()),
            "TILE_MAX_NATIVE_ZOOM" => Some(max_native_zoom.to_string()),
            "TILE_BOUNDS" => Some(bounds.to_owned()),
            "TILE_SIZE" => Some("2".to_owned()),
            _ => None,
        })
        .unwrap()
    }

    #[tokio::test]
    async fn generates_real_tile_plan_with_renderer_callback() {
        let source_bounds = GeographicBounds {
            west: -180.0,
            south: -85.0,
            east: 180.0,
            north: 85.0,
        };
        let raster_shape = RasterShape {
            width: 1000,
            height: 1000,
        };
        let plan = TileGenerationPlan {
            ranges: vec![TileRange {
                z: 3,
                min_x: 2,
                max_x: 2,
                min_y: 3,
                max_y: 3,
            }],
            tile_count: 1,
        };

        let rendered_windows = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let tiles =
            generate_tiles_for_plan_with_renderer(source_bounds, raster_shape, 2, &plan, 1, {
                let rendered_windows = std::sync::Arc::clone(&rendered_windows);

                move |coord, window| {
                    let rendered_windows = std::sync::Arc::clone(&rendered_windows);

                    async move {
                        rendered_windows.lock().unwrap().push((coord, window));

                        Ok(RenderedTile {
                            coord,
                            png_bytes: b"fake-png".to_vec(),
                            renderable_pixel_count: 1,
                        })
                    }
                }
            })
            .await
            .unwrap();

        let rendered_windows = rendered_windows.lock().unwrap();
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].coord, TileCoord { z: 3, x: 2, y: 3 });
        assert_eq!(tiles[0].png_bytes, b"fake-png");
        assert_eq!(rendered_windows.len(), 1);
        assert_eq!(rendered_windows[0].0, TileCoord { z: 3, x: 2, y: 3 });
        assert!(rendered_windows[0].1.source.width > 0);
        assert!(rendered_windows[0].1.source.height > 0);
        assert!(rendered_windows[0].1.target.width > 0);
        assert!(rendered_windows[0].1.target.height > 0);
    }

    #[tokio::test]
    async fn builds_generated_tile_set_with_manifest() {
        let config = test_config();
        let source_bounds = GeographicBounds::from(config.tile_bounds);
        let raster_shape = RasterShape {
            width: 1000,
            height: 1000,
        };

        let tile_set = generate_tile_set_with_renderer(
            &config,
            TileSetBuildRequest {
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-a1b2c3d4".to_owned(),
                dataset_date: chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                generated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
                processor_version: "processing-svc:test-sha".to_owned(),
                generation_bounds: source_bounds,
                source_bounds,
                raster_shape,
                source_granules: Vec::new(),
            },
            |coord, _window| async move {
                Ok(RenderedTile {
                    coord,
                    png_bytes: b"fake-png".to_vec(),
                    renderable_pixel_count: 1,
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(tile_set.tiles.len(), tile_set.plan.tile_count as usize);
        assert_eq!(tile_set.manifest.tile_count, tile_set.tiles.len() as u32);
        assert_eq!(
            tile_set.manifest.tile_set_id,
            "2026-05-21-radiance-dark-sky-v1-a1b2c3d4"
        );
        assert_eq!(tile_set.manifest.tile_size, config.tile_size);
        assert_eq!(tile_set.manifest.checksums.manifest_sha256.len(), 64);
    }

    #[tokio::test]
    async fn manifest_uses_non_empty_max_native_tile_coverage() {
        let config = test_config();
        let source_bounds = GeographicBounds::from(config.tile_bounds);
        let raster_shape = RasterShape {
            width: 1000,
            height: 1000,
        };
        let plan = plan_tile_generation_for_bounds(
            source_bounds,
            config.tile_min_zoom,
            config.tile_max_native_zoom,
        )
        .unwrap();
        let native_range = plan.ranges.last().unwrap();
        let evidence_coord = TileCoord {
            z: native_range.z,
            x: native_range.min_x,
            y: native_range.min_y,
        };

        let tile_set = generate_tile_set_with_renderer(
            &config,
            TileSetBuildRequest {
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-a1b2c3d4".to_owned(),
                dataset_date: chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                generated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
                processor_version: "processing-svc:test-sha".to_owned(),
                generation_bounds: source_bounds,
                source_bounds,
                raster_shape,
                source_granules: Vec::new(),
            },
            move |coord, _window| async move {
                Ok(RenderedTile {
                    coord,
                    png_bytes: b"fake-png".to_vec(),
                    renderable_pixel_count: u32::from(coord == evidence_coord),
                })
            },
        )
        .await
        .unwrap();

        let expected_bounds = tile_bounds(evidence_coord).unwrap();

        assert_eq!(tile_set.tiles.len(), 1);
        assert_eq!(tile_set.tiles[0].coord, evidence_coord);
        assert_eq!(tile_set.manifest.tile_count, 1);
        assert_eq!(tile_set.manifest.bounds, expected_bounds.into());
    }

    #[tokio::test]
    async fn zero_evidence_tile_set_is_not_manifested() {
        let config = test_config();
        let source_bounds = GeographicBounds::from(config.tile_bounds);
        let raster_shape = RasterShape {
            width: 1000,
            height: 1000,
        };

        let error = generate_tile_set_with_renderer(
            &config,
            TileSetBuildRequest {
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-a1b2c3d4".to_owned(),
                dataset_date: chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                generated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
                processor_version: "processing-svc:test-sha".to_owned(),
                generation_bounds: source_bounds,
                source_bounds,
                raster_shape,
                source_granules: Vec::new(),
            },
            |coord, _window| async move {
                Ok(RenderedTile {
                    coord,
                    png_bytes: b"fake-png".to_vec(),
                    renderable_pixel_count: 0,
                })
            },
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            GenerateError::NoRenderableTiles {
                generation_bounds
            } if generation_bounds == source_bounds
        ));
    }

    #[tokio::test]
    async fn builds_generated_tile_sets_for_product_bounds_and_zoom_smoke_matrix() {
        struct Case {
            product: &'static str,
            tile_set_id: &'static str,
            bounds: &'static str,
            min_zoom: u8,
            max_native_zoom: u8,
        }

        let cases = [
            Case {
                product: "VNP46A2",
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-vnp46a2",
                bounds: "-90,30,-80,40",
                min_zoom: 3,
                max_native_zoom: 3,
            },
            Case {
                product: "VJ146A2",
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-vj146a2",
                bounds: "-85,32,-84,33",
                min_zoom: 6,
                max_native_zoom: 7,
            },
            Case {
                product: "VNP46A3",
                tile_set_id: "2026-05-01-radiance-dark-sky-v1-vnp46a3",
                bounds: "-125,24,-66,50",
                min_zoom: 3,
                max_native_zoom: 4,
            },
        ];

        for case in cases {
            let config = test_config_for(case.bounds, case.min_zoom, case.max_native_zoom);
            let source_bounds = GeographicBounds::from(config.tile_bounds);
            let raster_shape = RasterShape {
                width: 1000,
                height: 1000,
            };

            let tile_set = generate_tile_set_with_renderer(
                &config,
                TileSetBuildRequest {
                    tile_set_id: case.tile_set_id.to_owned(),
                    dataset_date: chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                    generated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
                    processor_version: "processing-svc:test-sha".to_owned(),
                    generation_bounds: source_bounds,
                    source_bounds,
                    raster_shape,
                    source_granules: vec![SourceGranule {
                        ingest_id: Uuid::parse_str("00000000-0000-0000-0000-00000000f001").unwrap(),
                        product: case.product.to_owned(),
                        blob_path: format!("{}/2026-05-21/h11v06.h5", case.product),
                    }],
                },
                |coord, _window| async move {
                    Ok(RenderedTile {
                        coord,
                        png_bytes: b"\x89PNG\r\n\x1a\nfixture".to_vec(),
                        renderable_pixel_count: 1,
                    })
                },
            )
            .await
            .unwrap();

            assert_eq!(
                tile_set.plan.ranges.len(),
                usize::from(case.max_native_zoom - case.min_zoom + 1),
                "{}",
                case.product
            );
            assert_eq!(tile_set.tiles.len(), tile_set.plan.tile_count as usize);
            assert_eq!(tile_set.manifest.tile_count, tile_set.plan.tile_count);
            assert_eq!(tile_set.manifest.tile_set_id, case.tile_set_id);
            assert_eq!(tile_set.manifest.min_zoom, case.min_zoom);
            assert_eq!(tile_set.manifest.max_native_zoom, case.max_native_zoom);
            assert_eq!(tile_set.manifest.source_granules[0].product, case.product);
            assert!(tile_set
                .tiles
                .iter()
                .all(|tile| tile.png_bytes.starts_with(b"\x89PNG\r\n\x1a\n")));
            assert!(tile_set
                .manifest
                .tile_url_template
                .contains(case.tile_set_id));
        }
    }

    #[tokio::test]
    async fn excludes_tiles_that_only_touch_source_boundary() {
        let config = test_config_for("-100,20,-90,30", 3, 3);
        let source_bounds = GeographicBounds::from(config.tile_bounds);
        let raster_shape = RasterShape {
            width: 1000,
            height: 1000,
        };

        let tile_set = generate_tile_set_with_renderer(
            &config,
            TileSetBuildRequest {
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-boundary".to_owned(),
                dataset_date: chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                generated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
                processor_version: "processing-svc:test-sha".to_owned(),
                generation_bounds: source_bounds,
                source_bounds,
                raster_shape,
                source_granules: Vec::new(),
            },
            |coord, _window| async move {
                Ok(RenderedTile {
                    coord,
                    png_bytes: b"fake-png".to_vec(),
                    renderable_pixel_count: 1,
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(tile_set.plan.tile_count, 1);
        assert_eq!(tile_set.tiles.len(), 1);
        assert_eq!(tile_set.tiles[0].coord, TileCoord { z: 3, x: 1, y: 3 });
    }

    #[tokio::test]
    async fn clips_generation_plan_to_source_bounds() {
        let mut config = test_config();
        config.tile_max_native_zoom = 8;
        let source_bounds = GeographicBounds {
            west: -85.0,
            south: 32.0,
            east: -84.0,
            north: 33.0,
        };
        let raster_shape = RasterShape {
            width: 1000,
            height: 1000,
        };

        let tile_set = generate_tile_set_with_renderer(
            &config,
            TileSetBuildRequest {
                tile_set_id: "2026-05-21-radiance-dark-sky-v1-a1b2c3d4".to_owned(),
                dataset_date: chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
                generated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
                processor_version: "processing-svc:test-sha".to_owned(),
                generation_bounds: source_bounds,
                source_bounds,
                raster_shape,
                source_granules: Vec::new(),
            },
            |coord, _window| async move {
                Ok(RenderedTile {
                    coord,
                    png_bytes: b"fake-png".to_vec(),
                    renderable_pixel_count: 1,
                })
            },
        )
        .await
        .unwrap();

        assert!(tile_set.manifest.bounds.west <= source_bounds.west);
        assert!(tile_set.manifest.bounds.south <= source_bounds.south);
        assert!(tile_set.manifest.bounds.east >= source_bounds.east);
        assert!(tile_set.manifest.bounds.north >= source_bounds.north);
        assert!(tile_set.plan.tile_count > 0);
        assert!(tile_set.plan.tile_count < plan_tile_count_for_config_bounds(&config));
    }

    fn plan_tile_count_for_config_bounds(config: &AppConfig) -> u32 {
        plan_tile_generation_for_bounds(
            GeographicBounds::from(config.tile_bounds),
            config.tile_min_zoom,
            config.tile_max_native_zoom,
        )
        .unwrap()
        .tile_count
    }

    #[tokio::test]
    async fn bounds_renderer_parallelism_and_preserves_plan_order() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        let source_bounds = GeographicBounds {
            west: -180.0,
            south: -85.0,
            east: 180.0,
            north: 85.0,
        };
        let raster_shape = RasterShape {
            width: 1000,
            height: 1000,
        };
        let plan = TileGenerationPlan {
            ranges: vec![TileRange {
                z: 3,
                min_x: 2,
                max_x: 3,
                min_y: 3,
                max_y: 4,
            }],
            tile_count: 4,
        };
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let tiles =
            generate_tiles_for_plan_with_renderer(source_bounds, raster_shape, 2, &plan, 2, {
                let in_flight = Arc::clone(&in_flight);
                let max_in_flight = Arc::clone(&max_in_flight);

                move |coord, _window| {
                    let in_flight = Arc::clone(&in_flight);
                    let max_in_flight = Arc::clone(&max_in_flight);

                    async move {
                        let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        max_in_flight.fetch_max(current, Ordering::SeqCst);
                        tokio::task::yield_now().await;
                        in_flight.fetch_sub(1, Ordering::SeqCst);

                        Ok(RenderedTile {
                            coord,
                            png_bytes: vec![coord.x as u8, coord.y as u8],
                            renderable_pixel_count: 1,
                        })
                    }
                }
            })
            .await
            .unwrap();

        assert!(max_in_flight.load(Ordering::SeqCst) <= 2);
        assert_eq!(
            tiles.iter().map(|tile| tile.coord).collect::<Vec<_>>(),
            vec![
                TileCoord { z: 3, x: 2, y: 3 },
                TileCoord { z: 3, x: 2, y: 4 },
                TileCoord { z: 3, x: 3, y: 3 },
                TileCoord { z: 3, x: 3, y: 4 },
            ]
        );
    }
}
