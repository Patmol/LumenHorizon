//! Low-level pixel-quality classification rules.
//!
//! These helpers keep the raw VIIRS quality flags and cloud-mask bits separate
//! from higher-level processing decisions.

use super::mapping::DatasetMapping;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PixelQuality {
    Valid,
    Invalid,
}

/// Classifies a radiance sample using the product fill value and valid minimum.
pub(crate) fn classify_radiance(mapping: &DatasetMapping, radiance: f32) -> PixelQuality {
    if radiance == mapping.radiance_fill_value || radiance < mapping.radiance_valid_min {
        PixelQuality::Invalid
    } else {
        PixelQuality::Valid
    }
}

/// Classifies daily mandatory quality flags.
pub(crate) fn classify_daily_quality(quality_flag: u8) -> PixelQuality {
    match quality_flag {
        0 => PixelQuality::Valid,
        _ => PixelQuality::Invalid,
    }
}

/// Classifies monthly composite quality flags.
pub(crate) fn classify_monthly_quality(quality_flag: u8) -> PixelQuality {
    match quality_flag {
        0 | 2 => PixelQuality::Valid,
        _ => PixelQuality::Invalid,
    }
}

/// Returns whether daily cloud-mask bits indicate cloud contamination.
pub(crate) fn is_cloud_contaminated(qf_cloud_mask: u16) -> bool {
    // Bits 6-7 encode cloud-detection confidence; 2 and 3 are treated as cloudy.
    let cloud_detection_bits = (qf_cloud_mask >> 6) & 0b11;

    matches!(cloud_detection_bits, 0b10 | 0b11)
}

#[cfg(test)]
mod tests {
    use super::{
        classify_daily_quality, classify_monthly_quality, classify_radiance, is_cloud_contaminated,
        PixelQuality,
    };
    use crate::science::dataset_mapping_for_product;
    use shared::processing_message::ProcessingProduct;

    #[test]
    fn classifies_radiance_fill_and_negative_values_as_invalid() {
        let mapping = dataset_mapping_for_product(ProcessingProduct::Vnp46A2);

        assert_eq!(
            classify_radiance(mapping, mapping.radiance_fill_value),
            PixelQuality::Invalid
        );
        assert_eq!(classify_radiance(mapping, -1.0), PixelQuality::Invalid);
        assert_eq!(classify_radiance(mapping, 0.0), PixelQuality::Valid);
        assert_eq!(classify_radiance(mapping, 1.25), PixelQuality::Valid);
    }

    #[test]
    fn classifies_daily_mandatory_quality() {
        assert_eq!(classify_daily_quality(0), PixelQuality::Valid);
        assert_eq!(classify_daily_quality(1), PixelQuality::Invalid);
        assert_eq!(classify_daily_quality(2), PixelQuality::Invalid);
        assert_eq!(classify_daily_quality(5), PixelQuality::Invalid);
        assert_eq!(classify_daily_quality(255), PixelQuality::Invalid);
    }

    #[test]
    fn classifies_monthly_quality() {
        assert_eq!(classify_monthly_quality(0), PixelQuality::Valid);
        assert_eq!(classify_monthly_quality(1), PixelQuality::Invalid);
        assert_eq!(classify_monthly_quality(2), PixelQuality::Valid);
        assert_eq!(classify_monthly_quality(255), PixelQuality::Invalid);
    }

    #[test]
    fn detects_cloud_contamination_from_daily_cloud_mask_bits() {
        assert!(!is_cloud_contaminated(0b00 << 6));
        assert!(!is_cloud_contaminated(0b01 << 6));
        assert!(is_cloud_contaminated(0b10 << 6));
        assert!(is_cloud_contaminated(0b11 << 6));
    }
}
