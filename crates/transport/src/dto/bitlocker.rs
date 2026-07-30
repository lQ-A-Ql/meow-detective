use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitLockerProtectorDto {
    pub code: u16,
    pub kind: String,
    pub label: String,
    pub unlockable: bool,
}

/// Outcome of reconstructing the numerical recovery password from a
/// memory-recovered VMK. Carried only in the transient unlock command
/// response: it is never persisted, logged, or included in reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryPasswordReconstructionDto {
    /// `recovered` | `unavailable`
    pub status: String,
    /// The 48-digit recovery password, present only when `status` is
    /// `recovered`. This is a live secret revealed to the investigator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protector_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse_datum_fingerprint: Option<String>,
    /// Why reconstruction was not possible (e.g. the active VMK does not
    /// authenticate the recovery protector's reverse datum).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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
    /// Present only on memory-image unlock responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_password_reconstruction: Option<RecoveryPasswordReconstructionDto>,
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
