use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_SUPPORTED_ZOOM: u8 = 22;
pub const WEB_MERCATOR_LATITUDE_LIMIT: f64 = 85.051_128_78;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileCoord {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeographicBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileRange {
    pub z: u8,
    pub min_x: u32,
    pub max_x: u32,
    pub min_y: u32,
    pub max_y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ViirsTileCoord {
    pub tile_h: i16,
    pub tile_v: i16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViirsCoverageSummary {
    pub expected_tile_count: usize,
    pub present_tile_count: usize,
    pub coverage_fraction: f64,
    pub complete: bool,
    pub expected_tiles: Vec<ViirsTileCoord>,
    pub present_tiles: Vec<ViirsTileCoord>,
    pub missing_tiles: Vec<ViirsTileCoord>,
    pub missing_columns: Vec<i16>,
    pub missing_rows: Vec<i16>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TileMathError {
    #[error("invalid geographic bounds")]
    InvalidBounds,

    #[error("invalid VIIRS tile h{tile_h:02}v{tile_v:02}")]
    InvalidViirsTile { tile_h: i16, tile_v: i16 },

    #[error("invalid tile set id")]
    InvalidTileSetId,

    #[error("tile x {x} is outside zoom {z} range 0..={max}")]
    XOutOfRange { z: u8, x: u32, max: u32 },

    #[error("tile y {y} is outside zoom {z} range 0..={max}")]
    YOutOfRange { z: u8, y: u32, max: u32 },

    #[error("tile zoom {z} exceeds supported maximum")]
    ZoomTooLarge { z: u8 },
}

pub fn validate_tile_coord(coord: TileCoord) -> Result<(), TileMathError> {
    let tiles_per_axis = tiles_per_axis(coord.z)?;
    let max = tiles_per_axis - 1;

    if coord.x > max {
        return Err(TileMathError::XOutOfRange {
            z: coord.z,
            x: coord.x,
            max,
        });
    }

    if coord.y > max {
        return Err(TileMathError::YOutOfRange {
            z: coord.z,
            y: coord.y,
            max,
        });
    }

    Ok(())
}

pub fn tile_bounds(coord: TileCoord) -> Result<GeographicBounds, TileMathError> {
    validate_tile_coord(coord)?;

    let tiles_per_axis = f64::from(tiles_per_axis(coord.z)?);
    let west = f64::from(coord.x) / tiles_per_axis * 360.0 - 180.0;
    let east = f64::from(coord.x + 1) / tiles_per_axis * 360.0 - 180.0;
    let north = tile_y_to_latitude(coord.y, tiles_per_axis);
    let south = tile_y_to_latitude(coord.y + 1, tiles_per_axis);

    Ok(GeographicBounds {
        west,
        south,
        east,
        north,
    })
}

pub fn bounds_for_tiles<I>(coords: I) -> Result<Option<GeographicBounds>, TileMathError>
where
    I: IntoIterator<Item = TileCoord>,
{
    let mut union: Option<GeographicBounds> = None;

    for coord in coords {
        let bounds = tile_bounds(coord)?;
        union = Some(match union {
            Some(existing) => GeographicBounds {
                west: existing.west.min(bounds.west),
                south: existing.south.min(bounds.south),
                east: existing.east.max(bounds.east),
                north: existing.north.max(bounds.north),
            },
            None => bounds,
        });
    }

    Ok(union)
}

pub fn clip_bounds(
    bounds: GeographicBounds,
    clip_to: GeographicBounds,
) -> Option<GeographicBounds> {
    let clipped = GeographicBounds {
        west: bounds.west.max(clip_to.west),
        south: bounds.south.max(clip_to.south),
        east: bounds.east.min(clip_to.east),
        north: bounds.north.min(clip_to.north),
    };

    (clipped.west < clipped.east && clipped.south < clipped.north).then_some(clipped)
}

pub fn viirs_tile_bounds(tile_h: i16, tile_v: i16) -> Result<GeographicBounds, TileMathError> {
    if !(0..=35).contains(&tile_h) || !(0..=17).contains(&tile_v) {
        return Err(TileMathError::InvalidViirsTile { tile_h, tile_v });
    }

    let west = -180.0 + f64::from(tile_h) * 10.0;
    let east = west + 10.0;
    let raw_north = 90.0 - f64::from(tile_v) * 10.0;
    let raw_south = raw_north - 10.0;

    Ok(GeographicBounds {
        west,
        south: raw_south.max(-WEB_MERCATOR_LATITUDE_LIMIT),
        east,
        north: raw_north.min(WEB_MERCATOR_LATITUDE_LIMIT),
    })
}

pub fn viirs_tile_overlaps_bounds(
    tile_h: i16,
    tile_v: i16,
    bounds: GeographicBounds,
) -> Result<bool, TileMathError> {
    validate_geographic_bounds(bounds)?;
    let tile_bounds = viirs_tile_bounds(tile_h, tile_v)?;

    Ok(clip_bounds(tile_bounds, bounds).is_some())
}

pub fn viirs_tiles_for_bounds(
    bounds: GeographicBounds,
) -> Result<Vec<ViirsTileCoord>, TileMathError> {
    validate_geographic_bounds(bounds)?;

    let mut tiles = Vec::new();
    for tile_h in 0..=35 {
        for tile_v in 0..=17 {
            if viirs_tile_overlaps_bounds(tile_h, tile_v, bounds)? {
                tiles.push(ViirsTileCoord { tile_h, tile_v });
            }
        }
    }

    Ok(tiles)
}

pub fn summarize_viirs_coverage<I, J>(expected_tiles: I, present_tiles: J) -> ViirsCoverageSummary
where
    I: IntoIterator<Item = ViirsTileCoord>,
    J: IntoIterator<Item = ViirsTileCoord>,
{
    let expected = expected_tiles.into_iter().collect::<BTreeSet<_>>();
    let present = present_tiles
        .into_iter()
        .filter(|coord| expected.contains(coord))
        .collect::<BTreeSet<_>>();
    let missing = expected.difference(&present).copied().collect::<Vec<_>>();
    let missing_set = missing.iter().copied().collect::<BTreeSet<_>>();

    let expected_by_column = group_by_column(&expected);
    let expected_by_row = group_by_row(&expected);
    let missing_columns = expected_by_column
        .iter()
        .filter_map(|(tile_h, column_tiles)| {
            column_tiles
                .iter()
                .all(|coord| missing_set.contains(coord))
                .then_some(*tile_h)
        })
        .collect::<Vec<_>>();
    let missing_rows = expected_by_row
        .iter()
        .filter_map(|(tile_v, row_tiles)| {
            row_tiles
                .iter()
                .all(|coord| missing_set.contains(coord))
                .then_some(*tile_v)
        })
        .collect::<Vec<_>>();

    let expected_tile_count = expected.len();
    let present_tile_count = present.len();
    let coverage_fraction = if expected_tile_count == 0 {
        1.0
    } else {
        present_tile_count as f64 / expected_tile_count as f64
    };

    ViirsCoverageSummary {
        expected_tile_count,
        present_tile_count,
        coverage_fraction,
        complete: missing.is_empty(),
        expected_tiles: expected.into_iter().collect(),
        present_tiles: present.into_iter().collect(),
        missing_tiles: missing,
        missing_columns,
        missing_rows,
    }
}

fn group_by_column(coords: &BTreeSet<ViirsTileCoord>) -> BTreeMap<i16, Vec<ViirsTileCoord>> {
    let mut grouped = BTreeMap::new();
    for coord in coords {
        grouped
            .entry(coord.tile_h)
            .or_insert_with(Vec::new)
            .push(*coord);
    }

    grouped
}

fn group_by_row(coords: &BTreeSet<ViirsTileCoord>) -> BTreeMap<i16, Vec<ViirsTileCoord>> {
    let mut grouped = BTreeMap::new();
    for coord in coords {
        grouped
            .entry(coord.tile_v)
            .or_insert_with(Vec::new)
            .push(*coord);
    }

    grouped
}

pub fn tile_range_for_bounds(bounds: GeographicBounds, z: u8) -> Result<TileRange, TileMathError> {
    validate_bounds(bounds)?;
    let tiles_per_axis = tiles_per_axis(z)?;

    let min_x = longitude_to_tile_x(bounds.west, tiles_per_axis);
    let max_x = longitude_to_max_tile_x(bounds.east, tiles_per_axis);
    let min_y = latitude_to_tile_y(bounds.north, tiles_per_axis);
    let max_y = latitude_to_max_tile_y(bounds.south, tiles_per_axis);

    Ok(TileRange {
        z,
        min_x: min_x.min(max_x),
        max_x: min_x.max(max_x),
        min_y: min_y.min(max_y),
        max_y: min_y.max(max_y),
    })
}

pub fn tile_blob_path(tile_set_id: &str, coord: TileCoord) -> Result<String, TileMathError> {
    validate_tile_set_id(tile_set_id)?;
    validate_tile_coord(coord)?;

    Ok(format!(
        "tiles/{}/{}/{}/{}.png",
        tile_set_id, coord.z, coord.x, coord.y
    ))
}

pub fn manifest_blob_path(tile_set_id: &str) -> Result<String, TileMathError> {
    validate_tile_set_id(tile_set_id)?;
    Ok(format!("manifests/{tile_set_id}.json"))
}

pub fn latest_manifest_blob_path() -> &'static str {
    "manifests/latest.json"
}

fn tiles_per_axis(z: u8) -> Result<u32, TileMathError> {
    if z > MAX_SUPPORTED_ZOOM {
        return Err(TileMathError::ZoomTooLarge { z });
    }

    Ok(1_u32 << z)
}

fn tile_y_to_latitude(y: u32, tiles_per_axis: f64) -> f64 {
    let mercator = std::f64::consts::PI * (1.0 - 2.0 * f64::from(y) / tiles_per_axis);
    mercator.sinh().atan().to_degrees()
}

fn longitude_to_tile_x(longitude: f64, tiles_per_axis: u32) -> u32 {
    scaled_longitude(longitude, tiles_per_axis)
        .floor()
        .min(f64::from(tiles_per_axis - 1)) as u32
}

fn longitude_to_max_tile_x(longitude: f64, tiles_per_axis: u32) -> u32 {
    exclusive_upper_tile_index(scaled_longitude(longitude, tiles_per_axis), tiles_per_axis)
}

fn latitude_to_tile_y(latitude: f64, tiles_per_axis: u32) -> u32 {
    scaled_latitude(latitude, tiles_per_axis)
        .floor()
        .min(f64::from(tiles_per_axis - 1)) as u32
}

fn latitude_to_max_tile_y(latitude: f64, tiles_per_axis: u32) -> u32 {
    exclusive_upper_tile_index(scaled_latitude(latitude, tiles_per_axis), tiles_per_axis)
}

fn scaled_longitude(longitude: f64, tiles_per_axis: u32) -> f64 {
    let normalized = ((longitude + 180.0) / 360.0).clamp(0.0, 1.0);

    normalized * f64::from(tiles_per_axis)
}

fn scaled_latitude(latitude: f64, tiles_per_axis: u32) -> f64 {
    let latitude = latitude.clamp(-WEB_MERCATOR_LATITUDE_LIMIT, WEB_MERCATOR_LATITUDE_LIMIT);
    let latitude_radians = latitude.to_radians();
    let normalized = (1.0
        - (latitude_radians.tan() + (1.0 / latitude_radians.cos())).ln() / std::f64::consts::PI)
        / 2.0;

    normalized * f64::from(tiles_per_axis)
}

fn exclusive_upper_tile_index(scaled_coordinate: f64, tiles_per_axis: u32) -> u32 {
    if scaled_coordinate <= 0.0 {
        0
    } else {
        ((scaled_coordinate.ceil() as u32).saturating_sub(1)).min(tiles_per_axis - 1)
    }
}

fn validate_bounds(bounds: GeographicBounds) -> Result<(), TileMathError> {
    let valid_longitude =
        (-180.0..=180.0).contains(&bounds.west) && (-180.0..=180.0).contains(&bounds.east);
    let valid_latitude = (-WEB_MERCATOR_LATITUDE_LIMIT..=WEB_MERCATOR_LATITUDE_LIMIT)
        .contains(&bounds.south)
        && (-WEB_MERCATOR_LATITUDE_LIMIT..=WEB_MERCATOR_LATITUDE_LIMIT).contains(&bounds.north);

    if valid_longitude && valid_latitude && bounds.west < bounds.east && bounds.south < bounds.north
    {
        Ok(())
    } else {
        Err(TileMathError::InvalidBounds)
    }
}

fn validate_geographic_bounds(bounds: GeographicBounds) -> Result<(), TileMathError> {
    let valid_longitude =
        (-180.0..=180.0).contains(&bounds.west) && (-180.0..=180.0).contains(&bounds.east);
    let valid_latitude =
        (-90.0..=90.0).contains(&bounds.south) && (-90.0..=90.0).contains(&bounds.north);

    if valid_longitude && valid_latitude && bounds.west < bounds.east && bounds.south < bounds.north
    {
        Ok(())
    } else {
        Err(TileMathError::InvalidBounds)
    }
}

fn validate_tile_set_id(tile_set_id: &str) -> Result<(), TileMathError> {
    let valid = !tile_set_id.trim().is_empty()
        && !tile_set_id.contains('/')
        && !tile_set_id.contains('\\')
        && !tile_set_id.contains("..")
        && !tile_set_id.starts_with("processed-tiles");

    valid.then_some(()).ok_or(TileMathError::InvalidTileSetId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_tile_bounds_at_zoom_edge() {
        assert!(validate_tile_coord(TileCoord { z: 3, x: 7, y: 7 }).is_ok());
        assert!(matches!(
            validate_tile_coord(TileCoord { z: 3, x: 8, y: 7 }),
            Err(TileMathError::XOutOfRange { .. })
        ));
    }

    #[test]
    fn calculates_world_tile_bounds() {
        let bounds = tile_bounds(TileCoord { z: 0, x: 0, y: 0 }).unwrap();

        assert_eq!(bounds.west, -180.0);
        assert_eq!(bounds.east, 180.0);
        assert!(bounds.north > 85.0);
        assert!(bounds.south < -85.0);
    }

    #[test]
    fn returns_none_for_empty_tile_bounds_union() {
        assert_eq!(bounds_for_tiles([]).unwrap(), None);
    }

    #[test]
    fn unions_tile_bounds() {
        let bounds = bounds_for_tiles([
            TileCoord { z: 2, x: 1, y: 1 },
            TileCoord { z: 2, x: 2, y: 2 },
        ])
        .unwrap()
        .unwrap();
        let first = tile_bounds(TileCoord { z: 2, x: 1, y: 1 }).unwrap();
        let second = tile_bounds(TileCoord { z: 2, x: 2, y: 2 }).unwrap();

        assert_eq!(bounds.west, first.west);
        assert_eq!(bounds.north, first.north);
        assert_eq!(bounds.east, second.east);
        assert_eq!(bounds.south, second.south);
    }

    #[test]
    fn clips_intersecting_bounds() {
        let clipped = clip_bounds(
            GeographicBounds {
                west: -100.0,
                south: 20.0,
                east: -80.0,
                north: 40.0,
            },
            GeographicBounds {
                west: -90.0,
                south: 30.0,
                east: -70.0,
                north: 50.0,
            },
        )
        .unwrap();

        assert_eq!(clipped.west, -90.0);
        assert_eq!(clipped.south, 30.0);
        assert_eq!(clipped.east, -80.0);
        assert_eq!(clipped.north, 40.0);
    }

    #[test]
    fn maps_viirs_tile_coordinates_to_geographic_bounds() {
        let bounds = viirs_tile_bounds(5, 6).unwrap();

        assert_eq!(
            bounds,
            GeographicBounds {
                west: -130.0,
                south: 20.0,
                east: -120.0,
                north: 30.0,
            }
        );
    }

    #[test]
    fn detects_viirs_tile_overlap_against_bounds() {
        let conus = GeographicBounds {
            west: -125.0,
            south: 24.0,
            east: -66.0,
            north: 50.0,
        };

        assert!(viirs_tile_overlaps_bounds(5, 6, conus).unwrap());
        assert!(viirs_tile_overlaps_bounds(8, 4, conus).unwrap());
        assert!(!viirs_tile_overlaps_bounds(6, 3, conus).unwrap());
    }

    #[test]
    fn computes_expected_viirs_tiles_for_conus_bounds() {
        let tiles = viirs_tiles_for_bounds(GeographicBounds {
            west: -125.0,
            south: 24.0,
            east: -66.0,
            north: 50.0,
        })
        .unwrap();

        let expected = (5..=11)
            .flat_map(|tile_h| (4..=6).map(move |tile_v| ViirsTileCoord { tile_h, tile_v }))
            .collect::<Vec<_>>();

        assert_eq!(tiles, expected);
    }

    #[test]
    fn summarizes_missing_viirs_columns_and_tiles() {
        let expected = (5..=11)
            .flat_map(|tile_h| (4..=6).map(move |tile_v| ViirsTileCoord { tile_h, tile_v }))
            .collect::<Vec<_>>();
        let present = [
            ViirsTileCoord {
                tile_h: 6,
                tile_v: 6,
            },
            ViirsTileCoord {
                tile_h: 7,
                tile_v: 4,
            },
            ViirsTileCoord {
                tile_h: 7,
                tile_v: 5,
            },
            ViirsTileCoord {
                tile_h: 9,
                tile_v: 4,
            },
            ViirsTileCoord {
                tile_h: 9,
                tile_v: 5,
            },
            ViirsTileCoord {
                tile_h: 9,
                tile_v: 6,
            },
            ViirsTileCoord {
                tile_h: 10,
                tile_v: 5,
            },
            ViirsTileCoord {
                tile_h: 11,
                tile_v: 6,
            },
            ViirsTileCoord {
                tile_h: 11,
                tile_v: 6,
            },
            ViirsTileCoord {
                tile_h: 6,
                tile_v: 3,
            },
        ];

        let summary = summarize_viirs_coverage(expected, present);

        assert_eq!(summary.expected_tile_count, 21);
        assert_eq!(summary.present_tile_count, 8);
        assert!(!summary.complete);
        assert_eq!(summary.missing_columns, vec![5, 8]);
        assert!(summary.missing_rows.is_empty());
        assert!(summary.missing_tiles.contains(&ViirsTileCoord {
            tile_h: 5,
            tile_v: 4,
        }));
        assert!(summary.missing_tiles.contains(&ViirsTileCoord {
            tile_h: 8,
            tile_v: 6,
        }));
    }

    #[test]
    fn tile_range_excludes_east_boundary_only_tiles() {
        let range = tile_range_for_bounds(
            GeographicBounds {
                west: -100.0,
                south: 20.0,
                east: -90.0,
                north: 30.0,
            },
            3,
        )
        .unwrap();

        assert_eq!(
            range,
            TileRange {
                z: 3,
                min_x: 1,
                max_x: 1,
                min_y: 3,
                max_y: 3,
            }
        );
    }

    #[test]
    fn tile_range_excludes_south_boundary_only_tiles() {
        let tile = tile_bounds(TileCoord { z: 3, x: 2, y: 3 }).unwrap();
        let range = tile_range_for_bounds(tile, 3).unwrap();

        assert_eq!(
            range,
            TileRange {
                z: 3,
                min_x: 2,
                max_x: 2,
                min_y: 3,
                max_y: 3,
            }
        );
    }

    #[test]
    fn tile_range_keeps_world_east_and_south_edges() {
        let range = tile_range_for_bounds(
            GeographicBounds {
                west: -180.0,
                south: -WEB_MERCATOR_LATITUDE_LIMIT,
                east: 180.0,
                north: WEB_MERCATOR_LATITUDE_LIMIT,
            },
            3,
        )
        .unwrap();

        assert_eq!(
            range,
            TileRange {
                z: 3,
                min_x: 0,
                max_x: 7,
                min_y: 0,
                max_y: 7,
            }
        );
    }

    #[test]
    fn rejects_invalid_viirs_tile_coordinates() {
        assert!(matches!(
            viirs_tile_bounds(36, 6),
            Err(TileMathError::InvalidViirsTile { .. })
        ));
        assert!(matches!(
            viirs_tile_bounds(5, 18),
            Err(TileMathError::InvalidViirsTile { .. })
        ));
    }

    #[test]
    fn builds_container_relative_tile_paths() {
        let path = tile_blob_path(
            "2026-05-21-radiance-dark-sky-v1-a1b2c3d4",
            TileCoord { z: 5, x: 8, y: 12 },
        )
        .unwrap();

        assert_eq!(
            path,
            "tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4/5/8/12.png"
        );
    }

    #[test]
    fn builds_manifest_paths() {
        assert_eq!(
            manifest_blob_path("2026-05-21-radiance-dark-sky-v1-a1b2c3d4").unwrap(),
            "manifests/2026-05-21-radiance-dark-sky-v1-a1b2c3d4.json"
        );
        assert_eq!(latest_manifest_blob_path(), "manifests/latest.json");
    }

    #[test]
    fn rejects_container_prefixed_tile_set_ids() {
        assert!(matches!(
            manifest_blob_path("processed-tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4"),
            Err(TileMathError::InvalidTileSetId)
        ));
    }
}
