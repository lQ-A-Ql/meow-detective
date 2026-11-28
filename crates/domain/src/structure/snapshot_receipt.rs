use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use super::{canonical_identity, CanonicalIdentityError, SourceRevisionRef, StructureAttemptRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotAnalysisReceipt {
    pub source_revision: SourceRevisionRef,
    pub dependency_revisions: Vec<SourceRevisionRef>,
    pub target_id: String,
    pub input_capability_ids: BTreeSet<String>,
    pub generation_identity: String,
    pub scope_fingerprint: String,
    pub output_sha256: String,
    pub attempt: StructureAttemptRef,
    pub verifier_id: String,
    pub policy_version: String,
    pub id: String,
}

impl SnapshotAnalysisReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_revision: SourceRevisionRef,
        mut dependency_revisions: Vec<SourceRevisionRef>,
        target_id: String,
        input_capability_ids: BTreeSet<String>,
        generation_identity: String,
        scope_fingerprint: String,
        output_sha256: String,
        attempt: StructureAttemptRef,
        verifier_id: String,
        policy_version: String,
    ) -> Result<Self, SnapshotAnalysisReceiptError> {
        dependency_revisions.sort_by(|left, right| revision_key(left).cmp(&revision_key(right)));
        dependency_revisions.dedup();
        let mut receipt = Self {
            source_revision,
            dependency_revisions,
            target_id,
            input_capability_ids,
            generation_identity,
            scope_fingerprint,
            output_sha256,
            attempt,
            verifier_id,
            policy_version,
            id: String::new(),
        };
        receipt.validate_fields()?;
        receipt.id = receipt.canonical_id()?;
        Ok(receipt)
    }

    pub fn validate(&self) -> Result<(), SnapshotAnalysisReceiptError> {
        self.validate_fields()?;
        if self.id != self.canonical_id()? {
            return Err(SnapshotAnalysisReceiptError::IdentityMismatch);
        }
        Ok(())
    }

    fn validate_fields(&self) -> Result<(), SnapshotAnalysisReceiptError> {
        validate_revision(&self.source_revision)?;
        for (value, field) in [
            (self.target_id.as_str(), "target_id"),
            (self.generation_identity.as_str(), "generation_identity"),
            (self.attempt.attempt_id.as_str(), "attempt_id"),
            (self.attempt.owner.as_str(), "owner"),
            (self.verifier_id.as_str(), "verifier_id"),
            (self.policy_version.as_str(), "policy_version"),
        ] {
            require_text(value, field)?;
        }
        require_digest(&self.scope_fingerprint, "scope_fingerprint")?;
        require_digest(&self.output_sha256, "output_sha256")?;
        if self.input_capability_ids.is_empty() {
            return Err(SnapshotAnalysisReceiptError::MissingCapabilities);
        }
        for capability in &self.input_capability_ids {
            require_text(capability, "input_capability_id")?;
        }
        for dependency in &self.dependency_revisions {
            validate_revision(dependency)?;
            if dependency.case_id != self.source_revision.case_id {
                return Err(SnapshotAnalysisReceiptError::CrossCaseDependency);
            }
        }
        for pair in self.dependency_revisions.windows(2) {
            if revision_key(&pair[0]) >= revision_key(&pair[1]) {
                return Err(SnapshotAnalysisReceiptError::NonCanonicalDependencies);
            }
            if pair[0].source_id == pair[1].source_id {
                return Err(SnapshotAnalysisReceiptError::ConflictingDependencyRevision);
            }
        }
        if !self.dependency_revisions.contains(&self.source_revision) {
            return Err(SnapshotAnalysisReceiptError::MissingSourceDependency);
        }
        Ok(())
    }

    fn canonical_id(&self) -> Result<String, SnapshotAnalysisReceiptError> {
        let identity = ReceiptIdentity {
            source_revision: &self.source_revision,
            dependency_revisions: &self.dependency_revisions,
            target_id: &self.target_id,
            input_capability_ids: &self.input_capability_ids,
            generation_identity: &self.generation_identity,
            scope_fingerprint: &self.scope_fingerprint,
            output_sha256: &self.output_sha256,
            attempt: &self.attempt,
            verifier_id: &self.verifier_id,
            policy_version: &self.policy_version,
        };
        Ok(canonical_identity(
            "snapshot-analysis-receipt-v1",
            &identity,
        )?)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptIdentity<'a> {
    source_revision: &'a SourceRevisionRef,
    dependency_revisions: &'a [SourceRevisionRef],
    target_id: &'a str,
    input_capability_ids: &'a BTreeSet<String>,
    generation_identity: &'a str,
    scope_fingerprint: &'a str,
    output_sha256: &'a str,
    attempt: &'a StructureAttemptRef,
    verifier_id: &'a str,
    policy_version: &'a str,
}

impl<'de> Deserialize<'de> for SnapshotAnalysisReceipt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            source_revision: RevisionRecord,
            dependency_revisions: Vec<RevisionRecord>,
            target_id: String,
            input_capability_ids: BTreeSet<String>,
            generation_identity: String,
            scope_fingerprint: String,
            output_sha256: String,
            attempt: AttemptRecord,
            verifier_id: String,
            policy_version: String,
            id: String,
        }
        let record = Record::deserialize(deserializer)?;
        let receipt = Self {
            source_revision: record.source_revision.into_revision(),
            dependency_revisions: record
                .dependency_revisions
                .into_iter()
                .map(RevisionRecord::into_revision)
                .collect(),
            target_id: record.target_id,
            input_capability_ids: record.input_capability_ids,
            generation_identity: record.generation_identity,
            scope_fingerprint: record.scope_fingerprint,
            output_sha256: record.output_sha256,
            attempt: StructureAttemptRef {
                attempt_id: record.attempt.attempt_id,
                owner: record.attempt.owner,
            },
            verifier_id: record.verifier_id,
            policy_version: record.policy_version,
            id: record.id,
        };
        receipt.validate().map_err(serde::de::Error::custom)?;
        Ok(receipt)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevisionRecord {
    case_id: String,
    source_id: String,
    revision: u64,
}

impl RevisionRecord {
    fn into_revision(self) -> SourceRevisionRef {
        SourceRevisionRef {
            case_id: self.case_id,
            source_id: self.source_id,
            revision: self.revision,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AttemptRecord {
    attempt_id: String,
    owner: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotAnalysisReceiptError {
    #[error("snapshot analysis receipt field '{field}' is empty")]
    EmptyField { field: &'static str },
    #[error("snapshot analysis source revision must be registered")]
    UnregisteredRevision,
    #[error("snapshot analysis receipt digest '{field}' must be lowercase SHA-256")]
    InvalidDigest { field: &'static str },
    #[error("snapshot analysis receipt requires input capability references")]
    MissingCapabilities,
    #[error("snapshot analysis dependencies belong to different cases")]
    CrossCaseDependency,
    #[error("snapshot analysis dependencies must be sorted and deduplicated")]
    NonCanonicalDependencies,
    #[error("snapshot analysis dependencies contain different revisions of the same source")]
    ConflictingDependencyRevision,
    #[error("snapshot analysis dependencies must include the selected source revision")]
    MissingSourceDependency,
    #[error("snapshot analysis receipt identity does not match its contents")]
    IdentityMismatch,
    #[error(transparent)]
    Identity(#[from] CanonicalIdentityError),
}

fn validate_revision(revision: &SourceRevisionRef) -> Result<(), SnapshotAnalysisReceiptError> {
    require_text(&revision.case_id, "case_id")?;
    require_text(&revision.source_id, "source_id")?;
    if revision.revision == 0 {
        return Err(SnapshotAnalysisReceiptError::UnregisteredRevision);
    }
    Ok(())
}

fn revision_key(revision: &SourceRevisionRef) -> (&str, &str, u64) {
    (&revision.case_id, &revision.source_id, revision.revision)
}

fn require_text(value: &str, field: &'static str) -> Result<(), SnapshotAnalysisReceiptError> {
    if value.trim().is_empty() {
        return Err(SnapshotAnalysisReceiptError::EmptyField { field });
    }
    Ok(())
}

fn require_digest(value: &str, field: &'static str) -> Result<(), SnapshotAnalysisReceiptError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SnapshotAnalysisReceiptError::InvalidDigest { field });
    }
    Ok(())
}
