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

pub fn tile_range_for_bounds(bounds: GeographicBounds, z: u8) -> Result<TileRange, TileMathError> {
    validate_bounds(bounds)?;
    let tiles_per_axis = tiles_per_axis(z)?;

    let min_x = longitude_to_tile_x(bounds.west, tiles_per_axis);
    let max_x = longitude_to_tile_x(bounds.east, tiles_per_axis);
    let min_y = latitude_to_tile_y(bounds.north, tiles_per_axis);
    let max_y = latitude_to_tile_y(bounds.south, tiles_per_axis);

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
    let normalized = ((longitude + 180.0) / 360.0).clamp(0.0, 1.0);
    ((normalized * f64::from(tiles_per_axis)).floor() as u32).min(tiles_per_axis - 1)
}

fn latitude_to_tile_y(latitude: f64, tiles_per_axis: u32) -> u32 {
    let latitude = latitude.clamp(-WEB_MERCATOR_LATITUDE_LIMIT, WEB_MERCATOR_LATITUDE_LIMIT);
    let latitude_radians = latitude.to_radians();
    let normalized = (1.0
        - (latitude_radians.tan() + (1.0 / latitude_radians.cos())).ln() / std::f64::consts::PI)
        / 2.0;

    ((normalized * f64::from(tiles_per_axis)).floor() as u32).min(tiles_per_axis - 1)
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
