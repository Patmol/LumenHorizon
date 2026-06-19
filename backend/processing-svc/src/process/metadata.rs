use crate::{hdf_cli, science};

pub(super) fn build_quality_metadata(
    sample_window: hdf_cli::RasterWindow,
    quality_summary: &science::QualitySummary,
    max_cloud_fraction: f32,
    exceeds_max_cloud_fraction: bool,
    first_dark_sky_class: Option<science::DarkSkyClass>,
) -> serde_json::Value {
    serde_json::json!({
        "quality_rule_version": science::QUALITY_RULE_VERSION,
        "sample_window": {
            "x_offset": sample_window.x_offset,
            "y_offset": sample_window.y_offset,
            "width": sample_window.width,
            "height": sample_window.height,
        },
        "quality_summary": {
            "total_pixel_count": quality_summary.total_pixel_count,
            "valid_pixel_count": quality_summary.valid_pixel_count,
            "rejected_pixel_count": quality_summary.rejected_pixel_count,
            "cloud_contaminated_valid_pixel_count": quality_summary.cloud_contaminated_valid_pixel_count,
            "cloud_fraction": quality_summary.cloud_fraction,
            "max_cloud_fraction": max_cloud_fraction,
            "exceeds_max_cloud_fraction": exceeds_max_cloud_fraction,
        },
        "dark_sky_classification": {
            "version": science::DARK_SKY_CLASSIFICATION_VERSION,
            "first_sample": first_dark_sky_class.map(|classification| serde_json::json!({
                "class": classification.class,
                "color_hex": classification.color_hex,
                "label": classification.label,
            })),
        },
    })
}

pub(super) fn cloud_rejection_reason(
    quality_summary: &science::QualitySummary,
    max_cloud_fraction: f32,
) -> Option<&'static str> {
    science::exceeds_max_cloud_fraction(quality_summary, max_cloud_fraction)
        .then_some("cloud fraction exceeds configured maximum")
}

#[cfg(test)]
mod tests {
    use super::{build_quality_metadata, cloud_rejection_reason};
    use crate::{hdf_cli::RasterWindow, science};

    #[test]
    fn quality_metadata_records_acceptance_fixture() {
        let summary = science::QualitySummary {
            total_pixel_count: 4,
            valid_pixel_count: 3,
            rejected_pixel_count: 1,
            cloud_contaminated_valid_pixel_count: 1,
            cloud_fraction: 1.0 / 3.0,
        };
        let dark_sky_class = science::classify_dark_sky(0.4);

        let metadata = build_quality_metadata(
            RasterWindow {
                x_offset: 10,
                y_offset: 20,
                width: 2,
                height: 2,
            },
            &summary,
            0.4,
            false,
            dark_sky_class,
        );

        assert_eq!(
            metadata["quality_rule_version"],
            science::QUALITY_RULE_VERSION
        );
        assert_eq!(metadata["sample_window"]["x_offset"], 10);
        assert_eq!(metadata["sample_window"]["y_offset"], 20);
        assert_eq!(metadata["sample_window"]["width"], 2);
        assert_eq!(metadata["sample_window"]["height"], 2);
        assert_eq!(metadata["quality_summary"]["total_pixel_count"], 4);
        assert_eq!(metadata["quality_summary"]["valid_pixel_count"], 3);
        assert_eq!(metadata["quality_summary"]["rejected_pixel_count"], 1);
        assert_eq!(
            metadata["quality_summary"]["cloud_contaminated_valid_pixel_count"],
            1
        );
        let max_cloud_fraction = metadata["quality_summary"]["max_cloud_fraction"]
            .as_f64()
            .unwrap();
        assert!((max_cloud_fraction - 0.4).abs() < 0.000_001);
        assert_eq!(
            metadata["quality_summary"]["exceeds_max_cloud_fraction"],
            false
        );
        assert_eq!(
            metadata["dark_sky_classification"]["version"],
            science::DARK_SKY_CLASSIFICATION_VERSION
        );
        assert_eq!(
            metadata["dark_sky_classification"]["first_sample"]["class"],
            3
        );
    }

    #[test]
    fn cloud_rejection_reason_matches_threshold_metadata() {
        let summary = science::QualitySummary {
            total_pixel_count: 4,
            valid_pixel_count: 4,
            rejected_pixel_count: 0,
            cloud_contaminated_valid_pixel_count: 3,
            cloud_fraction: 0.75,
        };

        assert_eq!(
            cloud_rejection_reason(&summary, 0.4),
            Some("cloud fraction exceeds configured maximum")
        );

        let metadata = build_quality_metadata(
            RasterWindow {
                x_offset: 0,
                y_offset: 0,
                width: 2,
                height: 2,
            },
            &summary,
            0.4,
            true,
            None,
        );

        assert_eq!(
            metadata["quality_summary"]["exceeds_max_cloud_fraction"],
            true
        );
        assert!(metadata["dark_sky_classification"]["first_sample"].is_null());
    }
}
