use std::{io, path::Path, process::Command};

use crate::science::DatasetMapping;

use super::{error::HdfCliError, types::GdalInfoOutput, types::RasterShape};

pub(crate) fn radiance_shape(
    granule_path: &Path,
    mapping: &DatasetMapping,
) -> Result<RasterShape, HdfCliError> {
    dataset_shape(granule_path, mapping.radiance_dataset)
}

pub(crate) fn dataset_shape(
    granule_path: &Path,
    dataset_path: &str,
) -> Result<RasterShape, HdfCliError> {
    let subdataset_name = hdf5_subdataset_name(granule_path, dataset_path);

    dataset_shape_with_runner(&subdataset_name, run_gdalinfo_json)
}

pub(crate) fn verify_gdalinfo_available() -> Result<(), HdfCliError> {
    verify_gdalinfo_available_with_runner(run_gdalinfo_version)
}

fn verify_gdalinfo_available_with_runner(
    runner: impl FnOnce() -> Result<GdalInfoOutput, io::Error>,
) -> Result<(), HdfCliError> {
    let output = runner().map_err(|source| HdfCliError::MissingGdalCli {
        executable: "gdalinfo",
        source,
    })?;

    if !output.status.success() {
        return Err(HdfCliError::GdalInfoFailed {
            subdataset_name: "gdalinfo --version".to_owned(),
            status: output.status,
            stderr: output.stderr,
        });
    }

    Ok(())
}

fn run_gdalinfo_version() -> Result<GdalInfoOutput, io::Error> {
    let output = Command::new("gdalinfo").arg("--version").output()?;

    Ok(GdalInfoOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn dataset_shape_with_runner(
    subdataset_name: &str,
    runner: impl FnOnce(&str) -> Result<GdalInfoOutput, io::Error>,
) -> Result<RasterShape, HdfCliError> {
    let output = runner(subdataset_name).map_err(|source| HdfCliError::MissingGdalCli {
        executable: "gdalinfo",
        source,
    })?;

    if !output.status.success() {
        return Err(HdfCliError::GdalInfoFailed {
            subdataset_name: subdataset_name.to_owned(),
            status: output.status,
            stderr: output.stderr,
        });
    }

    parse_raster_shape(subdataset_name, &output.stdout)
}

fn run_gdalinfo_json(subdataset_name: &str) -> Result<GdalInfoOutput, io::Error> {
    let output = Command::new("gdalinfo")
        .arg("-json")
        .arg(subdataset_name)
        .output()?;

    Ok(GdalInfoOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn parse_raster_shape(subdataset_name: &str, stdout: &str) -> Result<RasterShape, HdfCliError> {
    let value: serde_json::Value =
        serde_json::from_str(stdout).map_err(|source| HdfCliError::InvalidJson {
            subdataset_name: subdataset_name.to_owned(),
            source,
        })?;

    let size = value
        .get("size")
        .and_then(serde_json::Value::as_array)
        .filter(|size| size.len() == 2)
        .ok_or_else(|| HdfCliError::MissingRasterSize {
            subdataset_name: subdataset_name.to_owned(),
        })?;

    let width = size[0]
        .as_u64()
        .ok_or_else(|| HdfCliError::MissingRasterSize {
            subdataset_name: subdataset_name.to_owned(),
        })?;
    let height = size[1]
        .as_u64()
        .ok_or_else(|| HdfCliError::MissingRasterSize {
            subdataset_name: subdataset_name.to_owned(),
        })?;

    Ok(RasterShape {
        width: width as usize,
        height: height as usize,
    })
}

pub(super) fn hdf5_subdataset_name(granule_path: &Path, subdataset_path: &str) -> String {
    format!("HDF5:\"{}\":{}", granule_path.display(), subdataset_path)
}

#[cfg(test)]
mod tests {
    use std::{io, os::unix::process::ExitStatusExt, path::Path, process::ExitStatus};

    use super::*;

    fn status(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn builds_hdf5_subdataset_name() {
        let name = hdf5_subdataset_name(
            Path::new("/tmp/sample.h5"),
            "//HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Gap_Filled_DNB_BRDF-Corrected_NTL",
        );

        assert_eq!(
            name,
            "HDF5:\"/tmp/sample.h5\"://HDFEOS/GRIDS/VIIRS_Grid_DNB_2d/Data_Fields/Gap_Filled_DNB_BRDF-Corrected_NTL"
        );
    }

    #[test]
    fn parses_raster_shape_from_gdalinfo_json() {
        let shape = parse_raster_shape("sample", r#"{ "size": [2400, 2400] }"#).unwrap();

        assert_eq!(
            shape,
            RasterShape {
                width: 2400,
                height: 2400
            }
        );
    }

    #[test]
    fn runner_success_returns_raster_shape() {
        let shape = dataset_shape_with_runner("sample", |_| {
            Ok(GdalInfoOutput {
                status: status(0),
                stdout: r#"{ "size": [1200, 800] }"#.to_owned(),
                stderr: String::new(),
            })
        })
        .unwrap();

        assert_eq!(
            shape,
            RasterShape {
                width: 1200,
                height: 800
            }
        );
    }

    #[test]
    fn missing_gdalinfo_returns_clear_error() {
        let error = dataset_shape_with_runner("sample", |_| {
            Err(io::Error::new(io::ErrorKind::NotFound, "gdalinfo"))
        })
        .unwrap_err();

        assert!(matches!(
            error,
            HdfCliError::MissingGdalCli {
                executable: "gdalinfo",
                ..
            }
        ));
        assert!(error.to_string().contains("gdalinfo executable"));
    }

    #[test]
    fn non_zero_gdalinfo_exit_returns_clear_error() {
        let error = dataset_shape_with_runner("sample", |_| {
            Ok(GdalInfoOutput {
                status: status(1),
                stdout: String::new(),
                stderr: "cannot open dataset".to_owned(),
            })
        })
        .unwrap_err();

        assert!(matches!(
            error,
            HdfCliError::GdalInfoFailed {
                subdataset_name,
                stderr,
                ..
            } if subdataset_name == "sample" && stderr == "cannot open dataset"
        ));
    }

    #[test]
    fn malformed_json_returns_clear_error() {
        let error = parse_raster_shape("sample", "not json").unwrap_err();

        assert!(matches!(error, HdfCliError::InvalidJson { .. }));
    }

    #[test]
    fn missing_size_returns_clear_error() {
        let error = parse_raster_shape("sample", r#"{ "driver": "HDF5" }"#).unwrap_err();

        assert!(matches!(error, HdfCliError::MissingRasterSize { .. }));
    }

    #[test]
    fn preflight_succeeds_when_gdalinfo_version_succeeds() {
        verify_gdalinfo_available_with_runner(|| {
            Ok(GdalInfoOutput {
                status: status(0),
                stdout: "GDAL 3.13.1".to_owned(),
                stderr: String::new(),
            })
        })
        .unwrap();
    }

    #[test]
    fn preflight_reports_missing_gdalinfo() {
        let error = verify_gdalinfo_available_with_runner(|| {
            Err(io::Error::new(io::ErrorKind::NotFound, "gdalinfo"))
        })
        .unwrap_err();

        assert!(matches!(
            error,
            HdfCliError::MissingGdalCli {
                executable: "gdalinfo",
                ..
            }
        ));
    }

    #[test]
    fn preflight_reports_non_zero_gdalinfo_version() {
        let error = verify_gdalinfo_available_with_runner(|| {
            Ok(GdalInfoOutput {
                status: status(1),
                stdout: String::new(),
                stderr: "broken gdal install".to_owned(),
            })
        })
        .unwrap_err();

        assert!(matches!(
            error,
            HdfCliError::GdalInfoFailed {
                subdataset_name,
                stderr,
                ..
            } if subdataset_name == "gdalinfo --version" && stderr == "broken gdal install"
        ));
    }
}
