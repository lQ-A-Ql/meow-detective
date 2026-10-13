use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryRecoveryStatusDto {
    NotNeeded,
    Recovered,
    Partial,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryLogSlotDto {
    Log1,
    Log2,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryLogFormatDto {
    Hvle,
    Dirt,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryLogStatusDto {
    Applied,
    NotNeeded,
    Stale,
    Invalid,
    Unavailable,
    Duplicate,
    NotSelected,
    Missing,
    Ambiguous,
    Locked,
    TooLarge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryLogProvenanceDto {
    pub slot: RegistryLogSlotDto,
    pub status: RegistryLogStatusDto,
    pub applied_entries: u32,
    pub applied_pages: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<RegistryLogFormatDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_sequence: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sequence: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRecoveryDto {
    pub status: RegistryRecoveryStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_sha256: Option<String>,
    pub base_primary_sequence: u32,
    pub base_secondary_sequence: u32,
    pub base_checksum_valid: bool,
    pub applied_entries: u32,
    pub applied_pages: u32,
    pub logs: Vec<RegistryLogProvenanceDto>,
    pub diagnostic_codes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_sequence: Option<u32>,
}
