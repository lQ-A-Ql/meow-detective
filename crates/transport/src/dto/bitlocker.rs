use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitLockerProtectorDto {
    pub code: u16,
    pub kind: String,
    pub label: String,
    pub unlockable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitLockerVolumeStatusDto {
    pub data_source_id: String,
    pub partition_index: u32,
    pub unlocked: bool,
    pub encryption_method: String,
    pub encryption_method_code: u16,
    pub decryptable: bool,
    pub bytes_per_sector: u16,
    pub metadata_fingerprint: String,
    pub metadata_copy_count: u32,
    pub protectors: Vec<BitLockerProtectorDto>,
    pub supports_password: bool,
    pub supports_recovery_password: bool,
    pub stored_key_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plaintext_filesystem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitLockerCatalogImportDto {
    pub volume: BitLockerVolumeStatusDto,
    pub imported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/bitlocker.rs"]
mod tests;
