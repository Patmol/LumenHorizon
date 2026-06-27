use chrono::{DateTime, NaiveDate, Utc};

use crate::{
    config::AppConfig,
    manifest::{SourceGranule, TileManifest, TileManifestInput},
    publish::RenderedTile,
    render::{render_png_tile, renderable_pixel_count, RenderError, RenderPixel},
    tiles::{GeographicBounds, TileCoord, TileRange},
};

use super::{
    error::GenerateError,
    planning::plan_tile_generation,
    types::{SyntheticTileInput, SyntheticTileSet, TileGenerationPlan},
};

pub fn generate_synthetic_tile_set(
    tile_size: u16,
    plan: &TileGenerationPlan,
) -> Result<Vec<RenderedTile>, GenerateError> {
    let mut tiles = Vec::with_capacity(plan.tile_count as usize);

    for range in &plan.ranges {
        for x in range.min_x..=range.max_x {
            for y in range.min_y..=range.max_y {
                let coord = TileCoord { z: range.z, x, y };
                let pixels = synthetic_pixels_for_coord(tile_size, coord)?;
                let png_bytes = render_png_tile(tile_size, &pixels)
                    .map_err(|source| GenerateError::RenderTile { coord, source })?;
                let renderable_pixel_count = renderable_pixel_count(&pixels);

                tiles.push(RenderedTile {
                    coord,
                    png_bytes,
                    renderable_pixel_count,
                });
            }
        }
    }

    Ok(tiles)
}

pub fn generate_synthetic_tile_set_with_manifest(
    config: &AppConfig,
    tile_set_id: String,
    dataset_date: NaiveDate,
    generated_at: DateTime<Utc>,
    processor_version: String,
    source_granules: Vec<SourceGranule>,
) -> Result<SyntheticTileSet, GenerateError> {
    let plan = plan_tile_generation(config)?;
    let tiles = generate_synthetic_tile_set(config.tile_size, &plan)?;

    let manifest = TileManifest::from_config(
        config,
        TileManifestInput {
            tile_set_id,
            dataset_date,
            generated_at,
            processor_version,
            bounds: GeographicBounds::from(config.tile_bounds),
            tile_count: plan.tile_count,
            source_granules,
            coverage: None,
        },
    )?;

    Ok(SyntheticTileSet {
        plan,
        tiles,
        manifest,
    })
}

fn synthetic_pixels_for_coord(
    tile_size: u16,
    coord: TileCoord,
) -> Result<Vec<RenderPixel>, GenerateError> {
    if tile_size == 0 {
        return Ok(Vec::new());
    }

    let pixel_count = usize::from(tile_size) * usize::from(tile_size);
    let seed = coord.z as u32 + coord.x + coord.y;

    let pixels = (0..pixel_count)
        .map(|index| {
            let index = index as u32;
            let rejected = (seed + index).is_multiple_of(7);
            let nodata = (seed + index).is_multiple_of(11);
            let radiance = if nodata {
                None
            } else {
                Some(((seed + index) % 60) as f32)
            };

            RenderPixel { radiance, rejected }
        })
        .collect();

    Ok(pixels)
}

pub fn generate_synthetic_tiles(
    tile_size: u16,
    range: TileRange,
    inputs: &[SyntheticTileInput],
) -> Result<Vec<RenderedTile>, GenerateError> {
    let mut rendered_tiles = Vec::with_capacity(inputs.len());

    for input in inputs {
        if !tile_range_contains(range, input.coord) {
            return Err(GenerateError::TileOutsideRange {
                coord: input.coord,
                range,
            });
        }

        let png_bytes = render_png_tile(tile_size, &input.pixels).map_err(|source| {
            GenerateError::RenderTile {
                coord: input.coord,
                source,
            }
        })?;

        rendered_tiles.push(RenderedTile {
            coord: input.coord,
            png_bytes,
            renderable_pixel_count: renderable_pixel_count(&input.pixels),
        });
    }

    Ok(rendered_tiles)
}

pub fn tile_range_contains(range: TileRange, coord: TileCoord) -> bool {
    coord.z == range.z
        && coord.x >= range.min_x
        && coord.x <= range.max_x
        && coord.y >= range.min_y
        && coord.y <= range.max_y
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::render::RenderPixel;

    use super::super::planning::plan_tile_generation_for_bounds;

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

    fn sample_pixels() -> Vec<RenderPixel> {
        vec![
            RenderPixel {
                radiance: Some(0.1),
                rejected: false,
            },
            RenderPixel {
                radiance: Some(0.5),
                rejected: false,
            },
            RenderPixel {
                radiance: None,
                rejected: false,
            },
            RenderPixel {
                radiance: Some(50.0),
                rejected: true,
            },
        ]
    }

    fn sample_range() -> TileRange {
        TileRange {
            z: 5,
            min_x: 8,
            max_x: 9,
            min_y: 12,
            max_y: 13,
        }
    }

    #[test]
    fn checks_tile_range_membership() {
        let range = sample_range();

        assert!(tile_range_contains(range, TileCoord { z: 5, x: 8, y: 12 }));
        assert!(tile_range_contains(range, TileCoord { z: 5, x: 9, y: 13 }));
        assert!(!tile_range_contains(range, TileCoord { z: 4, x: 8, y: 12 }));
        assert!(!tile_range_contains(
            range,
            TileCoord { z: 5, x: 10, y: 12 }
        ));
    }

    #[test]
    fn generates_png_tiles_for_synthetic_inputs() {
        let tiles = generate_synthetic_tiles(
            2,
            sample_range(),
            &[SyntheticTileInput {
                coord: TileCoord { z: 5, x: 8, y: 12 },
                pixels: sample_pixels(),
            }],
        )
        .unwrap();

        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].coord, TileCoord { z: 5, x: 8, y: 12 });
        assert!(tiles[0].png_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn rejects_tiles_outside_requested_range() {
        let error = generate_synthetic_tiles(
            2,
            sample_range(),
            &[SyntheticTileInput {
                coord: TileCoord { z: 5, x: 10, y: 12 },
                pixels: sample_pixels(),
            }],
        )
        .unwrap_err();

        assert!(matches!(
            error,
            GenerateError::TileOutsideRange {
                coord: TileCoord { z: 5, x: 10, y: 12 },
                ..
            }
        ));
    }

    #[test]
    fn wraps_render_errors_with_tile_coordinate() {
        let error = generate_synthetic_tiles(
            2,
            sample_range(),
            &[SyntheticTileInput {
                coord: TileCoord { z: 5, x: 8, y: 12 },
                pixels: vec![RenderPixel {
                    radiance: Some(0.1),
                    rejected: false,
                }],
            }],
        )
        .unwrap_err();

        assert!(matches!(
            error,
            GenerateError::RenderTile {
                coord: TileCoord { z: 5, x: 8, y: 12 },
                source: RenderError::PixelCountMismatch { .. },
            }
        ));
    }

    #[test]
    fn generates_synthetic_tile_set_for_planned_ranges() {
        let plan = TileGenerationPlan {
            ranges: vec![sample_range()],
            tile_count: 4,
        };

        let tiles = generate_synthetic_tile_set(2, &plan).unwrap();

        assert_eq!(tiles.len(), 4);
        assert_eq!(tiles[0].coord, TileCoord { z: 5, x: 8, y: 12 });
        assert_eq!(tiles[1].coord, TileCoord { z: 5, x: 8, y: 13 });
        assert_eq!(tiles[2].coord, TileCoord { z: 5, x: 9, y: 12 });
        assert_eq!(tiles[3].coord, TileCoord { z: 5, x: 9, y: 13 });
        assert!(tiles
            .iter()
            .all(|tile| tile.png_bytes.starts_with(b"\x89PNG\r\n\x1a\n")));
    }

    #[test]
    fn synthetic_tile_set_generation_is_deterministic() {
        let plan = TileGenerationPlan {
            ranges: vec![sample_range()],
            tile_count: 4,
        };

        let first = generate_synthetic_tile_set(2, &plan).unwrap();
        let second = generate_synthetic_tile_set(2, &plan).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn synthetic_tile_set_count_matches_plan_count() {
        let plan = plan_tile_generation_for_bounds(
            GeographicBounds {
                west: -90.0,
                south: 30.0,
                east: -80.0,
                north: 40.0,
            },
            3,
            4,
        )
        .unwrap();

        let tiles = generate_synthetic_tile_set(2, &plan).unwrap();

        assert_eq!(tiles.len(), plan.tile_count as usize);
    }

    #[test]
    fn builds_synthetic_tile_set_with_matching_manifest_count() {
        let tile_set = generate_synthetic_tile_set_with_manifest(
            &test_config(),
            "2026-05-21-radiance-dark-sky-v1-a1b2c3d4".to_owned(),
            chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap(),
            chrono::Utc.with_ymd_and_hms(2026, 5, 21, 9, 15, 0).unwrap(),
            "processing-svc:test-sha".to_owned(),
            Vec::new(),
        )
        .unwrap();

        assert_eq!(tile_set.tiles.len(), tile_set.plan.tile_count as usize);
        assert_eq!(tile_set.manifest.tile_count, tile_set.plan.tile_count);
        assert_eq!(
            tile_set.manifest.tile_set_id,
            "2026-05-21-radiance-dark-sky-v1-a1b2c3d4"
        );
        assert_eq!(tile_set.manifest.bounds.west, -90.0);
        assert_eq!(tile_set.manifest.bounds.south, 30.0);
        assert_eq!(tile_set.manifest.bounds.east, -80.0);
        assert_eq!(tile_set.manifest.bounds.north, 40.0);
        assert_eq!(tile_set.manifest.checksums.manifest_sha256.len(), 64);
    }
}
