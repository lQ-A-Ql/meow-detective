use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryScanStateDto {
    Complete,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryCompletenessDto {
    MetadataOnly,
    Partial,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAllocationStateDto {
    Unverified,
    Free,
    Allocated,
    PartiallyOverwritten,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryIssueSeverityDto {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryHashAlgorithmDto {
    Md5,
    Sha1,
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryProvenanceRangeDto {
    pub ordinal: u32,
    pub range_role: String,
    pub source_kind: String,
    pub logical_offset: u64,
    pub source_offset: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_offset: Option<u64>,
    pub length: u64,
    pub allocation_state: RecoveryAllocationStateDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryIssueDto {
    pub ordinal: u32,
    pub severity: RecoveryIssueSeverityDto,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
}

/// A deleted-file candidate reconstructed from verified filesystem metadata.
///
/// `metadataOnly` is the default forensic boundary. Content is claimable only
/// when the backend has validated ownership and allocation for every content
/// range represented by `recoverableBytes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedFileRecoveryDto {
    pub id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub filesystem_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem_uuid: Option<String>,
    pub inode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mft_sequence: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at_unix: Option<u64>,
    pub declared_size: u64,
    pub recoverable_bytes: u64,
    pub completeness: RecoveryCompletenessDto,
    pub allocation_state: RecoveryAllocationStateDto,
    pub recovery_method: String,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_cycle: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_md5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_sha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
    pub provenance_ranges: Vec<RecoveryProvenanceRangeDto>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedRecoveryScanDto {
    pub id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub filesystem_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem_uuid: Option<String>,
    pub parser_version: String,
    pub log_kind: String,
    pub snapshot_identity_sha256: String,
    pub state: RecoveryScanStateDto,
    pub transaction_count: u64,
    pub candidate_count: u64,
    pub warnings: Vec<String>,
    pub started_at: String,
    pub completed_at: String,
    pub issues: Vec<RecoveryIssueDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedRecoveryPageDto {
    pub scan: DeletedRecoveryScanDto,
    pub recoveries: Vec<DeletedFileRecoveryDto>,
    pub offset: u64,
    pub limit: u32,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedRecoveryHashSearchDto {
    pub algorithm: RecoveryHashAlgorithmDto,
    pub normalized_hash: String,
    pub matches: Vec<DeletedFileRecoveryDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedRecoveryFailureDto {
    pub partition_index: u32,
    pub filesystem_type: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedRecoveryRunDto {
    pub data_source_id: String,
    pub scans: Vec<DeletedRecoveryScanDto>,
    pub failures: Vec<DeletedRecoveryFailureDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedRecoveryContentRangeDto {
    pub recovery_id: String,
    pub offset: u64,
    pub bytes_base64: String,
    pub bytes_read: u32,
    pub declared_size: u64,
    pub eof: bool,
    pub verified_range_ordinals: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedRecoveryExportDto {
    pub recovery_id: String,
    pub bytes_written: u64,
    pub sha256: String,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/recovery.rs"]
mod tests;
