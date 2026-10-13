use serde::{Deserialize, Serialize};

/// Requested byte provenance; recovered views report whether replay was needed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryHiveViewDto {
    Base,
    Recovered,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryBrowserSourceDto {
    pub case_id: String,
    pub data_source_id: String,
    pub file_id: String,
    pub source_revision: String,
    pub view: RegistryHiveViewDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryBrowserCoverageStatusDto {
    Complete,
    Partial,
}

/// Counts describe the collection and this returned page; pagination is separate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryBrowserCoverageDto {
    pub declared_count: u64,
    pub returned_count: u32,
    pub skipped_count: u32,
    pub status: RegistryBrowserCoverageStatusDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryBrowserDiagnosticDto {
    pub code: String,
    pub recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}
