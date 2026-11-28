use std::collections::BTreeSet;

use domain::structure::{
    canonical_identity, CanonicalIdentityError, CapabilityKind, StructureRequest, StructureStage,
    TargetCoverage, TargetFailure, TargetResult, TargetSelector, TargetStatus, WorkIdentity,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityDefinition {
    pub work: WorkIdentity,
    pub stage: StructureStage,
    pub target: TargetSelector,
    pub scope_id: String,
    pub required_capabilities: BTreeSet<CapabilityKind>,
    pub requested_coverage: TargetCoverage,
    pub dependencies: BTreeSet<String>,
    pub parameters: serde_json::Value,
}

impl ActivityDefinition {
    pub fn key(&self) -> Result<String, RuntimeError> {
        self.validate()?;
        Ok(self.work.key()?)
    }

    pub fn validate(&self) -> Result<(), RuntimeError> {
        self.work.validate()?;
        self.target
            .validate()
            .map_err(|_| RuntimeError::InvalidDefinition)?;
        self.requested_coverage
            .validate()
            .map_err(|_| RuntimeError::InvalidDefinition)?;
        if self.scope_id.trim().is_empty()
            || self.required_capabilities.is_empty()
            || self
                .requested_coverage
                .has_proven_portion()
                .map_err(|_| RuntimeError::InvalidDefinition)?
            || self.dependencies.iter().any(|id| id.trim().is_empty())
            || self.work.selector_sha256 != identity_digest("target-selector-v1", &self.target)?
        {
            return Err(RuntimeError::InvalidDefinition);
        }
        // The definition digest also validates the adapter-owned parameter structure.
        self.definition_digest()?;
        Ok(())
    }

    pub fn failure_result(&self, failure: TargetFailure) -> Result<TargetResult, RuntimeError> {
        let status = match failure {
            TargetFailure::CorruptMetadata { .. } => TargetStatus::CorruptMetadata,
            TargetFailure::Conflicted { .. } => TargetStatus::Conflicted,
            TargetFailure::Cancelled { .. } => TargetStatus::Cancelled,
            TargetFailure::UnsupportedFeature { .. } => TargetStatus::UnsupportedFeature,
            _ => TargetStatus::NeedsEvidence,
        };
        let result = TargetResult {
            target_id: self
                .target
                .identity()
                .map_err(|_| RuntimeError::InvalidDefinition)?,
            status,
            required_capabilities: self.required_capabilities.clone(),
            coverage: self.requested_coverage.clone(),
            evidence_ids: BTreeSet::new(),
            error: Some(failure),
        };
        result.validate().map_err(|_| RuntimeError::InvalidOutput)?;
        Ok(result)
    }

    pub fn definition_digest(&self) -> Result<String, RuntimeError> {
        identity_digest("structure-activity-definition-v1", self)
    }
}

pub(super) fn identity_digest<T: Serialize + ?Sized>(
    domain: &str,
    value: &T,
) -> Result<String, RuntimeError> {
    let identity = canonical_identity(domain, value)?;
    Ok(identity[domain.len() + 1..].to_owned())
}

pub fn source_scope_id(request: &StructureRequest) -> Result<String, RuntimeError> {
    let captures = request
        .source_scope
        .iter()
        .map(|source| identity_digest("source-capture-v1", source))
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(canonical_identity("structure-source-scope-v1", &captures)?)
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Identity(#[from] CanonicalIdentityError),
    #[error(transparent)]
    Work(#[from] domain::structure::OperationModelError),
    #[error(transparent)]
    Frontier(#[from] super::frontier::FrontierError),
    #[error(transparent)]
    Database(#[from] persistence_sqlite::DbError),
    #[error("runtime activity definition is invalid")]
    InvalidDefinition,
    #[error("runtime registration is invalid")]
    InvalidRegistration,
    #[error("runtime registration already exists")]
    DuplicateRegistration,
    #[error("runtime operation is not registered")]
    MissingOperation,
    #[error("runtime activity definition conflicts with a stable work key")]
    ConflictingWork,
    #[error("runtime activity dependency is missing")]
    MissingDependency,
    #[error("runtime activity graph contains a cycle")]
    DependencyCycle,
    #[error("runtime bound was exceeded")]
    Capacity,
    #[error("runtime state update lost its execution owner")]
    LostOwner,
    #[error("runtime operation output is invalid")]
    InvalidOutput,
    #[error("runtime execution failed")]
    Target(TargetFailure),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
