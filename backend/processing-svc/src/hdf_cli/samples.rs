use std::{io, path::Path, process::Command};

use super::{
    error::HdfCliError,
    metadata::hdf5_subdataset_name,
    types::{GdalInfoOutput, RasterOutputSize, RasterSample, RasterWindow},
};

pub(crate) fn dataset_window_samples(
    granule_path: &Path,
    dataset_path: &str,
    window: RasterWindow,
) -> Result<Vec<RasterSample>, HdfCliError> {
    let subdataset_name = hdf5_subdataset_name(granule_path, dataset_path);

    dataset_window_samples_with_runner(&subdataset_name, window, run_gdal_translate_xyz)
}

pub(crate) fn dataset_window_samples_resized(
    granule_path: &Path,
    dataset_path: &str,
    window: RasterWindow,
    output_size: RasterOutputSize,
) -> Result<Vec<RasterSample>, HdfCliError> {
    let subdataset_name = hdf5_subdataset_name(granule_path, dataset_path);

    dataset_window_samples_resized_with_runner(
        &subdataset_name,
        window,
        output_size,
        run_gdal_translate_xyz_resized,
    )
}

fn dataset_window_samples_with_runner(
    subdataset_name: &str,
    window: RasterWindow,
    runner: impl FnOnce(&str, RasterWindow) -> Result<GdalInfoOutput, io::Error>,
) -> Result<Vec<RasterSample>, HdfCliError> {
    let output = runner(subdataset_name, window).map_err(|source| HdfCliError::MissingGdalCli {
        executable: "gdal_translate",
        source,
    })?;

    if !output.status.success() {
        return Err(HdfCliError::GdalInfoFailed {
            subdataset_name: subdataset_name.to_owned(),
            status: output.status,
            stderr: output.stderr,
        });
    }

    parse_xyz_samples(subdataset_name, &output.stdout)
}

fn run_gdal_translate_xyz_resized(
    subdataset_name: &str,
    window: RasterWindow,
    output_size: RasterOutputSize,
) -> Result<GdalInfoOutput, io::Error> {
    let output = Command::new("gdal_translate")
        .arg("-of")
        .arg("XYZ")
        .arg("-srcwin")
        .arg(window.x_offset.to_string())
        .arg(window.y_offset.to_string())
        .arg(window.width.to_string())
        .arg(window.height.to_string())
        .arg("-outsize")
        .arg(output_size.width.to_string())
        .arg(output_size.height.to_string())
        .arg(subdataset_name)
        .arg("/vsistdout/")
        .output()?;

    Ok(GdalInfoOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn dataset_window_samples_resized_with_runner(
    subdataset_name: &str,
    window: RasterWindow,
    output_size: RasterOutputSize,
    runner: impl FnOnce(&str, RasterWindow, RasterOutputSize) -> Result<GdalInfoOutput, io::Error>,
) -> Result<Vec<RasterSample>, HdfCliError> {
    let output = runner(subdataset_name, window, output_size).map_err(|source| {
        HdfCliError::MissingGdalCli {
            executable: "gdal_translate",
            source,
        }
    })?;

    if !output.status.success() {
        return Err(HdfCliError::GdalInfoFailed {
            subdataset_name: subdataset_name.to_owned(),
            status: output.status,
            stderr: output.stderr,
        });
    }

    parse_xyz_samples(subdataset_name, &output.stdout)
}

fn run_gdal_translate_xyz(
    subdataset_name: &str,
    window: RasterWindow,
) -> Result<GdalInfoOutput, io::Error> {
    let output = Command::new("gdal_translate")
        .arg("-of")
        .arg("XYZ")
        .arg("-srcwin")
        .arg(window.x_offset.to_string())
        .arg(window.y_offset.to_string())
        .arg(window.width.to_string())
        .arg(window.height.to_string())
        .arg(subdataset_name)
        .arg("/vsistdout/")
        .output()?;

    Ok(GdalInfoOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn parse_xyz_samples(
    subdataset_name: &str,
    stdout: &str,
) -> Result<Vec<RasterSample>, HdfCliError> {
    let mut samples = Vec::new();

    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let parts = line.split_whitespace().collect::<Vec<_>>();

        if parts.len() != 3 {
            return Err(HdfCliError::InvalidXyz {
                subdataset_name: subdataset_name.to_owned(),
                line: line.to_owned(),
            });
        }

        let x = parts[0]
            .parse::<f64>()
            .map_err(|_| HdfCliError::InvalidXyz {
                subdataset_name: subdataset_name.to_owned(),
                line: line.to_owned(),
            })?;
        let y = parts[1]
            .parse::<f64>()
            .map_err(|_| HdfCliError::InvalidXyz {
                subdataset_name: subdataset_name.to_owned(),
                line: line.to_owned(),
            })?;
        let value = parts[2]
            .parse::<f32>()
            .map_err(|_| HdfCliError::InvalidXyz {
                subdataset_name: subdataset_name.to_owned(),
                line: line.to_owned(),
            })?;

        samples.push(RasterSample { x, y, value });
    }

    Ok(samples)
}

#[cfg(test)]
mod tests {
    use std::{io, os::unix::process::ExitStatusExt, process::ExitStatus};

    use super::*;

    fn status(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn resized_window_runner_success_returns_samples() {
        let window = RasterWindow {
            x_offset: 10,
            y_offset: 20,
            width: 30,
            height: 40,
        };
        let output_size = RasterOutputSize {
            width: 256,
            height: 256,
        };

        let samples = dataset_window_samples_resized_with_runner(
            "sample",
            window,
            output_size,
            |subdataset_name, window, output_size| {
                assert_eq!(subdataset_name, "sample");
                assert_eq!(
                    window,
                    RasterWindow {
                        x_offset: 10,
                        y_offset: 20,
                        width: 30,
                        height: 40,
                    }
                );
                assert_eq!(
                    output_size,
                    RasterOutputSize {
                        width: 256,
                        height: 256,
                    }
                );

                Ok(GdalInfoOutput {
                    status: status(0),
                    stdout: "0.0 0.0 1.25\n1.0 0.0 2.5\n".to_owned(),
                    stderr: String::new(),
                })
            },
        )
        .unwrap();

        assert_eq!(
            samples,
            vec![
                RasterSample {
                    x: 0.0,
                    y: 0.0,
                    value: 1.25,
                },
                RasterSample {
                    x: 1.0,
                    y: 0.0,
                    value: 2.5,
                },
            ]
        );
    }

    #[test]
    fn parses_xyz_samples() {
        let samples = parse_xyz_samples("sample", "10.5 20.25 1.5\n11.5 21.25 2.25\n").unwrap();

        assert_eq!(
            samples,
            vec![
                RasterSample {
                    x: 10.5,
                    y: 20.25,
                    value: 1.5,
                },
                RasterSample {
                    x: 11.5,
                    y: 21.25,
                    value: 2.25,
                },
            ]
        );
    }

    #[test]
    fn invalid_xyz_returns_clear_error() {
        let error = parse_xyz_samples("sample", "10.5 20.25 not-a-number").unwrap_err();

        assert!(matches!(
            error,
            HdfCliError::InvalidXyz {
                subdataset_name,
                line,
            } if subdataset_name == "sample" && line == "10.5 20.25 not-a-number"
        ));
    }

    #[test]
    fn window_runner_success_returns_samples() {
        let window = RasterWindow {
            x_offset: 3,
            y_offset: 4,
            width: 2,
            height: 1,
        };

        let samples =
            dataset_window_samples_with_runner("sample", window, |subdataset_name, window| {
                assert_eq!(subdataset_name, "sample");
                assert_eq!(
                    window,
                    RasterWindow {
                        x_offset: 3,
                        y_offset: 4,
                        width: 2,
                        height: 1,
                    }
                );

                Ok(GdalInfoOutput {
                    status: status(0),
                    stdout: "100.0 200.0 1.25\n101.0 200.0 2.5\n".to_owned(),
                    stderr: String::new(),
                })
            })
            .unwrap();

        assert_eq!(
            samples,
            vec![
                RasterSample {
                    x: 100.0,
                    y: 200.0,
                    value: 1.25,
                },
                RasterSample {
                    x: 101.0,
                    y: 200.0,
                    value: 2.5,
                },
            ]
        );
    }

    #[test]
    fn window_runner_missing_gdal_translate_returns_clear_error() {
        let window = RasterWindow {
            x_offset: 0,
            y_offset: 0,
            width: 1,
            height: 1,
        };

        let error = dataset_window_samples_with_runner("sample", window, |_, _| {
            Err(io::Error::new(io::ErrorKind::NotFound, "gdal_translate"))
        })
        .unwrap_err();

        assert!(matches!(
            error,
            HdfCliError::MissingGdalCli {
                executable: "gdal_translate",
                ..
            }
        ));
        assert!(error.to_string().contains("gdal_translate executable"));
    }

    #[test]
    fn window_runner_non_zero_exit_returns_clear_error() {
        let window = RasterWindow {
            x_offset: 0,
            y_offset: 0,
            width: 1,
            height: 1,
        };

        let error = dataset_window_samples_with_runner("sample", window, |_, _| {
            Ok(GdalInfoOutput {
                status: status(1),
                stdout: String::new(),
                stderr: "cannot translate window".to_owned(),
            })
        })
        .unwrap_err();

        assert!(matches!(
            error,
            HdfCliError::GdalInfoFailed {
                subdataset_name,
                stderr,
                ..
            } if subdataset_name == "sample" && stderr == "cannot translate window"
        ));
    }
}
