use serde::{Deserialize, Serialize};

use super::{ByteRangeSet, ReadVerification, SourceRevisionRef, StructureAttemptRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReceipt {
    pub id: String,
    pub source_revision: SourceRevisionRef,
    pub dependency_revisions: Vec<SourceRevisionRef>,
    pub scope_fingerprint: String,
    pub generation_identity: String,
    pub target_fingerprint: String,
    pub adapter_version: String,
    pub policy_version: String,
    pub ranges: ByteRangeSet,
    pub bytes_verified: u64,
    pub output_sha256: String,
    pub expected_sha256: Option<String>,
    pub verification: ReadVerification,
    pub attempt: StructureAttemptRef,
}
