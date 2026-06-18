use std::{io, process::ExitStatus};

#[derive(Debug, thiserror::Error)]
pub(crate) enum HdfCliError {
    #[error(
        "gdalinfo failed for HDF subdataset '{subdataset_name}' with status {status}: {stderr}"
    )]
    GdalInfoFailed {
        subdataset_name: String,
        status: ExitStatus,
        stderr: String,
    },
    #[error("gdalinfo returned invalid JSON for HDF subdataset '{subdataset_name}': {source}")]
    InvalidJson {
        subdataset_name: String,
        source: serde_json::Error,
    },
    #[error(
        "gdal_translate returned invalid XYZ output for HDF subdataset '{subdataset_name}': {line}"
    )]
    InvalidXyz {
        subdataset_name: String,
        line: String,
    },
    #[error("{executable} executable was not found or could not be started: {source}")]
    MissingGdalCli {
        executable: &'static str,
        source: io::Error,
    },
    #[error("gdalinfo JSON did not include raster size for HDF subdataset '{subdataset_name}'")]
    MissingRasterSize { subdataset_name: String },
}
