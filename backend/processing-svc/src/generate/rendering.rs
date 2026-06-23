//! Tile rendering from HDF raster samples.
//!
//! Rendering extracts radiance and quality samples for a tile window, applies
//! product-specific science rules, and encodes accepted pixels as PNG tiles.

use std::path::Path;

use crate::{
    hdf_cli::{self, RasterOutputSize, RasterSample},
    publish::RenderedTile,
    render::{render_png_tile, renderable_pixel_count, RenderPixel},
    science::{self, DatasetMapping},
    tiles::TileCoord,
};

use super::error::GenerateError;
use super::window::{TilePixelWindow, TileRasterWindow};

#[cfg(test)]
use crate::hdf_cli::RasterWindow;

/// Converts aligned raster samples into renderable tile pixels.
///
/// Radiance samples are kept only when the product-specific quality rules allow
/// them; rejected samples are preserved as rejected pixels for the renderer.
pub fn render_pixels_from_samples(
    mapping: &DatasetMapping,
    radiance_samples: &[RasterSample],
    quality_samples: &[RasterSample],
    cloud_samples: Option<&[RasterSample]>,
    observation_count_samples: Option<&[RasterSample]>,
) -> Result<Vec<RenderPixel>, GenerateError> {
    // The sample arrays are indexed together below, so each optional dataset
    // must either be absent or match the radiance sample count exactly.
    validate_aligned_sample_counts(
        radiance_samples.len(),
        quality_samples.len(),
        cloud_samples.map(<[RasterSample]>::len),
        observation_count_samples.map(<[RasterSample]>::len),
    )?;

    let mut pixels = Vec::with_capacity(radiance_samples.len());

    for (index, radiance_sample) in radiance_samples.iter().enumerate() {
        let classification = science::classify_pixel_sample(
            mapping,
            radiance_sample.value,
            quality_samples[index].value,
            cloud_samples
                .and_then(|samples| samples.get(index))
                .map(|sample| sample.value),
            observation_count_samples
                .and_then(|samples| samples.get(index))
                .map(|sample| sample.value),
        )?;

        let rejected = !science::is_renderable_sample(&classification);

        pixels.push(RenderPixel {
            radiance: (!rejected).then_some(radiance_sample.value),
            rejected,
        });
    }

    Ok(pixels)
}

#[cfg(test)]
pub fn render_tile_window_from_samples(
    mapping: &DatasetMapping,
    coord: TileCoord,
    window: hdf_cli::RasterWindow,
    radiance_samples: &[RasterSample],
    quality_samples: &[RasterSample],
    cloud_samples: Option<&[RasterSample]>,
    observation_count_samples: Option<&[RasterSample]>,
) -> Result<RenderedTile, GenerateError> {
    let tile_size = tile_size_from_window(window)?;

    render_tile_from_samples(
        mapping,
        coord,
        tile_size,
        radiance_samples,
        quality_samples,
        cloud_samples,
        observation_count_samples,
    )
}

/// Renders one tile from already extracted and resized raster samples.
pub fn render_tile_from_samples(
    mapping: &DatasetMapping,
    coord: TileCoord,
    tile_size: u16,
    radiance_samples: &[RasterSample],
    quality_samples: &[RasterSample],
    cloud_samples: Option<&[RasterSample]>,
    observation_count_samples: Option<&[RasterSample]>,
) -> Result<RenderedTile, GenerateError> {
    let pixels = render_pixels_from_samples(
        mapping,
        radiance_samples,
        quality_samples,
        cloud_samples,
        observation_count_samples,
    )?;

    let png_bytes = render_png_tile(tile_size, &pixels)
        .map_err(|source| GenerateError::RenderTile { coord, source })?;
    let renderable_pixel_count = renderable_pixel_count(&pixels);

    Ok(RenderedTile {
        coord,
        png_bytes,
        renderable_pixel_count,
    })
}

#[cfg(test)]
fn tile_size_from_window(window: RasterWindow) -> Result<u16, GenerateError> {
    if window.width != window.height {
        return Err(GenerateError::InvalidTileWindow {
            width: window.width,
            height: window.height,
        });
    }

    u16::try_from(window.width).map_err(|_| GenerateError::InvalidTileWindow {
        width: window.width,
        height: window.height,
    })
}

/// Extracts, resizes, classifies, and renders one tile window from an HDF granule.
pub fn render_tile_window_from_granule(
    granule_path: &Path,
    mapping: &DatasetMapping,
    coord: TileCoord,
    window: TileRasterWindow,
    tile_size: u16,
) -> Result<RenderedTile, GenerateError> {
    let output_size = RasterOutputSize {
        width: window.target.width,
        height: window.target.height,
    };

    let radiance_samples = tile_samples_from_resized_window(
        granule_path,
        mapping.radiance_dataset,
        window,
        output_size,
        tile_size,
        mapping.radiance_fill_value,
    )?;

    let quality_samples = tile_samples_from_resized_window(
        granule_path,
        mapping.quality_dataset,
        window,
        output_size,
        tile_size,
        f32::from(u8::MAX),
    )?;

    // Cloud and observation-count datasets vary by VIIRS product cadence.
    let cloud_samples = match mapping.cloud_dataset {
        Some(dataset) => Some(tile_samples_from_resized_window(
            granule_path,
            dataset,
            window,
            output_size,
            tile_size,
            0.0,
        )?),
        None => None,
    };

    let observation_count_samples = match mapping.observation_count_dataset {
        Some(dataset) => Some(tile_samples_from_resized_window(
            granule_path,
            dataset,
            window,
            output_size,
            tile_size,
            0.0,
        )?),
        None => None,
    };

    render_tile_from_samples(
        mapping,
        coord,
        tile_size,
        &radiance_samples,
        &quality_samples,
        cloud_samples.as_deref(),
        observation_count_samples.as_deref(),
    )
}

fn tile_samples_from_resized_window(
    granule_path: &Path,
    dataset: &'static str,
    window: TileRasterWindow,
    output_size: RasterOutputSize,
    tile_size: u16,
    fill_value: f32,
) -> Result<Vec<RasterSample>, GenerateError> {
    let samples =
        hdf_cli::dataset_window_samples_resized(granule_path, dataset, window.source, output_size)?;

    expand_samples_to_tile(
        dataset,
        &samples,
        usize::from(tile_size),
        window.target,
        fill_value,
    )
}

fn expand_samples_to_tile(
    dataset: &'static str,
    samples: &[RasterSample],
    tile_size: usize,
    target: TilePixelWindow,
    fill_value: f32,
) -> Result<Vec<RasterSample>, GenerateError> {
    let expected = target.width * target.height;
    if samples.len() != expected {
        return Err(GenerateError::ResizedSampleCountMismatch {
            dataset,
            expected,
            actual: samples.len(),
        });
    }

    let mut expanded = (0..(tile_size * tile_size))
        .map(|index| RasterSample {
            x: (index % tile_size) as f64,
            y: (index / tile_size) as f64,
            value: fill_value,
        })
        .collect::<Vec<_>>();

    for row in 0..target.height {
        for column in 0..target.width {
            let source_index = row * target.width + column;
            let destination_x = target.x_offset + column;
            let destination_y = target.y_offset + row;
            let destination_index = destination_y * tile_size + destination_x;
            expanded[destination_index] = RasterSample {
                x: destination_x as f64,
                y: destination_y as f64,
                value: samples[source_index].value,
            };
        }
    }

    Ok(expanded)
}

/// Validates that all present datasets describe the same resized output grid.
fn validate_aligned_sample_counts(
    radiance: usize,
    quality: usize,
    cloud: Option<usize>,
    observation_count: Option<usize>,
) -> Result<(), GenerateError> {
    let aligned = radiance == quality
        && cloud.is_none_or(|count| count == radiance)
        && observation_count.is_none_or(|count| count == radiance);

    if aligned {
        Ok(())
    } else {
        Err(GenerateError::SampleCountMismatch {
            radiance,
            quality,
            cloud,
            observation_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::RenderError;

    fn sample_raster_values(values: &[f32]) -> Vec<RasterSample> {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| RasterSample {
                x: index as f64,
                y: 0.0,
                value: *value,
            })
            .collect()
    }

    #[test]
    fn renders_tile_window_from_aligned_samples() {
        let mapping = crate::science::dataset_mapping_for_product(
            shared::processing_message::ProcessingProduct::Vnp46A2,
        );

        let rendered = render_tile_window_from_samples(
            mapping,
            TileCoord { z: 5, x: 8, y: 12 },
            RasterWindow {
                x_offset: 0,
                y_offset: 0,
                width: 2,
                height: 2,
            },
            &sample_raster_values(&[0.1, 0.5, 1.25, 50.0]),
            &sample_raster_values(&[0.0, 0.0, 0.0, 0.0]),
            Some(&sample_raster_values(&[0.0, 0.0, 0.0, 0.0])),
            None,
        )
        .unwrap();

        assert_eq!(rendered.coord, TileCoord { z: 5, x: 8, y: 12 });
        assert_eq!(rendered.renderable_pixel_count, 4);
        assert!(rendered.png_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn expands_partial_window_samples_into_full_tile_with_fill_value() {
        let expanded = expand_samples_to_tile(
            "test-dataset",
            &[
                RasterSample {
                    x: 0.0,
                    y: 0.0,
                    value: 1.0,
                },
                RasterSample {
                    x: 0.0,
                    y: 1.0,
                    value: 2.0,
                },
            ],
            3,
            TilePixelWindow {
                x_offset: 1,
                y_offset: 1,
                width: 1,
                height: 2,
            },
            -999.9,
        )
        .unwrap();

        assert_eq!(expanded.len(), 9);
        assert_eq!(expanded[4].value, 1.0);
        assert_eq!(expanded[7].value, 2.0);
        assert_eq!(expanded[0].value, -999.9);
        assert_eq!(expanded[8].value, -999.9);
    }

    #[test]
    fn rejects_resized_sample_count_mismatch() {
        let error = expand_samples_to_tile(
            "test-dataset",
            &[RasterSample {
                x: 0.0,
                y: 0.0,
                value: 1.0,
            }],
            3,
            TilePixelWindow {
                x_offset: 1,
                y_offset: 1,
                width: 1,
                height: 2,
            },
            -999.9,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            GenerateError::ResizedSampleCountMismatch {
                dataset: "test-dataset",
                expected: 2,
                actual: 1,
            }
        ));
    }

    #[test]
    fn render_tile_window_from_samples_rejects_non_square_window() {
        let mapping = crate::science::dataset_mapping_for_product(
            shared::processing_message::ProcessingProduct::Vnp46A2,
        );

        let error = render_tile_window_from_samples(
            mapping,
            TileCoord { z: 5, x: 8, y: 12 },
            RasterWindow {
                x_offset: 0,
                y_offset: 0,
                width: 2,
                height: 1,
            },
            &sample_raster_values(&[0.1, 0.5]),
            &sample_raster_values(&[0.0, 0.0]),
            None,
            None,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            GenerateError::InvalidTileWindow {
                width: 2,
                height: 1,
            }
        ));
    }

    #[test]
    fn converts_aligned_samples_to_render_pixels() {
        let mapping = crate::science::dataset_mapping_for_product(
            shared::processing_message::ProcessingProduct::Vnp46A2,
        );

        let radiance = sample_raster_values(&[0.1, 1.25, -1.0]);
        let quality = sample_raster_values(&[0.0, 0.0, 0.0]);
        let cloud = sample_raster_values(&[0.0, (0b10 << 6) as f32, 0.0]);

        let pixels =
            render_pixels_from_samples(mapping, &radiance, &quality, Some(&cloud), None).unwrap();

        assert_eq!(
            pixels,
            vec![
                RenderPixel {
                    radiance: Some(0.1),
                    rejected: false,
                },
                RenderPixel {
                    radiance: None,
                    rejected: true,
                },
                RenderPixel {
                    radiance: None,
                    rejected: true,
                },
            ]
        );
    }

    #[test]
    fn rejects_mismatched_sample_counts() {
        let mapping = crate::science::dataset_mapping_for_product(
            shared::processing_message::ProcessingProduct::Vnp46A2,
        );

        let radiance = sample_raster_values(&[0.1, 1.25]);
        let quality = sample_raster_values(&[0.0]);

        let error =
            render_pixels_from_samples(mapping, &radiance, &quality, None, None).unwrap_err();

        assert!(matches!(
            error,
            GenerateError::SampleCountMismatch {
                radiance: 2,
                quality: 1,
                cloud: None,
                observation_count: None,
            }
        ));
    }

    #[test]
    fn derives_tile_size_from_square_window() {
        assert_eq!(
            tile_size_from_window(RasterWindow {
                x_offset: 0,
                y_offset: 0,
                width: 256,
                height: 256,
            })
            .unwrap(),
            256
        );
    }

    #[test]
    fn rejects_non_square_tile_window() {
        assert!(matches!(
            tile_size_from_window(RasterWindow {
                x_offset: 0,
                y_offset: 0,
                width: 256,
                height: 128,
            }),
            Err(GenerateError::InvalidTileWindow {
                width: 256,
                height: 128,
            })
        ));
    }

    #[test]
    fn renders_tile_from_samples_with_explicit_tile_size() {
        let mapping = crate::science::dataset_mapping_for_product(
            shared::processing_message::ProcessingProduct::Vnp46A2,
        );

        let rendered = render_tile_from_samples(
            mapping,
            TileCoord { z: 5, x: 8, y: 12 },
            2,
            &sample_raster_values(&[0.1, 0.5, 1.25, 50.0]),
            &sample_raster_values(&[0.0, 0.0, 0.0, 0.0]),
            Some(&sample_raster_values(&[0.0, 0.0, 0.0, 0.0])),
            None,
        )
        .unwrap();

        assert_eq!(rendered.coord, TileCoord { z: 5, x: 8, y: 12 });
        assert_eq!(rendered.renderable_pixel_count, 4);
        assert!(rendered.png_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn wraps_render_errors_with_tile_coordinate() {
        let mapping = crate::science::dataset_mapping_for_product(
            shared::processing_message::ProcessingProduct::Vnp46A2,
        );

        let error = render_tile_from_samples(
            mapping,
            TileCoord { z: 5, x: 8, y: 12 },
            2,
            &sample_raster_values(&[0.1]),
            &sample_raster_values(&[0.0]),
            None,
            None,
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
}
