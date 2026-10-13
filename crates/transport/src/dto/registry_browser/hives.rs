use serde::{Deserialize, Serialize};

use super::{RegistryBrowserCoverageDto, RegistryBrowserDiagnosticDto};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryHiveReadStateDto {
    Candidate,
    Readable,
    Locked,
    Unsupported,
    Corrupt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHiveDescriptorDto {
    pub file_id: String,
    pub data_source_id: String,
    pub name: String,
    /// Path inside the evidence source, never a host filesystem path.
    pub evidence_path: String,
    pub byte_length: u64,
    pub read_state: RegistryHiveReadStateDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHiveListRequestDto {
    pub data_source_id: String,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHivePageDto {
    pub hives: Vec<RegistryHiveDescriptorDto>,
    pub coverage: RegistryBrowserCoverageDto,
    pub diagnostics: Vec<RegistryBrowserDiagnosticDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
