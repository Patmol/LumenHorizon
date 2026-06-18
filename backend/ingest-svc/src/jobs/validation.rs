const HDF5_MAGIC: &[u8; 8] = b"\x89HDF\r\n\x1A\n";

pub(super) fn validate_raw_granule_bytes(bytes: &[u8]) -> Result<(), RawGranuleValidationError> {
    if bytes.len() < HDF5_MAGIC.len() {
        return Err(RawGranuleValidationError::TooSmall {
            actual_size: bytes.len(),
            minimum_size: HDF5_MAGIC.len(),
        });
    }

    if !bytes.starts_with(HDF5_MAGIC) {
        return Err(RawGranuleValidationError::MissingHdf5Magic);
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum RawGranuleValidationError {
    #[error("raw granule rejected: missing HDF5 magic number")]
    MissingHdf5Magic,
    #[error(
        "raw granule rejected: file is {actual_size} bytes; expected at least {minimum_size} bytes"
    )]
    TooSmall {
        actual_size: usize,
        minimum_size: usize,
    },
}
