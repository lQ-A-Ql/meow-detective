use super::{RegistryKeyMetadataDto, RegistryValueMetadataDto};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchLocateRequestDto {
    pub session_id: String,
    pub key_path: String,
    pub key_cell_offset: u32,
    pub value_cell_offset: Option<u32>,
    pub value_index: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchLocationDto {
    pub key: RegistryKeyMetadataDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<RegistryValueMetadataDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values_cursor: Option<String>,
}
