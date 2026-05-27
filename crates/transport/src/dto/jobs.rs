use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshotDto {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub progress: u32,
    pub status: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_partition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_partitions: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_partitions: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_progress: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningItemDto {
    pub id: String,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceItemDto {
    pub id: String,
    pub ts: String,
    pub message: String,
}
