use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntryDto {
    pub id: String,
    pub scope_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_id: Option<String>,
    pub audit_id: String,
    pub sequence: u64,
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    pub details: String,
    pub previous_hash: String,
    pub entry_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LedgerBatchDto {
    pub id: String,
    pub scope_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_id: Option<String>,
    pub start_sequence: u64,
    pub end_sequence: u64,
    pub entry_count: u64,
    pub merkle_root: String,
    pub head_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LedgerBatchVerificationDto {
    pub valid: bool,
    pub batch_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LedgerVerificationDto {
    pub valid: bool,
    pub entry_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_error: Option<String>,
    pub batches: LedgerBatchVerificationDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LedgerSnapshotDto {
    pub entries: Vec<LedgerEntryDto>,
    pub batches: Vec<LedgerBatchDto>,
    pub verification: LedgerVerificationDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LedgerProofStepDto {
    pub sibling_hash: String,
    pub sibling_on_left: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LedgerProofDto {
    pub batch: LedgerBatchDto,
    pub sequence: u64,
    pub entry_hash: String,
    pub leaf_index: u64,
    pub leaf_count: u64,
    pub steps: Vec<LedgerProofStepDto>,
    pub merkle_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetLedgerRequest {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetLedgerProofRequest {
    pub batch_id: String,
    pub sequence: u64,
}
