//! Pixel-level science classification and quality summaries.
//!
//! This module combines radiance validity, cadence-specific quality masks,
//! optional cloud masks, and optional observation counts into processing and
//! rendering decisions.

use super::{
    classification::{
        classify_daily_quality, classify_monthly_quality, classify_radiance, is_cloud_contaminated,
        PixelQuality,
    },
    mapping::DatasetMapping,
    validation::{sample_value_as_u16, ScienceError},
};
use shared::processing_message::ProductCadence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelSampleClassification {
    pub radiance_quality: PixelQuality,
    pub quality_mask_quality: PixelQuality,
    pub quality_sample: u16,
    pub cloud_contaminated: Option<bool>,
    pub observation_count_sample: Option<u16>,
}

/// Classifies one aligned set of radiance and quality samples.
///
/// Daily products can include cloud-mask samples. Monthly products can include
/// observation counts. Missing optional samples are preserved as `None` rather
/// than treated as invalid.
pub(crate) fn classify_pixel_sample(
    mapping: &DatasetMapping,
    radiance_value: f32,
    quality_value: f32,
    cloud_value: Option<f32>,
    observation_count_value: Option<f32>,
) -> Result<PixelSampleClassification, ScienceError> {
    let radiance_quality = classify_radiance(mapping, radiance_value);

    let quality_sample = sample_value_as_u16(mapping.quality_dataset, quality_value)?;
    let quality_flag = quality_sample.min(u8::MAX as u16) as u8;

    // Daily and monthly products encode acceptable quality with different flags.
    let quality_mask_quality = match mapping.cadence {
        ProductCadence::Daily => classify_daily_quality(quality_flag),
        ProductCadence::Monthly => classify_monthly_quality(quality_flag),
    };

    let cloud_contaminated = match (mapping.cloud_dataset, cloud_value) {
        (Some(dataset), Some(value)) => {
            let cloud_mask = sample_value_as_u16(dataset, value)?;
            Some(is_cloud_contaminated(cloud_mask))
        }
        _ => None,
    };

    let observation_count_sample =
        match (mapping.observation_count_dataset, observation_count_value) {
            (Some(dataset), Some(value)) => Some(sample_value_as_u16(dataset, value)?),
            _ => None,
        };

    Ok(PixelSampleClassification {
        radiance_quality,
        quality_mask_quality,
        quality_sample,
        cloud_contaminated,
        observation_count_sample,
    })
}

pub(crate) const QUALITY_RULE_VERSION: &str = "viirs-quality-v1";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct QualitySummary {
    pub total_pixel_count: usize,
    pub valid_pixel_count: usize,
    pub rejected_pixel_count: usize,
    pub cloud_contaminated_valid_pixel_count: usize,
    pub cloud_fraction: f32,
}

/// Summarizes pixel classifications for rejection decisions and metadata.
///
/// Cloud fraction is calculated over otherwise valid pixels so invalid radiance
/// or quality-mask samples do not dilute the cloudiness estimate.
pub(crate) fn summarize_quality(classifications: &[PixelSampleClassification]) -> QualitySummary {
    let total_pixel_count = classifications.len();
    let mut valid_pixel_count = 0;
    let mut cloud_contaminated_valid_pixel_count = 0;

    for classification in classifications {
        let valid = classification.radiance_quality == PixelQuality::Valid
            && classification.quality_mask_quality == PixelQuality::Valid;

        if valid {
            valid_pixel_count += 1;

            if classification.cloud_contaminated == Some(true) {
                cloud_contaminated_valid_pixel_count += 1;
            }
        }
    }

    let rejected_pixel_count = total_pixel_count - valid_pixel_count;
    // No valid pixels means there is no valid cloud denominator to compare.
    let cloud_fraction = if valid_pixel_count == 0 {
        0.0
    } else {
        cloud_contaminated_valid_pixel_count as f32 / valid_pixel_count as f32
    };

    QualitySummary {
        total_pixel_count,
        valid_pixel_count,
        rejected_pixel_count,
        cloud_contaminated_valid_pixel_count,
        cloud_fraction,
    }
}

/// Returns true when a classified sample should contribute radiance to a tile.
pub(crate) fn is_renderable_sample(classification: &PixelSampleClassification) -> bool {
    classification.radiance_quality == PixelQuality::Valid
        && classification.quality_mask_quality == PixelQuality::Valid
        && classification.cloud_contaminated != Some(true)
}

/// Applies the configured maximum cloud fraction threshold.
pub(crate) fn exceeds_max_cloud_fraction(
    summary: &QualitySummary,
    max_cloud_fraction: f32,
) -> bool {
    summary.cloud_fraction > max_cloud_fraction
}

#[cfg(test)]
mod tests {
    use super::{
        classify_pixel_sample, exceeds_max_cloud_fraction, summarize_quality,
        PixelSampleClassification, QualitySummary, QUALITY_RULE_VERSION,
    };
    use crate::science::{classification::PixelQuality, dataset_mapping_for_product};
    use shared::processing_message::ProcessingProduct;

    #[test]
    fn classifies_daily_pixel_sample() {
        let mapping = dataset_mapping_for_product(ProcessingProduct::Vnp46A2);

        let classification =
            classify_pixel_sample(mapping, 1.25, 0.0, Some((0b10 << 6) as f32), None).unwrap();

        assert_eq!(classification.radiance_quality, PixelQuality::Valid);
        assert_eq!(classification.quality_mask_quality, PixelQuality::Valid);
        assert_eq!(classification.quality_sample, 0);
        assert_eq!(classification.cloud_contaminated, Some(true));
        assert_eq!(classification.observation_count_sample, None);
    }

    #[test]
    fn classifies_monthly_pixel_sample() {
        let mapping = dataset_mapping_for_product(ProcessingProduct::Vnp46A3);

        let classification = classify_pixel_sample(mapping, 1.25, 2.0, None, Some(12.0)).unwrap();

        assert_eq!(classification.radiance_quality, PixelQuality::Valid);
        assert_eq!(classification.quality_mask_quality, PixelQuality::Valid);
        assert_eq!(classification.quality_sample, 2);
        assert_eq!(classification.cloud_contaminated, None);
        assert_eq!(classification.observation_count_sample, Some(12));
    }

    #[test]
    fn summarizes_quality_counts_only_valid_pixels_in_cloud_denominator() {
        let summary = summarize_quality(&[
            PixelSampleClassification {
                radiance_quality: PixelQuality::Valid,
                quality_mask_quality: PixelQuality::Valid,
                quality_sample: 0,
                cloud_contaminated: Some(false),
                observation_count_sample: None,
            },
            PixelSampleClassification {
                radiance_quality: PixelQuality::Valid,
                quality_mask_quality: PixelQuality::Valid,
                quality_sample: 0,
                cloud_contaminated: Some(true),
                observation_count_sample: None,
            },
            PixelSampleClassification {
                radiance_quality: PixelQuality::Invalid,
                quality_mask_quality: PixelQuality::Valid,
                quality_sample: 0,
                cloud_contaminated: Some(true),
                observation_count_sample: None,
            },
            PixelSampleClassification {
                radiance_quality: PixelQuality::Valid,
                quality_mask_quality: PixelQuality::Invalid,
                quality_sample: 1,
                cloud_contaminated: Some(true),
                observation_count_sample: None,
            },
        ]);

        assert_eq!(summary.total_pixel_count, 4);
        assert_eq!(summary.valid_pixel_count, 2);
        assert_eq!(summary.rejected_pixel_count, 2);
        assert_eq!(summary.cloud_contaminated_valid_pixel_count, 1);
        assert_eq!(summary.cloud_fraction, 0.5);
    }

    #[test]
    fn summarizes_quality_with_zero_valid_pixels_as_zero_cloud_fraction() {
        let summary = summarize_quality(&[PixelSampleClassification {
            radiance_quality: PixelQuality::Invalid,
            quality_mask_quality: PixelQuality::Invalid,
            quality_sample: 1,
            cloud_contaminated: Some(true),
            observation_count_sample: None,
        }]);

        assert_eq!(summary.total_pixel_count, 1);
        assert_eq!(summary.valid_pixel_count, 0);
        assert_eq!(summary.rejected_pixel_count, 1);
        assert_eq!(summary.cloud_contaminated_valid_pixel_count, 0);
        assert_eq!(summary.cloud_fraction, 0.0);
    }

    #[test]
    fn exposes_quality_rule_version() {
        assert_eq!(QUALITY_RULE_VERSION, "viirs-quality-v1");
    }

    #[test]
    fn cloud_fraction_rejection_triggers_above_threshold() {
        let summary = QualitySummary {
            total_pixel_count: 4,
            valid_pixel_count: 4,
            rejected_pixel_count: 0,
            cloud_contaminated_valid_pixel_count: 3,
            cloud_fraction: 0.75,
        };

        assert!(exceeds_max_cloud_fraction(&summary, 0.5));
    }

    #[test]
    fn cloud_fraction_rejection_does_not_trigger_at_threshold() {
        let summary = QualitySummary {
            total_pixel_count: 4,
            valid_pixel_count: 4,
            rejected_pixel_count: 0,
            cloud_contaminated_valid_pixel_count: 2,
            cloud_fraction: 0.5,
        };

        assert!(!exceeds_max_cloud_fraction(&summary, 0.5));
    }

    #[test]
    fn cloud_fraction_rejection_does_not_trigger_without_valid_denominator() {
        let summary = QualitySummary {
            total_pixel_count: 2,
            valid_pixel_count: 0,
            rejected_pixel_count: 2,
            cloud_contaminated_valid_pixel_count: 0,
            cloud_fraction: 0.0,
        };

        assert!(!exceeds_max_cloud_fraction(&summary, 0.5));
    }
}
