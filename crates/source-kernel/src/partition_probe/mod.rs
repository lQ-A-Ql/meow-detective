//! Business-neutral partition and filesystem probe model.
//!
//! The probe reads on-disk metadata only: it never mounts, never writes, and
//! never resolves application-level data-source state. Application services
//! re-export these items so existing `crate::datasource_service::*` paths stay
//! stable.

pub mod bluestore;
pub mod direct_ntfs_volume;
pub mod fs_magic;
pub mod partition_index;
pub mod probe;
pub mod types;

pub use bluestore::has_bluestore_label;
pub use partition_index::{assign_effective_partition_indices, effective_partition_index};
pub use probe::{detect_image_filesystem, partition_display_name, volume_display_name};
pub use types::{
    DataSourceError, ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe,
    ImageFilesystemSource, LvmDiscoverySource, LvmLogicalVolumeIdentity, LvmPhysicalVolumeSource,
    PartitionRecord, PartitionStatus, Result, UnsupportedImageKind, UnsupportedImageVolume,
};

#[cfg(test)]
#[path = "../../tests/unit/partition_probe.rs"]
mod tests;
