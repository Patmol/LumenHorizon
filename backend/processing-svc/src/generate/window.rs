//! Raster-window mapping for generated tiles.
//!
//! The functions here translate geographic tile bounds into HDF raster windows
//! relative to the source granule bounds.

use crate::{
    hdf_cli::{RasterShape, RasterWindow},
    tiles::{clip_bounds, tile_bounds, GeographicBounds, TileCoord},
};

use super::error::GenerateError;

/// Maps a map tile coordinate to the raster window needed from the source granule.
///
/// Tiles partially outside the source bounds are clipped. Tiles with no overlap
/// are rejected so callers do not render empty data.
pub fn raster_window_for_tile(
    coord: TileCoord,
    source_bounds: GeographicBounds,
    raster_shape: RasterShape,
) -> Result<RasterWindow, GenerateError> {
    let bounds = tile_bounds(coord)?;
    let clipped =
        clip_bounds(bounds, source_bounds).ok_or(GenerateError::TileOutsideSourceBounds {
            coord,
            source_bounds,
        })?;

    let source_width = source_bounds.east - source_bounds.west;
    let source_height = source_bounds.north - source_bounds.south;

    // Convert clipped geographic bounds into normalized raster fractions.
    let x_start_fraction = (clipped.west - source_bounds.west) / source_width;
    let x_end_fraction = (clipped.east - source_bounds.west) / source_width;
    let y_start_fraction = (source_bounds.north - clipped.north) / source_height;
    let y_end_fraction = (source_bounds.north - clipped.south) / source_height;

    let x_offset = floor_to_raster_index(x_start_fraction, raster_shape.width);
    let y_offset = floor_to_raster_index(y_start_fraction, raster_shape.height);
    let x_end = ceil_to_raster_index(x_end_fraction, raster_shape.width);
    let y_end = ceil_to_raster_index(y_end_fraction, raster_shape.height);

    Ok(RasterWindow {
        x_offset,
        y_offset,
        width: (x_end.saturating_sub(x_offset)).max(1),
        height: (y_end.saturating_sub(y_offset)).max(1),
    })
}

/// Maps a normalized start fraction to an inclusive raster start index.
fn floor_to_raster_index(fraction: f64, raster_size: usize) -> usize {
    ((fraction.clamp(0.0, 1.0) * raster_size as f64).floor() as usize).min(raster_size)
}

/// Maps a normalized end fraction to an exclusive raster end index.
fn ceil_to_raster_index(fraction: f64, raster_size: usize) -> usize {
    ((fraction.clamp(0.0, 1.0) * raster_size as f64).ceil() as usize).min(raster_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_global_tile_to_raster_window() {
        let window = raster_window_for_tile(
            TileCoord { z: 1, x: 0, y: 0 },
            GeographicBounds {
                west: -180.0,
                south: -85.051_128_78,
                east: 180.0,
                north: 85.051_128_78,
            },
            RasterShape {
                width: 3600,
                height: 1700,
            },
        )
        .unwrap();

        assert_eq!(window.x_offset, 0);
        assert_eq!(window.y_offset, 0);
        assert_eq!(window.width, 1800);
        assert!(window.height > 0);
        assert!(window.height <= 850);
    }

    #[test]
    fn clips_tile_window_to_source_bounds() {
        let window = raster_window_for_tile(
            TileCoord { z: 3, x: 2, y: 3 },
            GeographicBounds {
                west: -90.0,
                south: 30.0,
                east: -80.0,
                north: 40.0,
            },
            RasterShape {
                width: 1000,
                height: 1000,
            },
        )
        .unwrap();

        assert!(window.x_offset < 1000);
        assert!(window.y_offset < 1000);
        assert!(window.width > 0);
        assert!(window.height > 0);
        assert!(window.x_offset + window.width <= 1000);
        assert!(window.y_offset + window.height <= 1000);
    }

    #[test]
    fn rejects_tile_outside_source_bounds() {
        let error = raster_window_for_tile(
            TileCoord { z: 3, x: 0, y: 0 },
            GeographicBounds {
                west: -90.0,
                south: 30.0,
                east: -80.0,
                north: 40.0,
            },
            RasterShape {
                width: 1000,
                height: 1000,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            GenerateError::TileOutsideSourceBounds {
                coord: TileCoord { z: 3, x: 0, y: 0 },
                ..
            }
        ));
    }
}
