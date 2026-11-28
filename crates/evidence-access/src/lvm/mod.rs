mod descriptor;
mod discovery;
mod filesystem;
mod identity;
pub(crate) mod mapping_cache;
pub use mapping_cache::mapping_discovery_count;
pub(crate) mod persisted_inputs;
mod plan;
mod reader;

pub use descriptor::{lvm_descriptor_path, read_lvm_descriptor, write_lvm_descriptor};
pub use discovery::discover_volumes_from_collection;
pub use filesystem::{open_lvm_filesystem, open_lvm_partition, probe_lvm_filesystems};
pub use identity::registered_source_id;
pub use plan::{
    lvm_native_version, lvm_volume_fingerprint, LvmPlanError, LvmVolumePlan, LvmVolumeSourcePlan,
    LVM_ADAPTER_VERSION, LVM_POLICY_VERSION,
};
pub use reader::{
    lvm_reader_content_bytes, open_lvm_reader, open_lvm_volume_reader, verify_lvm_source_content,
    LvmVolumeReader,
};
