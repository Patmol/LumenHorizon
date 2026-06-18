//! Product-specific HDF dataset mappings.
//!
//! Daily VIIRS products provide radiance, mandatory quality, and cloud-mask
//! datasets. Monthly products use a different radiance composite and expose
//! observation counts instead of cloud masks.

use shared::processing_message::{ProcessingProduct, ProductCadence};

const DAILY_RADIANCE_DATASET: &str =
    "//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Gap_Filled_DNB_BRDF-Corrected_NTL";
const DAILY_QUALITY_DATASET: &str =
    "//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Mandatory_Quality_Flag";
const DAILY_CLOUD_DATASET: &str = "//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/QF_Cloud_Mask";

const MONTHLY_RADIANCE_DATASET: &str =
    "//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free";
const MONTHLY_QUALITY_DATASET: &str =
    "//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free_Quality";
const MONTHLY_OBSERVATION_COUNT_DATASET: &str =
    "//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/AllAngle_Composite_Snow_Free_Num";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DatasetMapping {
    pub product: ProcessingProduct,
    pub cadence: ProductCadence,
    pub radiance_dataset: &'static str,
    pub quality_dataset: &'static str,
    pub cloud_dataset: Option<&'static str>,
    pub observation_count_dataset: Option<&'static str>,
    pub radiance_fill_value: f32,
    pub radiance_valid_min: f32,
    pub radiance_scale_factor: f32,
    pub radiance_offset: f32,
    pub radiance_units: &'static str,
    pub quality_fill_value: u16,
    pub cloud_fill_value: Option<u16>,
    pub observation_count_fill_value: Option<u16>,
}

/// Returns the HDF dataset contract for a supported processing product.
pub(crate) fn dataset_mapping_for_product(product: ProcessingProduct) -> &'static DatasetMapping {
    match product {
        ProcessingProduct::Vnp46A2 => &VNP46A2_MAPPING,
        ProcessingProduct::Vj146A2 => &VJ146A2_MAPPING,
        ProcessingProduct::Vnp46A3 => &VNP46A3_MAPPING,
    }
}

static VNP46A2_MAPPING: DatasetMapping = DatasetMapping {
    product: ProcessingProduct::Vnp46A2,
    cadence: ProductCadence::Daily,
    radiance_dataset: DAILY_RADIANCE_DATASET,
    quality_dataset: DAILY_QUALITY_DATASET,
    cloud_dataset: Some(DAILY_CLOUD_DATASET),
    observation_count_dataset: None,
    radiance_fill_value: -999.9,
    radiance_valid_min: 0.0,
    radiance_scale_factor: 1.0,
    radiance_offset: 0.0,
    radiance_units: "nWatts/(cm^2 sr)",
    quality_fill_value: 255,
    cloud_fill_value: Some(65535),
    observation_count_fill_value: None,
};

static VJ146A2_MAPPING: DatasetMapping = DatasetMapping {
    product: ProcessingProduct::Vj146A2,
    cadence: ProductCadence::Daily,
    radiance_dataset: DAILY_RADIANCE_DATASET,
    quality_dataset: DAILY_QUALITY_DATASET,
    cloud_dataset: Some(DAILY_CLOUD_DATASET),
    observation_count_dataset: None,
    radiance_fill_value: -999.9,
    radiance_valid_min: 0.0,
    radiance_scale_factor: 1.0,
    radiance_offset: 0.0,
    radiance_units: "nWatts/(cm^2 sr)",
    quality_fill_value: 255,
    cloud_fill_value: Some(65535),
    observation_count_fill_value: None,
};

static VNP46A3_MAPPING: DatasetMapping = DatasetMapping {
    product: ProcessingProduct::Vnp46A3,
    cadence: ProductCadence::Monthly,
    radiance_dataset: MONTHLY_RADIANCE_DATASET,
    quality_dataset: MONTHLY_QUALITY_DATASET,
    cloud_dataset: None,
    observation_count_dataset: Some(MONTHLY_OBSERVATION_COUNT_DATASET),
    radiance_fill_value: -999.9,
    radiance_valid_min: 0.0,
    radiance_scale_factor: 1.0,
    radiance_offset: 0.0,
    radiance_units: "nWatts/(cm^2 sr)",
    quality_fill_value: 255,
    cloud_fill_value: None,
    observation_count_fill_value: Some(65535),
};

#[cfg(test)]
mod tests {
    use super::{
        dataset_mapping_for_product, ProductCadence, DAILY_CLOUD_DATASET, DAILY_QUALITY_DATASET,
        DAILY_RADIANCE_DATASET, MONTHLY_OBSERVATION_COUNT_DATASET, MONTHLY_QUALITY_DATASET,
        MONTHLY_RADIANCE_DATASET,
    };
    use shared::processing_message::ProcessingProduct;

    #[test]
    fn maps_vnp46a2_daily_datasets() {
        let mapping = dataset_mapping_for_product(ProcessingProduct::Vnp46A2);

        assert_eq!(mapping.product, ProcessingProduct::Vnp46A2);
        assert_eq!(mapping.cadence, ProductCadence::Daily);
        assert_eq!(mapping.radiance_dataset, DAILY_RADIANCE_DATASET);
        assert_eq!(mapping.quality_dataset, DAILY_QUALITY_DATASET);
        assert_eq!(mapping.cloud_dataset, Some(DAILY_CLOUD_DATASET));
        assert_eq!(mapping.observation_count_dataset, None);
        assert_eq!(mapping.radiance_fill_value, -999.9);
        assert_eq!(mapping.radiance_valid_min, 0.0);
        assert_eq!(mapping.radiance_scale_factor, 1.0);
        assert_eq!(mapping.radiance_offset, 0.0);
        assert_eq!(mapping.radiance_units, "nWatts/(cm^2 sr)");
        assert_eq!(mapping.quality_fill_value, 255);
        assert_eq!(mapping.cloud_fill_value, Some(65535));
        assert_eq!(mapping.observation_count_fill_value, None);
    }

    #[test]
    fn maps_vj146a2_daily_datasets() {
        let mapping = dataset_mapping_for_product(ProcessingProduct::Vj146A2);

        assert_eq!(mapping.product, ProcessingProduct::Vj146A2);
        assert_eq!(mapping.cadence, ProductCadence::Daily);
        assert_eq!(mapping.radiance_dataset, DAILY_RADIANCE_DATASET);
        assert_eq!(mapping.quality_dataset, DAILY_QUALITY_DATASET);
        assert_eq!(mapping.cloud_dataset, Some(DAILY_CLOUD_DATASET));
        assert_eq!(mapping.observation_count_dataset, None);
    }

    #[test]
    fn maps_vnp46a3_monthly_datasets() {
        let mapping = dataset_mapping_for_product(ProcessingProduct::Vnp46A3);

        assert_eq!(mapping.product, ProcessingProduct::Vnp46A3);
        assert_eq!(mapping.cadence, ProductCadence::Monthly);
        assert_eq!(mapping.radiance_dataset, MONTHLY_RADIANCE_DATASET);
        assert_eq!(mapping.quality_dataset, MONTHLY_QUALITY_DATASET);
        assert_eq!(mapping.cloud_dataset, None);
        assert_eq!(
            mapping.observation_count_dataset,
            Some(MONTHLY_OBSERVATION_COUNT_DATASET)
        );
        assert_eq!(mapping.observation_count_fill_value, Some(65535));
    }
}
