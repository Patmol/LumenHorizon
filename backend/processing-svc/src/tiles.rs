use crate::config::TileBounds;
pub use shared::slippy_tiles::{
    bounds_for_tiles, clip_bounds, latest_manifest_blob_path, manifest_blob_path,
    summarize_viirs_coverage, tile_blob_path, tile_bounds, tile_range_for_bounds,
    viirs_tile_bounds, viirs_tiles_for_bounds, GeographicBounds, TileCoord, TileMathError,
    TileRange, ViirsCoverageSummary, ViirsTileCoord,
};

#[cfg(test)]
pub use shared::slippy_tiles::validate_tile_coord;

impl From<TileBounds> for GeographicBounds {
    fn from(bounds: TileBounds) -> Self {
        Self {
            west: bounds.west,
            south: bounds.south,
            east: bounds.east,
            north: bounds.north,
        }
    }
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
    fn rejects_container_prefixed_tile_set_ids() {
        assert!(matches!(
            manifest_blob_path("processed-tiles/2026-05-21-radiance-dark-sky-v1-a1b2c3d4"),
            Err(TileMathError::InvalidTileSetId)
        ));
    }
}
