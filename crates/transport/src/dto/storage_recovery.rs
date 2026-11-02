use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageRecoveryDispositionDto {
    Ready,
    RecoveryRequired,
    RepairRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRecoveryItemDto {
    pub operation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub disposition: StorageRecoveryDispositionDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRecoveryReportDto {
    pub disposition: StorageRecoveryDispositionDto,
    pub items: Vec<StorageRecoveryItemDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairStorageOperationRequestDto {
    pub operation_id: String,
    pub reason: String,
}
