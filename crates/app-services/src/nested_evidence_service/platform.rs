use crate::datasource_service::{
    detect_image_filesystem, ImageFilesystemKind, ImageFilesystemProbe,
};

pub(super) struct DetectedPlatform {
    pub platform: &'static str,
    pub probe: ImageFilesystemProbe,
}

pub(super) fn detect(
    mut reader: &mut dyn evidence_core::EvidenceReader,
) -> Result<DetectedPlatform, persistence_sqlite::DbError> {
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
    let platform = if linux && !windows {
        "linux"
    } else {
        "windows"
    };
    Ok(DetectedPlatform { platform, probe })
}
