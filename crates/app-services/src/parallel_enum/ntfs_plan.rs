use crate::datasource_service::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemSource,
};
use domain::DataSourceKind;
use evidence_core::FileSystemReader;

use super::{error::ParallelEnumError, PartitionWork};

pub enum PartitionEnumStrategy {
    Generic(Box<dyn FileSystemReader + Send>),
    NtfsDeferred(NtfsPartitionPlan),
}

/// Preserve the entire candidate; offsets alone cannot describe LVM or other views.
#[derive(Debug, Clone)]
pub struct NtfsPartitionPlan {
    pub candidate: ImageFilesystemCandidate,
}

impl NtfsPartitionPlan {
    pub(crate) fn eligible(
        source_kind: &DataSourceKind,
        candidate: &ImageFilesystemCandidate,
    ) -> bool {
        matches!(source_kind, DataSourceKind::Raw | DataSourceKind::E01)
            && candidate.kind == ImageFilesystemKind::Ntfs
            && matches!(
                candidate.source,
                ImageFilesystemSource::DirectVolume
                    | ImageFilesystemSource::MbrPartition
                    | ImageFilesystemSource::GptPartition
            )
            && candidate.lvm_identity.is_none()
    }

    pub(super) fn validate(&self, work: &PartitionWork) -> Result<(), ParallelEnumError> {
        if !Self::eligible(&work.source_kind, &self.candidate)
            || !work.fs_kind.eq_ignore_ascii_case("ntfs")
            || work.volume_offset != self.candidate.offset
        {
            return Err(ParallelEnumError::InvalidPlan);
        }
        Ok(())
    }
}
