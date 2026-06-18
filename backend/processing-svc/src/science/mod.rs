//! VIIRS science rules used by processing and tile rendering.
//!
//! The science layer maps product names to HDF datasets, validates sample
//! shapes, classifies pixel quality, and assigns radiance-based dark-sky classes.

mod classification;
mod dark_sky;
mod mapping;
mod quality;
mod validation;

pub(crate) use dark_sky::{classify_dark_sky, DarkSkyClass, DARK_SKY_CLASSIFICATION_VERSION};
pub(crate) use mapping::{dataset_mapping_for_product, DatasetMapping};
pub(crate) use quality::{
    classify_pixel_sample, exceeds_max_cloud_fraction, is_renderable_sample, summarize_quality,
    QualitySummary, QUALITY_RULE_VERSION,
};
pub(crate) use validation::{validate_matching_shape, validate_sample_count, ScienceError};
