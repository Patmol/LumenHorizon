use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::models;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocalGranuleWorkspace {
    root: PathBuf,
    granule_path: PathBuf,
}

impl LocalGranuleWorkspace {
    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn granule_path(&self) -> &Path {
        &self.granule_path
    }
}

pub(super) fn local_granule_workspace(
    processing_message: &models::ProcessingMessage,
    correlation_id: Uuid,
) -> LocalGranuleWorkspace {
    let root = std::env::temp_dir()
        .join("lumenhorizon-processing")
        .join(processing_message.ingest_id.to_string())
        .join(correlation_id.to_string());
    let granule_path = root.join(
        Path::new(&processing_message.blob_path)
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("granule.h5")),
    );

    LocalGranuleWorkspace { root, granule_path }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn workspace_path_is_attempt_scoped_and_uses_blob_filename() {
        let ingest_id = Uuid::parse_str("00000000-0000-0000-0000-00000000abcd").unwrap();
        let correlation_id = Uuid::parse_str("00000000-0000-0000-0000-00000000c0de").unwrap();
        let message = models::ProcessingMessage::new(
            ingest_id,
            "VNP46A2/2026-05-21/h11v06.h5",
            "VNP46A2",
            Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap(),
            11,
            6,
        )
        .unwrap();

        let workspace = local_granule_workspace(&message, correlation_id);

        assert!(workspace
            .root()
            .ends_with(format!("{ingest_id}/{correlation_id}")));
        assert!(workspace.granule_path().ends_with("h11v06.h5"));
        assert!(workspace.granule_path().starts_with(workspace.root()));
    }
}
