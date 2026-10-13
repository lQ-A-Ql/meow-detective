use serde::{Deserialize, Serialize};

use super::{RegistryBrowserCoverageDto, RegistryBrowserDiagnosticDto, RegistryItemReadStateDto};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryValueMetadataDto {
    pub value_id: String,
    /// An empty name represents an existing default value, not an absent value.
    pub name: String,
    pub type_code: u32,
    pub type_name: String,
    pub byte_length: u64,
    pub preview: String,
    pub cell_offset: u32,
    pub read_state: RegistryItemReadStateDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryValuesRequestDto {
    pub session_id: String,
    pub key_id: String,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryValuePageDto {
    pub values: Vec<RegistryValueMetadataDto>,
    pub coverage: RegistryBrowserCoverageDto,
    pub diagnostics: Vec<RegistryBrowserDiagnosticDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
