use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseSummaryDto {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examiner: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseMetricsDto {
    pub data_source_count: u64,
    pub indexed_file_count: u64,
    pub timeline_event_count: u64,
    pub artifact_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentObjectDto {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub time: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourceSummaryDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source_path: String,
    pub imported_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partitions: Vec<DataSourcePartitionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourcePartitionDto {
    pub index: u32,
    pub name: String,
    pub kind_label: String,
    pub status: String,
    pub offset: u64,
    pub length: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlock_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentCaseDto {
    pub case_root: String,
    pub name: String,
    pub opened_at: String,
}
