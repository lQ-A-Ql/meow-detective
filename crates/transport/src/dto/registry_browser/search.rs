use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchRequestDto {
    pub session_id: String,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchHitDto {
    pub kind: String,
    pub path: String,
    pub name: String,
    pub cell_offset: u32,
    pub matched_field: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchPageDto {
    pub hits: Vec<RegistrySearchHitDto>,
    pub scanned_keys: u32,
    pub scanned_values: u32,
    pub skipped_count: u32,
    pub truncated: bool,
}
