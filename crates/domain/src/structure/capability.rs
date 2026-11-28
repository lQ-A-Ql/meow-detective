use serde::{Deserialize, Deserializer, Serialize};

use super::coverage::{require_digest, require_text, CoverageError, TargetCoverage};
use super::{canonical_identity, CanonicalIdentityError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum CapabilityIntegrity {
    Observed,
    NativeVerified {
        verifier_id: String,
        witness_sha256: String,
    },
    ContentVerified {
        manifest_sha256: String,
        content_sha256: String,
    },
}

impl CapabilityIntegrity {
    pub fn validate(&self) -> Result<(), CapabilityError> {
        match self {
            Self::Observed => Ok(()),
            Self::NativeVerified {
                verifier_id,
                witness_sha256,
            } => {
                require_text(verifier_id, "verifier_id")?;
                require_digest(witness_sha256, "native_witness_sha256")?;
                Ok(())
            }
            Self::ContentVerified {
                manifest_sha256,
                content_sha256,
            } => {
                require_digest(manifest_sha256, "content_manifest_sha256")?;
                require_digest(content_sha256, "content_sha256")?;
                Ok(())
            }
        }
    }
}

impl<'de> Deserialize<'de> for CapabilityIntegrity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            tag = "kind",
            rename_all = "snake_case",
            rename_all_fields = "camelCase",
            deny_unknown_fields
        )]
        enum Record {
            Observed {},
            NativeVerified {
                verifier_id: String,
                witness_sha256: String,
            },
            ContentVerified {
                manifest_sha256: String,
                content_sha256: String,
            },
        }
        let value = match Record::deserialize(deserializer)? {
            Record::Observed {} => Self::Observed,
            Record::NativeVerified {
                verifier_id,
                witness_sha256,
            } => Self::NativeVerified {
                verifier_id,
                witness_sha256,
            },
            Record::ContentVerified {
                manifest_sha256,
                content_sha256,
            } => Self::ContentVerified {
                manifest_sha256,
                content_sha256,
            },
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    AcquiredBytes,
    ContainerMetadata,
    MemberMetadata,
    BlockRange,
    DeviceSet,
    CommittedTree,
    Namespace,
    FileRange,
    ObjectInventory,
    ObjectRange,
    KvInventory,
    KeyRange,
    LogFrames,
    EventRange,
    ResourceRecords,
    RuntimeState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRef {
    pub id: String,
    pub target_id: String,
    pub view_id: String,
    pub kind: CapabilityKind,
    pub revision: u64,
    pub integrity: CapabilityIntegrity,
    pub coverage: TargetCoverage,
}

impl CapabilityRef {
    pub fn new(
        target_id: String,
        view_id: String,
        kind: CapabilityKind,
        revision: u64,
        integrity: CapabilityIntegrity,
        coverage: TargetCoverage,
    ) -> Result<Self, CapabilityError> {
        let mut value = Self {
            id: String::new(),
            target_id,
            view_id,
            kind,
            revision,
            integrity,
            coverage,
        };
        value.validate_content()?;
        value.id = value.canonical_id()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), CapabilityError> {
        self.validate_content()?;
        if self.id != self.canonical_id()? {
            return Err(CapabilityError::IdentityMismatch);
        }
        Ok(())
    }

    fn validate_content(&self) -> Result<(), CapabilityError> {
        require_text(&self.target_id, "target_id")?;
        require_text(&self.view_id, "view_id")?;
        self.integrity.validate()?;
        self.coverage.validate()?;
        if self.revision == 0 {
            return Err(CapabilityError::InvalidRevision);
        }
        if self
            .coverage
            .view_id()
            .is_some_and(|view_id| view_id != self.view_id)
        {
            return Err(CapabilityError::ViewMismatch);
        }
        if !self.kind.accepts(&self.coverage) {
            return Err(CapabilityError::IncompatibleCoverage);
        }
        if self.integrity == CapabilityIntegrity::Observed && self.coverage.has_proven_portion()? {
            return Err(CapabilityError::UnverifiedCoverage);
        }
        Ok(())
    }

    fn canonical_id(&self) -> Result<String, CapabilityError> {
        Ok(canonical_identity(
            "structure-capability-v1",
            &(
                &self.target_id,
                &self.view_id,
                self.kind,
                self.revision,
                &self.integrity,
                &self.coverage,
            ),
        )?)
    }
}

impl CapabilityKind {
    pub fn accepts(self, coverage: &TargetCoverage) -> bool {
        match self {
            Self::AcquiredBytes
            | Self::BlockRange
            | Self::FileRange
            | Self::ObjectRange
            | Self::LogFrames => matches!(coverage, TargetCoverage::Bytes(_)),
            Self::Namespace | Self::ObjectInventory | Self::ResourceRecords => {
                matches!(coverage, TargetCoverage::Namespace(_))
            }
            Self::KvInventory | Self::KeyRange => matches!(coverage, TargetCoverage::Keys(_)),
            Self::EventRange => matches!(coverage, TargetCoverage::Events(_)),
            Self::RuntimeState => matches!(coverage, TargetCoverage::Runtime(_)),
            Self::ContainerMetadata | Self::MemberMetadata => matches!(
                coverage,
                TargetCoverage::Bytes(_) | TargetCoverage::Namespace(_)
            ),
            Self::DeviceSet | Self::CommittedTree => {
                matches!(coverage, TargetCoverage::Namespace(_))
            }
        }
    }
}

impl<'de> Deserialize<'de> for CapabilityRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            id: String,
            target_id: String,
            view_id: String,
            kind: CapabilityKind,
            revision: u64,
            integrity: CapabilityIntegrity,
            coverage: TargetCoverage,
        }
        let record = Record::deserialize(deserializer)?;
        let value = Self {
            id: record.id,
            target_id: record.target_id,
            view_id: record.view_id,
            kind: record.kind,
            revision: record.revision,
            integrity: record.integrity,
            coverage: record.coverage,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error(transparent)]
    Coverage(#[from] CoverageError),
    #[error(transparent)]
    Identity(#[from] CanonicalIdentityError),
    #[error("capability revisions start at one")]
    InvalidRevision,
    #[error("capability identity disagrees with its contents")]
    IdentityMismatch,
    #[error("capability coverage references a different fixed view")]
    ViewMismatch,
    #[error("capability kind does not support this coverage space")]
    IncompatibleCoverage,
    #[error("proven coverage requires verified integrity")]
    UnverifiedCoverage,
}
