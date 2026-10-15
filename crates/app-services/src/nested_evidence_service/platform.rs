use crate::datasource_service::{detect_image_filesystem, ImageFilesystemKind};
use std::path::Path;
pub(super) fn detect(path: &Path) -> Result<&'static str, persistence_sqlite::DbError> {
    let mut reader = evidence_core::RawImageReader::open(path)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let probe = detect_image_filesystem(&mut reader)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let linux = probe.candidates.iter().any(|candidate| {
        matches!(
            candidate.kind,
            ImageFilesystemKind::Ext4
                | ImageFilesystemKind::Xfs
                | ImageFilesystemKind::Btrfs
                | ImageFilesystemKind::LvmPool
                | ImageFilesystemKind::F2fs
                | ImageFilesystemKind::Erofs
        )
    });
    let windows = probe.candidates.iter().any(|candidate| {
        matches!(
            candidate.kind,
            ImageFilesystemKind::Ntfs | ImageFilesystemKind::BitLocker
        )
    });
    Ok(if linux && !windows {
        "linux"
    } else {
        "windows"
    })
}
