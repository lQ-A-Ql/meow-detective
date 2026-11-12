//! Data-source attachment, LVM expansion, and the partition-probe facade.
//!
//! The partition probe model lives in `source-kernel::partition_probe` so the
//! lower storage kernel can detect filesystems without depending on this
//! application-service crate. The re-exports below keep the historical
//! `crate::datasource_service::*` paths stable.

mod attach;
mod lvm;
mod reader;

pub use attach::{attach_data_source, attach_data_source_with_storage, classify_data_source_path};
pub use bluestore::has_bluestore_label;
pub use fs_magic::{is_exfat_boot_sector, read_boot_filesystem};
pub fn list_local_disks() -> Result<Vec<evidence_core::LocalDiskInfo>> {
    evidence_core::list_local_disks().map_err(DataSourceError::Io)
}
pub use lvm::{expand_lvm_pool_candidates, expand_lvm_pool_candidates_with_sources};
pub use lvm::{lvm_source_fingerprint, normalize_lvm_uuid_for_match};
pub use partition_index::{assign_effective_partition_indices, effective_partition_index};
pub use probe::identity::read_filesystem_uuid;
pub use probe::{detect_image_filesystem, partition_display_name, volume_display_name};
pub use reader::open_evidence_reader;
pub use source_kernel::partition_probe::{
    bluestore, direct_ntfs_volume, fs_magic, partition_index, probe, types,
};
pub use types::{
    DataSourceError, ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe,
    ImageFilesystemSource, LvmDiscoverySource, LvmLogicalVolumeIdentity, LvmPhysicalVolumeSource,
    PartitionRecord, PartitionStatus, Result, UnsupportedImageKind, UnsupportedImageVolume,
};

#[cfg(test)]
#[path = "../tests/unit/datasource_service.rs"]
mod tests;
