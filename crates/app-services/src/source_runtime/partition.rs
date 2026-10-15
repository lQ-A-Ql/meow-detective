use crate::datasource_service::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemSource, LvmLogicalVolumeIdentity,
};
use domain::DataSourceKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionReadPlan {
    pub partition_index: usize,
    pub offset: u64,
    pub length: Option<u64>,
    pub filesystem: ImageFilesystemKind,
    pub source: ImageFilesystemSource,
    pub lvm_identity: Option<LvmLogicalVolumeIdentity>,
    pub source_kind: DataSourceKind,
}

impl PartitionReadPlan {
    pub fn from_candidate(
        index: usize,
        candidate: &ImageFilesystemCandidate,
        source_kind: DataSourceKind,
    ) -> Self {
        Self {
            partition_index: index,
            offset: candidate.offset,
            length: candidate.length,
            filesystem: candidate.kind,
            source: candidate.source,
            lvm_identity: candidate.lvm_identity.clone(),
            source_kind,
        }
    }
    pub fn is_direct_ntfs(&self) -> bool {
        self.filesystem == ImageFilesystemKind::Ntfs
            && self.lvm_identity.is_none()
            && matches!(
                self.source,
                ImageFilesystemSource::DirectVolume
                    | ImageFilesystemSource::MbrPartition
                    | ImageFilesystemSource::GptPartition
            )
    }
}
