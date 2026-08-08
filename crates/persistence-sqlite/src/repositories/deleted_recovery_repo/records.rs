#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryScanRecord {
    pub id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub filesystem_type: String,
    pub filesystem_uuid: Option<String>,
    pub parser_version: String,
    pub log_kind: String,
    pub snapshot_identity_sha256: String,
    pub state: String,
    pub transaction_count: u64,
    pub candidate_count: u64,
    pub warnings: Vec<String>,
    pub started_at: String,
    pub completed_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeletedRecoveryRecord {
    pub id: String,
    pub inode: String,
    pub original_path: Option<String>,
    pub entry_type: Option<String>,
    pub mode: Option<u16>,
    pub mft_sequence: Option<u16>,
    pub deleted_at_unix: Option<u64>,
    pub declared_size: u64,
    pub recoverable_bytes: u64,
    pub completeness: String,
    pub recovery_method: String,
    pub confidence: f64,
    pub allocation_state: String,
    pub transaction_id: Option<String>,
    pub log_sequence: Option<u64>,
    pub log_cycle: Option<u64>,
    pub content_md5: Option<String>,
    pub content_sha1: Option<String>,
    pub content_sha256: Option<String>,
    pub warnings: Vec<String>,
    pub ranges: Vec<RecoveryRangeRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeletedRecoveryHashAlgorithm {
    Md5,
    Sha1,
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryRangeRecord {
    pub ordinal: u32,
    pub range_role: String,
    pub source_kind: String,
    pub logical_offset: u64,
    pub source_offset: u64,
    pub physical_offset: Option<u64>,
    pub length: u64,
    pub allocation_state: String,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryIssueRecord {
    pub ordinal: u32,
    pub severity: String,
    pub code: String,
    pub message: String,
    pub log_offset: Option<u64>,
    pub sequence: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeletedRecoveryAggregate {
    pub scan: RecoveryScanRecord,
    pub recoveries: Vec<DeletedRecoveryRecord>,
    pub issues: Vec<RecoveryIssueRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeletedRecoveryPageRecord {
    pub scan: RecoveryScanRecord,
    pub recoveries: Vec<DeletedRecoveryRecord>,
    pub issues: Vec<RecoveryIssueRecord>,
    pub offset: u64,
    pub limit: u32,
    pub total: u64,
}
