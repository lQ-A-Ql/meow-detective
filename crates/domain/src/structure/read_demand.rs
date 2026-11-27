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
        let id = demand_id(
            &source_revision,
            &target_fingerprint,
            intent,
            &ranges,
            priority,
            &consumers,
        );
        Ok(Self {
            id,
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
        })
    }

    pub fn with_attempt(mut self, attempt: StructureAttemptRef) -> Result<Self, ReadDemandError> {
        if attempt.attempt_id.is_empty() || attempt.owner.is_empty() {
            return Err(ReadDemandError::InvalidIdentity);
        }
        self.attempt = Some(attempt);
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

fn demand_id(
    source: &SourceRevisionRef,
    target_fingerprint: &str,
    intent: ReadIntent,
    ranges: &ByteRangeSet,
    priority: ReadPriority,
    consumers: &[String],
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"read-demand-v1");
    for value in [
        source.case_id.as_bytes(),
        source.source_id.as_bytes(),
        target_fingerprint.as_bytes(),
    ] {
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value);
    }
    digest.update(source.revision.to_le_bytes());
    digest.update([intent as u8, priority as u8]);
    digest.update(ranges.target_size().to_le_bytes());
    digest.update((ranges.ranges().len() as u64).to_le_bytes());
    for range in ranges.ranges() {
        digest.update(range.start().to_le_bytes());
        digest.update(range.length().to_le_bytes());
    }
    let mut consumers = consumers.to_vec();
    consumers.sort();
    for consumer in consumers {
        digest.update((consumer.len() as u64).to_le_bytes());
        digest.update(consumer.as_bytes());
    }
    format!("read-demand-v1-{}", hex::encode(digest.finalize()))
}
