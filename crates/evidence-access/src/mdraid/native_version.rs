use domain::structure::{MdMetadataVersion, NativeVersion};
use fs_lvm::mdraid::{MdRaidMetadataVersion, MdRaidSuperblock};

pub fn mdraid_native_version(superblock: &MdRaidSuperblock) -> NativeVersion {
    let metadata_version = match superblock.metadata_version {
        MdRaidMetadataVersion::V0_90 => MdMetadataVersion::V0_90,
        MdRaidMetadataVersion::V1_0 => MdMetadataVersion::V1_0,
        MdRaidMetadataVersion::V1_1 => MdMetadataVersion::V1_1,
        MdRaidMetadataVersion::V1_2 => MdMetadataVersion::V1_2,
    };
    NativeVersion::Md {
        metadata_version,
        events: superblock.events,
    }
}
