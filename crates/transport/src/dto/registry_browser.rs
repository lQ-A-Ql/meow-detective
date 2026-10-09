use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryBrowserValueDto {
    pub name: String,
    pub value_type: String,
    pub decoded: String,
    pub raw_hex: String,
    pub cell_offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryBrowserKeyDto {
    pub overlay_source: RegistryOverlaySourceDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay_warning: Option<String>,
    pub path: String,
    pub name: String,
    pub cell_offset: u32,
    pub last_write_time: Option<u64>,
    pub subkey_count: u32,
    pub value_count: u32,
    pub values: Vec<RegistryBrowserValueDto>,
    pub subkeys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryOverlaySourceDto {
    Base,
    Recovered,
    Merged,
}
