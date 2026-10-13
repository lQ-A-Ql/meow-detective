use serde::{Deserialize, Serialize};

use super::{RegistryBrowserCoverageDto, RegistryBrowserDiagnosticDto};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryItemReadStateDto {
    Readable,
    Deferred,
    Unsupported,
    Corrupt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryKeyMetadataDto {
    pub key_id: String,
    /// Path relative to this hive's root; the root is an empty string.
    pub path: String,
    pub name: String,
    /// HBIN-relative cell coordinate, not an absolute disk offset.
    pub cell_offset: u32,
    pub subkey_count: u32,
    pub value_count: u32,
    pub read_state: RegistryItemReadStateDto,
    /// Decimal FILETIME preserves all 100 ns ticks across JavaScript transport.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_write_filetime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryKeyResolveRequestDto {
    pub session_id: String,
    pub key_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySubkeysRequestDto {
    pub session_id: String,
    pub key_id: String,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySubkeyPageDto {
    pub keys: Vec<RegistryKeyMetadataDto>,
    pub coverage: RegistryBrowserCoverageDto,
    pub diagnostics: Vec<RegistryBrowserDiagnosticDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
