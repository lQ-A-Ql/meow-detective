//! Provenance for a generated guest disk, distinct from the original evidence.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedNtfsDiskDto {
    pub layout_version: u32,
    pub source_media_bytes: u64,
    pub source_volume_offset: u64,
    pub source_volume_bytes: u64,
    pub source_volume_serial: String,
    pub guest_partition_index: u32,
    pub guest_partition_offset: u64,
    pub guest_disk_bytes: u64,
    pub generated_disk_signature: u32,
    pub source_hidden_sectors: u32,
    pub guest_hidden_sectors: u32,
    pub generated_mbr_sha256: String,
    pub boot_repair_required: bool,
}
