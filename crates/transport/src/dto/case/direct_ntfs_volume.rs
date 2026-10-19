//! Case-source geometry of an NTFS volume captured without its disk layout.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectNtfsVolumeDto {
    pub bytes_per_sector: u32,
    pub logical_media_bytes: u64,
    pub declared_volume_bytes: u64,
    /// Includes a matching backup VBR; absent when only the primary is verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_extent_bytes: Option<u64>,
    /// NTFS serial, never a fabricated GPT partition UUID.
    pub volume_serial: String,
}
