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
    #[serde(default)]
    pub key_cell_offset: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_offset: Option<u64>,
    pub overlay_source: super::RegistryOverlaySourceDto,
    pub data_overlay_source: super::RegistryOverlaySourceDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchPageDto {
    pub hits: Vec<RegistrySearchHitDto>,
    pub scanned_keys: u32,
    pub scanned_values: u32,
    pub skipped_count: u32,
    pub truncated: bool,
    #[serde(default)]
    pub total_matches: u64,
    #[serde(default)]
    pub complete: bool,
}
