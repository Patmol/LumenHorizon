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
