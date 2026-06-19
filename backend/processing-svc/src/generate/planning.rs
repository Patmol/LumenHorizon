//! Tile planning from configured bounds and zoom levels.
//!
//! Planning produces inclusive Web Mercator tile ranges for each native zoom
//! level before rendering begins.

#[cfg(test)]
use crate::config::AppConfig;
use crate::tiles::{tile_range_for_bounds, GeographicBounds, TileRange};

use super::{error::GenerateError, types::TileGenerationPlan};

/// Plans tile generation from the service configuration.
#[cfg(test)]
pub fn plan_tile_generation(config: &AppConfig) -> Result<TileGenerationPlan, GenerateError> {
    let bounds = GeographicBounds::from(config.tile_bounds);

    plan_tile_generation_for_bounds(bounds, config.tile_min_zoom, config.tile_max_native_zoom)
}

/// Plans all tile ranges required to cover the requested bounds.
///
/// One inclusive range is produced for each zoom in `min_zoom..=max_native_zoom`.
///
/// # Errors
///
/// Returns an error if bounds cannot be converted to a tile range or if the
/// total tile count exceeds `u32::MAX`.
pub fn plan_tile_generation_for_bounds(
    bounds: GeographicBounds,
    min_zoom: u8,
    max_native_zoom: u8,
) -> Result<TileGenerationPlan, GenerateError> {
    let mut ranges = Vec::new();
    let mut tile_count: u64 = 0;

    for z in min_zoom..=max_native_zoom {
        let range = tile_range_for_bounds(bounds, z)?;
        tile_count += tile_range_tile_count(range);
        ranges.push(range);
    }

    let tile_count =
        u32::try_from(tile_count).map_err(|_| GenerateError::TileCountOverflow { tile_count })?;

    Ok(TileGenerationPlan { ranges, tile_count })
}

/// Counts tiles in an inclusive tile range.
pub fn tile_range_tile_count(range: TileRange) -> u64 {
    let width = u64::from(range.max_x - range.min_x + 1);
    let height = u64::from(range.max_y - range.min_y + 1);

    width * height
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn counts_tiles_in_inclusive_range() {
        assert_eq!(tile_range_tile_count(sample_range()), 4);
    }

    #[test]
    fn plans_tile_ranges_for_bounds_and_zoom_span() {
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

        assert_eq!(plan.ranges.len(), 2);
        assert_eq!(plan.ranges[0].z, 3);
        assert_eq!(plan.ranges[1].z, 4);
        assert_eq!(
            plan.tile_count,
            plan.ranges
                .iter()
                .map(|range| tile_range_tile_count(*range) as u32)
                .sum::<u32>()
        );
    }

    #[test]
    fn plans_representative_tile_smoke_matrix() {
        let cases = [
            (
                "daily continental single zoom",
                GeographicBounds {
                    west: -125.0,
                    south: 24.0,
                    east: -66.0,
                    north: 50.0,
                },
                3,
                3,
            ),
            (
                "daily clipped regional high zoom",
                GeographicBounds {
                    west: -85.0,
                    south: 32.0,
                    east: -84.0,
                    north: 33.0,
                },
                6,
                8,
            ),
            (
                "monthly broad low zoom",
                GeographicBounds {
                    west: -140.0,
                    south: 10.0,
                    east: -60.0,
                    north: 60.0,
                },
                2,
                4,
            ),
        ];

        for (name, bounds, min_zoom, max_native_zoom) in cases {
            let plan = plan_tile_generation_for_bounds(bounds, min_zoom, max_native_zoom).unwrap();

            assert_eq!(
                plan.ranges.len(),
                usize::from(max_native_zoom - min_zoom + 1),
                "{name}"
            );
            assert_eq!(plan.ranges.first().unwrap().z, min_zoom, "{name}");
            assert_eq!(plan.ranges.last().unwrap().z, max_native_zoom, "{name}");
            assert!(plan.tile_count > 0, "{name}");
            assert_eq!(
                plan.tile_count,
                plan.ranges
                    .iter()
                    .map(|range| tile_range_tile_count(*range) as u32)
                    .sum::<u32>(),
                "{name}"
            );
            assert!(
                plan.ranges
                    .iter()
                    .all(|range| range.min_x <= range.max_x && range.min_y <= range.max_y),
                "{name}"
            );
        }
    }

    #[test]
    fn plans_tile_generation_from_config_bounds() {
        let config = crate::config::AppConfig::from_lookup(|name| match name {
            "DATABASE_URL" => Some("postgres://localhost/lumenhorizon".to_owned()),
            "AZURE_STORAGE_ACCOUNT" => Some("devstoreaccount1".to_owned()),
            "AZURE_STORAGE_ACCESS_KEY" => Some("test-key".to_owned()),
            "TILE_MIN_ZOOM" => Some("3".to_owned()),
            "TILE_MAX_NATIVE_ZOOM" => Some("4".to_owned()),
            "TILE_BOUNDS" => Some("-90,30,-80,40".to_owned()),
            _ => None,
        })
        .unwrap();

        let plan = plan_tile_generation(&config).unwrap();

        assert_eq!(plan.ranges.len(), 2);
        assert_eq!(plan.ranges[0].z, 3);
        assert_eq!(plan.ranges[1].z, 4);
        assert!(plan.tile_count > 0);
    }
}
