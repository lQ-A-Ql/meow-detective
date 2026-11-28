use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use super::coverage::{require_text, CoverageError, TargetCoverage};
use super::CapabilityKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Pending,
    Ready,
    Admitted,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}

impl ActivityState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Blocked | Self::Cancelled
        )
    }

    pub fn transition(self, next: Self) -> Result<Self, OutcomeError> {
        let allowed = matches!(
            (self, next),
            (
                Self::Pending,
                Self::Ready | Self::Failed | Self::Blocked | Self::Cancelled
            ) | (
                Self::Ready,
                Self::Admitted | Self::Failed | Self::Blocked | Self::Cancelled
            ) | (
                Self::Admitted,
                Self::Running | Self::Failed | Self::Blocked | Self::Cancelled
            ) | (
                Self::Running,
                Self::Succeeded | Self::Failed | Self::Blocked | Self::Cancelled
            )
        );
        if !allowed {
            return Err(OutcomeError::InvalidActivityTransition {
                from: self,
                to: next,
            });
        }
        Ok(next)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetStatus {
    Verified,
    Partial,
    NeedsEvidence,
    UnsupportedFeature,
    CorruptMetadata,
    Conflicted,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRequirement {
    Member,
    BackingVersion,
    NativeVersionWitness,
    NamespacePage,
    ReferencedContent,
    RuntimeSnapshot,
    DecryptionMaterial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataFailure {
    InvalidStructure,
    InvalidChecksum,
    Truncated,
    InvalidNativeOrder,
    InconsistentReferences,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictReason {
    MemberIdentity,
    NativeVersion,
    InputChanged,
    PublicationOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Bytes,
    Readers,
    Handles,
    Cpu,
    ActiveWork,
    DependencyDepth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationScope {
    Target,
    Run,
    Source,
    Case,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum TargetFailure {
    MissingCapabilities {
        capabilities: BTreeSet<CapabilityKind>,
    },
    MissingEvidence {
        requirement: EvidenceRequirement,
    },
    UnsupportedFeature {
        capability: CapabilityKind,
    },
    CorruptMetadata {
        reason: MetadataFailure,
    },
    Conflicted {
        reason: ConflictReason,
    },
    Cancelled {
        scope: CancellationScope,
    },
    ResourceLimit {
        resource: ResourceKind,
        required: u64,
        limit: u64,
    },
    IoUnavailable,
    DeadlineExceeded,
    RequiredDependencyFailed,
}

impl<'de> Deserialize<'de> for TargetFailure {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            tag = "kind",
            rename_all = "snake_case",
            rename_all_fields = "camelCase",
            deny_unknown_fields
        )]
        enum Record {
            MissingCapabilities {
                capabilities: BTreeSet<CapabilityKind>,
            },
            MissingEvidence {
                requirement: EvidenceRequirement,
            },
            UnsupportedFeature {
                capability: CapabilityKind,
            },
            CorruptMetadata {
                reason: MetadataFailure,
            },
            Conflicted {
                reason: ConflictReason,
            },
            Cancelled {
                scope: CancellationScope,
            },
            ResourceLimit {
                resource: ResourceKind,
                required: u64,
                limit: u64,
            },
            IoUnavailable {},
            DeadlineExceeded {},
            RequiredDependencyFailed {},
        }
        let value = match Record::deserialize(deserializer)? {
            Record::MissingCapabilities { capabilities } => {
                Self::MissingCapabilities { capabilities }
            }
            Record::MissingEvidence { requirement } => Self::MissingEvidence { requirement },
            Record::UnsupportedFeature { capability } => Self::UnsupportedFeature { capability },
            Record::CorruptMetadata { reason } => Self::CorruptMetadata { reason },
            Record::Conflicted { reason } => Self::Conflicted { reason },
            Record::Cancelled { scope } => Self::Cancelled { scope },
            Record::ResourceLimit {
                resource,
                required,
                limit,
            } => Self::ResourceLimit {
                resource,
                required,
                limit,
            },
            Record::IoUnavailable {} => Self::IoUnavailable,
            Record::DeadlineExceeded {} => Self::DeadlineExceeded,
            Record::RequiredDependencyFailed {} => Self::RequiredDependencyFailed,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

impl TargetFailure {
    pub fn validate(&self) -> Result<(), OutcomeError> {
        match self {
            Self::MissingCapabilities { capabilities } if capabilities.is_empty() => {
                Err(OutcomeError::EmptyMissingCapabilities)
            }
            Self::ResourceLimit {
                required, limit, ..
            } if required <= limit => Err(OutcomeError::InvalidResourceLimit),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetResult {
    pub target_id: String,
    pub status: TargetStatus,
    pub required_capabilities: BTreeSet<CapabilityKind>,
    pub coverage: TargetCoverage,
    pub evidence_ids: BTreeSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TargetFailure>,
}

impl TargetResult {
    pub fn validate(&self) -> Result<(), OutcomeError> {
        require_text(&self.target_id, "target_id")?;
        self.coverage.validate()?;
        for evidence_id in &self.evidence_ids {
            require_text(evidence_id, "evidence_id")?;
        }
        if self.required_capabilities.is_empty() {
            return Err(OutcomeError::EmptyRequiredCapabilities);
        }
        if let Some(error) = &self.error {
            error.validate()?;
            if let TargetFailure::MissingCapabilities { capabilities } = error {
                if !capabilities.is_subset(&self.required_capabilities) {
                    return Err(OutcomeError::InconsistentTargetStatus);
                }
            }
        }
        let complete = self.coverage.is_complete()?;
        let proven = self.coverage.has_proven_portion()?;
        if proven && self.evidence_ids.is_empty() {
            return Err(OutcomeError::MissingEvidenceReferences);
        }
        let valid = match (&self.status, &self.error) {
            (TargetStatus::Verified, None) => complete && !self.evidence_ids.is_empty(),
            (TargetStatus::Partial, error) => {
                proven && !complete && error.as_ref().is_none_or(partial_failure)
            }
            (
                TargetStatus::NeedsEvidence,
                Some(TargetFailure::MissingCapabilities { capabilities }),
            ) => capabilities.is_subset(&self.required_capabilities) && !complete,
            (TargetStatus::NeedsEvidence, Some(TargetFailure::MissingEvidence { .. })) => !complete,
            (
                TargetStatus::NeedsEvidence,
                Some(
                    TargetFailure::ResourceLimit { .. }
                    | TargetFailure::IoUnavailable
                    | TargetFailure::DeadlineExceeded
                    | TargetFailure::RequiredDependencyFailed,
                ),
            ) => !complete,
            (
                TargetStatus::UnsupportedFeature,
                Some(TargetFailure::UnsupportedFeature { capability }),
            ) => self.required_capabilities.contains(capability),
            (TargetStatus::CorruptMetadata, Some(TargetFailure::CorruptMetadata { .. }))
            | (TargetStatus::Conflicted, Some(TargetFailure::Conflicted { .. }))
            | (TargetStatus::Cancelled, Some(TargetFailure::Cancelled { .. })) => true,
            _ => false,
        };
        if !valid {
            return Err(OutcomeError::InconsistentTargetStatus);
        }
        Ok(())
    }
}

fn partial_failure(error: &TargetFailure) -> bool {
    matches!(
        error,
        TargetFailure::MissingCapabilities { .. }
            | TargetFailure::MissingEvidence { .. }
            | TargetFailure::ResourceLimit { .. }
            | TargetFailure::IoUnavailable
            | TargetFailure::DeadlineExceeded
            | TargetFailure::RequiredDependencyFailed
    )
}

impl<'de> Deserialize<'de> for TargetResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            target_id: String,
            status: TargetStatus,
            required_capabilities: BTreeSet<CapabilityKind>,
            coverage: TargetCoverage,
            evidence_ids: BTreeSet<String>,
            error: Option<TargetFailure>,
        }
        let record = Record::deserialize(deserializer)?;
        let value = Self {
            target_id: record.target_id,
            status: record.status,
            required_capabilities: record.required_capabilities,
            coverage: record.coverage,
            evidence_ids: record.evidence_ids,
            error: record.error,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutcomeError {
    #[error(transparent)]
    Coverage(#[from] CoverageError),
    #[error("activity transition from {from:?} to {to:?} is not allowed")]
    InvalidActivityTransition {
        from: ActivityState,
        to: ActivityState,
    },
    #[error("target result requires explicit capabilities")]
    EmptyRequiredCapabilities,
    #[error("missing capabilities must contain at least one capability")]
    EmptyMissingCapabilities,
    #[error("resource limit failure must exceed the available limit")]
    InvalidResourceLimit,
    #[error("proven target portions require evidence references")]
    MissingEvidenceReferences,
    #[error("target status disagrees with its coverage or typed failure")]
    InconsistentTargetStatus,
}
