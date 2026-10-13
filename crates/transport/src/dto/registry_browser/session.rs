use serde::{Deserialize, Serialize};

use super::{
    RegistryBrowserDiagnosticDto, RegistryBrowserSourceDto, RegistryHiveDescriptorDto,
    RegistryHiveViewDto, RegistryKeyMetadataDto,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHiveOpenRequestDto {
    pub file_id: String,
    pub view: RegistryHiveViewDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHiveSessionDto {
    pub session_id: String,
    pub source: RegistryBrowserSourceDto,
    pub hive: RegistryHiveDescriptorDto,
    pub root_key: RegistryKeyMetadataDto,
    pub expires_at: String,
    pub diagnostics: Vec<RegistryBrowserDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHiveCloseRequestDto {
    pub session_id: String,
}
