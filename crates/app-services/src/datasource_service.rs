mod attach;
mod bluestore;
mod fs_magic;
mod lvm;
mod partition_index;
mod probe;
mod reader;
mod types;

pub use attach::{attach_data_source, attach_data_source_with_storage, classify_data_source_path};
pub fn list_local_disks() -> Result<Vec<evidence_core::LocalDiskInfo>> {
    evidence_core::list_local_disks().map_err(DataSourceError::Io)
}
pub(crate) use bluestore::has_bluestore_label;
pub(crate) use fs_magic::read_boot_filesystem;
pub use lvm::{expand_lvm_pool_candidates, expand_lvm_pool_candidates_with_sources};
pub(crate) use lvm::{lvm_source_fingerprint, normalize_lvm_uuid_for_match};
pub use partition_index::{assign_effective_partition_indices, effective_partition_index};
pub use probe::{detect_image_filesystem, partition_display_name, volume_display_name};
pub(crate) use reader::open_evidence_reader;
pub use types::{
    DataSourceError, ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe,
    ImageFilesystemSource, LvmDiscoverySource, LvmLogicalVolumeIdentity, LvmPhysicalVolumeSource,
    PartitionRecord, PartitionStatus, Result, UnsupportedImageKind, UnsupportedImageVolume,
};

#[cfg(test)]
#[path = "../tests/unit/datasource_service.rs"]
mod tests;
