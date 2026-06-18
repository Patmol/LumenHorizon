//! Validation helpers for VIIRS science sample contracts.
//!
//! Processing expects quality, cloud, and observation datasets to align with
//! radiance grids before samples are classified together.

use crate::hdf_cli::{RasterShape, RasterWindow};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub(crate) enum ScienceError {
    #[error(
        "science dataset shape mismatch: '{}' has {}x{}, expected {}x{} to match '{}'",
        compared_dataset,
        compared_shape.width,
        compared_shape.height,
        reference_shape.width,
        reference_shape.height,
        reference_dataset
    )]
    DatasetShapeMismatch {
        reference_dataset: &'static str,
        compared_dataset: &'static str,
        reference_shape: RasterShape,
        compared_shape: RasterShape,
    },
    #[error(
        "science sample value for '{dataset}' must be a finite non-negative integer that fits in u16, got {value}"
    )]
    InvalidIntegerSample { dataset: &'static str, value: f32 },
    #[error(
        "science sample window count mismatch: '{dataset}' returned {actual_count} samples, expected {expected_count} for window x={}, y={}, width={}, height={}",
        window.x_offset,
        window.y_offset,
        window.width,
        window.height
    )]
    SampleWindowCountMismatch {
        dataset: &'static str,
        window: RasterWindow,
        expected_count: usize,
        actual_count: usize,
    },
}

/// Ensures two datasets expose the same raster shape before aligned sampling.
pub(crate) fn validate_matching_shape(
    reference_dataset: &'static str,
    reference_shape: RasterShape,
    compared_dataset: &'static str,
    compared_shape: RasterShape,
) -> Result<(), ScienceError> {
    if reference_shape != compared_shape {
        return Err(ScienceError::DatasetShapeMismatch {
            reference_dataset,
            compared_dataset,
            reference_shape,
            compared_shape,
        });
    }

    Ok(())
}

/// Ensures an extracted sample window returned exactly one sample per pixel.
pub(crate) fn validate_sample_count(
    dataset: &'static str,
    window: RasterWindow,
    actual_count: usize,
) -> Result<(), ScienceError> {
    let expected_count = window.width * window.height;

    if actual_count != expected_count {
        return Err(ScienceError::SampleWindowCountMismatch {
            dataset,
            window,
            expected_count,
            actual_count,
        });
    }

    Ok(())
}

/// Converts a raw sample to `u16` when it represents an integer quality value.
pub(crate) fn sample_value_as_u16(dataset: &'static str, value: f32) -> Result<u16, ScienceError> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > u16::MAX as f32 {
        return Err(ScienceError::InvalidIntegerSample { dataset, value });
    }

    Ok(value as u16)
}

#[cfg(test)]
mod tests {
    use super::{
        sample_value_as_u16, validate_matching_shape, validate_sample_count, ScienceError,
    };
    use crate::hdf_cli::{RasterShape, RasterWindow};

    #[test]
    fn accepts_matching_dataset_shapes() {
        let shape = RasterShape {
            width: 2400,
            height: 2400,
        };

        validate_matching_shape("radiance", shape, "quality", shape).unwrap();
    }

    #[test]
    fn rejects_mismatched_dataset_shapes() {
        let error = validate_matching_shape(
            "radiance",
            RasterShape {
                width: 2400,
                height: 2400,
            },
            "quality",
            RasterShape {
                width: 1200,
                height: 2400,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ScienceError::DatasetShapeMismatch {
                reference_dataset: "radiance",
                compared_dataset: "quality",
                reference_shape: RasterShape {
                    width: 2400,
                    height: 2400,
                },
                compared_shape: RasterShape {
                    width: 1200,
                    height: 2400,
                },
            }
        ));

        assert!(error.to_string().contains("science dataset shape mismatch"));
    }

    #[test]
    fn accepts_expected_sample_count() {
        let window = RasterWindow {
            x_offset: 0,
            y_offset: 0,
            width: 2,
            height: 3,
        };

        validate_sample_count("radiance", window, 6).unwrap();
    }

    #[test]
    fn rejects_unexpected_sample_count() {
        let window = RasterWindow {
            x_offset: 0,
            y_offset: 0,
            width: 2,
            height: 3,
        };

        let error = validate_sample_count("radiance", window, 5).unwrap_err();

        assert!(matches!(
            error,
            ScienceError::SampleWindowCountMismatch {
                dataset: "radiance",
                expected_count: 6,
                actual_count: 5,
                ..
            }
        ));
    }

    #[test]
    fn converts_valid_integer_sample_to_u16() {
        assert_eq!(sample_value_as_u16("quality", 0.0).unwrap(), 0);
        assert_eq!(sample_value_as_u16("quality", 255.0).unwrap(), 255);
        assert_eq!(sample_value_as_u16("quality", 65535.0).unwrap(), 65535);
    }

    #[test]
    fn rejects_invalid_integer_samples() {
        for value in [f32::NAN, f32::INFINITY, -1.0, 1.5, 65536.0] {
            let error = sample_value_as_u16("quality", value).unwrap_err();

            assert!(matches!(
                error,
                ScienceError::InvalidIntegerSample {
                    dataset: "quality",
                    ..
                }
            ));
        }
    }
}
