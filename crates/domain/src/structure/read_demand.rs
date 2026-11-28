use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ByteRangeSet, ByteRangeSetError, SourceRevisionRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadIntent {
    MetadataInspection,
    ContentRead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadPriority {
    Interactive,
    Normal,
    Deferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadVerification {
    ContentDigest,
    NativeChecksum,
    RangeCoverage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureAttemptRef {
    pub attempt_id: String,
    pub owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadDemand {
    pub id: String,
    pub source_revision: SourceRevisionRef,
    pub target_fingerprint: String,
    pub intent: ReadIntent,
    pub ranges: ByteRangeSet,
    pub priority: ReadPriority,
    pub deadline_unix_ms: Option<u64>,
    pub reorderable: bool,
    pub consumers: Vec<String>,
    pub verification: ReadVerification,
    pub attempt: Option<StructureAttemptRef>,
}

impl ReadDemand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_revision: SourceRevisionRef,
        target_fingerprint: String,
        intent: ReadIntent,
        ranges: ByteRangeSet,
        priority: ReadPriority,
        deadline_unix_ms: Option<u64>,
        reorderable: bool,
        consumers: Vec<String>,
        verification: ReadVerification,
    ) -> Result<Self, ReadDemandError> {
        ranges.validate().map_err(ReadDemandError::Ranges)?;
        if target_fingerprint.is_empty()
            || source_revision.source_id.is_empty()
            || source_revision.case_id.is_empty()
            || consumers.is_empty()
            || consumers.iter().any(String::is_empty)
        {
            return Err(ReadDemandError::InvalidIdentity);
        }
        let mut demand = Self {
            id: String::new(),
            source_revision,
            target_fingerprint,
            intent,
            ranges,
            priority,
            deadline_unix_ms,
            reorderable,
            consumers,
            verification,
            attempt: None,
        };
        demand.id = demand_id(&demand);
        Ok(demand)
    }

    pub fn with_attempt(mut self, attempt: StructureAttemptRef) -> Result<Self, ReadDemandError> {
        if attempt.attempt_id.is_empty() || attempt.owner.is_empty() {
            return Err(ReadDemandError::InvalidIdentity);
        }
        self.attempt = Some(attempt);
        self.id = demand_id(&self);
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadDemandError {
    #[error(transparent)]
    Ranges(#[from] ByteRangeSetError),
    #[error("read demand identity is invalid")]
    InvalidIdentity,
}

fn demand_id(demand: &ReadDemand) -> String {
    let mut consumers = demand.consumers.clone();
    consumers.sort();
    consumers.dedup();
    let identity = serde_json::json!({
        "source": demand.source_revision, "target": demand.target_fingerprint,
        "intent": demand.intent, "ranges": demand.ranges, "priority": demand.priority,
        "deadline": demand.deadline_unix_ms, "reorderable": demand.reorderable,
        "consumers": consumers, "verification": demand.verification, "attempt": demand.attempt,
    });
    format!(
        "read-demand-v2-{}",
        hex::encode(Sha256::digest(identity.to_string().as_bytes()))
    )
}
