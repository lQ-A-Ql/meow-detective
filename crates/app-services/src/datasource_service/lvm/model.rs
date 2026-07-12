use super::super::{ImageFilesystemCandidate, LvmPhysicalVolumeSource};
use std::collections::HashSet;

#[derive(Clone)]
pub(super) struct LvmPvDiscoveryInfo {
    pub(super) source: LvmPhysicalVolumeSource,
    pub(super) label: fs_lvm::LvmLabel,
    pub(super) volume_group: Option<fs_lvm::VolumeGroup>,
    pub(super) metadata_warnings: Vec<String>,
}

pub(super) struct LvmMetadataGroup {
    pub(super) volume_group: fs_lvm::VolumeGroup,
}

#[derive(Default)]
pub(super) struct LvmExpansionState {
    pub(super) new_candidates: Vec<(ImageFilesystemCandidate, u64)>,
    pub(super) remove_indices: Vec<usize>,
    pub(super) expanded_vgs: HashSet<String>,
}

pub(super) struct ExpandedPoolSources {
    pub(super) sources: Vec<LvmPhysicalVolumeSource>,
    pub(super) offsets: Vec<u64>,
    pub(super) primary_offsets: Vec<u64>,
    pub(super) candidate_offset: u64,
    pub(super) seed_offset: u64,
}
