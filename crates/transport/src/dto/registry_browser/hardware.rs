use super::RegistryBrowserSourceDto;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHardwareRequestDto {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHardwareDeviceDto {
    pub kind: String,
    pub device_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// Exact numeric text avoids truncating firmware uint64 values in JavaScript.
    pub properties: BTreeMap<String, String>,
    pub key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHardwareSummaryDto {
    pub source: RegistryBrowserSourceDto,
    pub devices: Vec<RegistryHardwareDeviceDto>,
    pub warnings: Vec<String>,
}
