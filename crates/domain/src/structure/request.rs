use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use super::{
    canonical_identity, CanonicalIdentityError, ReadPriority, SourceCaptureRef, TargetSelector,
    VersionSelection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureGoal {
    Inventory,
    NamespaceQuery,
    RangeRead,
    Export,
    Analyze,
    EventQuery,
    HistoricalQuery,
    DeepVerify,
    RuntimeInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructureResourcePolicy {
    pub bytes: u64,
    pub readers: u32,
    pub handles: u32,
    pub cpu: u32,
    pub io_domains: u32,
    pub priority: ReadPriority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_unix_ms: Option<u64>,
    pub active_work_items: u32,
    pub page_rows: u32,
    pub page_bytes: u64,
}

impl StructureResourcePolicy {
    pub fn validate(&self) -> Result<(), StructureRequestError> {
        if self.bytes == 0
            || self.readers == 0
            || self.handles == 0
            || self.cpu == 0
            || self.io_domains == 0
            || self.active_work_items == 0
            || self.active_work_items > 4096
            || self.page_rows == 0
            || self.page_rows > 512
            || self.page_bytes == 0
            || self.page_bytes > self.bytes
            || self.deadline_unix_ms == Some(0)
        {
            return Err(StructureRequestError::ResourcePolicy);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructureRequest {
    pub case_id: String,
    pub source_scope: Vec<SourceCaptureRef>,
    pub targets: Vec<TargetSelector>,
    pub version_selection: VersionSelection,
    pub goals: BTreeSet<StructureGoal>,
    pub analysis_profiles: BTreeSet<String>,
    pub resource_policy: StructureResourcePolicy,
    pub adapter_hints: BTreeSet<String>,
}

impl StructureRequest {
    pub fn validate(&self) -> Result<(), StructureRequestError> {
        if self.case_id.is_empty()
            || self.source_scope.is_empty()
            || self.source_scope.len() > 1024
            || self.targets.len() > 4096
            || self.goals.is_empty()
        {
            return Err(StructureRequestError::Scope);
        }
        for source in &self.source_scope {
            source
                .validate()
                .map_err(|_| StructureRequestError::Scope)?;
            if source.source.case_id != self.case_id {
                return Err(StructureRequestError::Scope);
            }
        }
        for target in &self.targets {
            target
                .validate()
                .map_err(|_| StructureRequestError::Target)?;
        }
        self.version_selection
            .validate()
            .map_err(|_| StructureRequestError::Version)?;
        for value in self.analysis_profiles.iter().chain(&self.adapter_hints) {
            if value.is_empty() {
                return Err(StructureRequestError::EmptyLabel);
            }
        }
        if self.goals.contains(&StructureGoal::Analyze) && self.analysis_profiles.is_empty() {
            return Err(StructureRequestError::MissingAnalysisProfile);
        }
        if self.targets.is_empty() && !self.goals.contains(&StructureGoal::Inventory) {
            return Err(StructureRequestError::Target);
        }
        self.resource_policy.validate()
    }

    pub fn identity(&self) -> Result<String, StructureRequestError> {
        self.validate()?;
        let mut normalized = self.clone();
        let mut sources = normalized
            .source_scope
            .into_iter()
            .map(|source| canonical_identity("source-capture-v1", &source).map(|id| (id, source)))
            .collect::<Result<Vec<_>, _>>()?;
        sources.sort_by(|left, right| left.0.cmp(&right.0));
        sources.dedup_by(|left, right| left.0 == right.0);
        normalized.source_scope = sources.into_iter().map(|(_, source)| source).collect();
        let mut targets = normalized
            .targets
            .into_iter()
            .map(|target| canonical_identity("target-selector-v1", &target).map(|id| (id, target)))
            .collect::<Result<Vec<_>, _>>()?;
        targets.sort_by(|left, right| left.0.cmp(&right.0));
        targets.dedup_by(|left, right| left.0 == right.0);
        normalized.targets = targets.into_iter().map(|(_, target)| target).collect();
        Ok(canonical_identity("structure-request-v1", &normalized)?)
    }
}

impl<'de> Deserialize<'de> for StructureResourcePolicy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let policy = ResourcePolicyRecord::deserialize(deserializer)?;
        policy.validate().map_err(serde::de::Error::custom)?;
        Ok(policy)
    }
}

#[derive(Deserialize)]
#[serde(
    remote = "StructureResourcePolicy",
    rename_all = "camelCase",
    deny_unknown_fields
)]
struct ResourcePolicyRecord {
    bytes: u64,
    readers: u32,
    handles: u32,
    cpu: u32,
    io_domains: u32,
    priority: ReadPriority,
    deadline_unix_ms: Option<u64>,
    active_work_items: u32,
    page_rows: u32,
    page_bytes: u64,
}

impl<'de> Deserialize<'de> for StructureRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let request = RequestRecord::deserialize(deserializer)?;
        request.validate().map_err(serde::de::Error::custom)?;
        Ok(request)
    }
}

#[derive(Deserialize)]
#[serde(
    remote = "StructureRequest",
    rename_all = "camelCase",
    deny_unknown_fields
)]
struct RequestRecord {
    case_id: String,
    source_scope: Vec<SourceCaptureRef>,
    targets: Vec<TargetSelector>,
    version_selection: VersionSelection,
    goals: BTreeSet<StructureGoal>,
    analysis_profiles: BTreeSet<String>,
    resource_policy: StructureResourcePolicy,
    adapter_hints: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StructureRequestError {
    #[error("structure request source scope is invalid")]
    Scope,
    #[error("structure request target is invalid")]
    Target,
    #[error("structure request version selection is invalid")]
    Version,
    #[error("structure request resource policy is invalid")]
    ResourcePolicy,
    #[error("structure request contains an empty profile or adapter hint")]
    EmptyLabel,
    #[error("analysis requires an explicit analysis profile")]
    MissingAnalysisProfile,
    #[error(transparent)]
    Identity(#[from] CanonicalIdentityError),
}
