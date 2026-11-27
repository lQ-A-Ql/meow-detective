use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    AddressSpace, ByteRangeSet, ReadDemand, ReadPriority, SourceRevisionRef, StructureGraph,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterOperation {
    Probe,
    Inspect,
    Bind,
    SelectVersion,
    Plan,
    Verify,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureAdapterDescriptor {
    pub id: String,
    pub version: String,
    pub formats: Vec<String>,
    pub features: BTreeSet<String>,
    pub input_spaces: Vec<AddressSpace>,
    pub output_spaces: Vec<AddressSpace>,
    pub operations: BTreeSet<AdapterOperation>,
    pub max_workers: u32,
    pub requires_independent_readers: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureWorkItem {
    pub id: String,
    pub source_revision: SourceRevisionRef,
    pub target_fingerprint: String,
    pub adapter_id: String,
    pub adapter_version: String,
    pub ranges: ByteRangeSet,
    pub intent: super::ReadIntent,
    pub priority: ReadPriority,
    pub dependencies: Vec<String>,
    pub consumers: Vec<String>,
    pub demand: Option<ReadDemand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureAnalysisPlan {
    pub graph: StructureGraph,
    pub adapters: Vec<StructureAdapterDescriptor>,
    pub work_items: Vec<StructureWorkItem>,
    pub stages: Vec<StructureStage>,
}

impl StructureAnalysisPlan {
    pub fn validate(&self) -> Result<(), StructurePlanError> {
        if self.stages != StructureStage::all() {
            return Err(StructurePlanError::InvalidStages);
        }
        self.graph
            .validate()
            .map_err(|error| StructurePlanError::Graph(error.to_string()))?;
        let adapters = self
            .adapters
            .iter()
            .map(|adapter| (adapter.id.as_str(), adapter))
            .collect::<std::collections::BTreeMap<_, _>>();
        if adapters.len() != self.adapters.len() {
            return Err(StructurePlanError::DuplicateAdapter);
        }
        let work_ids = self
            .work_items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<BTreeSet<_>>();
        if work_ids.len() != self.work_items.len() {
            return Err(StructurePlanError::DuplicateWorkItem);
        }
        for item in &self.work_items {
            let adapter = adapters
                .get(item.adapter_id.as_str())
                .ok_or_else(|| StructurePlanError::UnknownAdapter(item.adapter_id.clone()))?;
            if adapter.version != item.adapter_version {
                return Err(StructurePlanError::AdapterVersionMismatch(
                    item.adapter_id.clone(),
                ));
            }
            if !adapter.operations.contains(&AdapterOperation::Plan) {
                return Err(StructurePlanError::UnsupportedOperation(
                    item.adapter_id.clone(),
                ));
            }
            for dependency in &item.dependencies {
                if !work_ids.contains(dependency.as_str()) {
                    return Err(StructurePlanError::MissingDependency(dependency.clone()));
                }
            }
            if item.ranges.target_size() == 0 {
                return Err(StructurePlanError::EmptyTarget(item.id.clone()));
            }
            item.ranges
                .validate()
                .map_err(|error| StructurePlanError::Range(error.to_string()))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureStage {
    SourceFreeze,
    BoundedDiscovery,
    IdentityBinding,
    VersionSelection,
    TargetPlanning,
    ReadVerification,
    AtomicPublication,
    InvestigationAnalysis,
}

impl StructureStage {
    pub const ALL: [Self; 8] = [
        Self::SourceFreeze,
        Self::BoundedDiscovery,
        Self::IdentityBinding,
        Self::VersionSelection,
        Self::TargetPlanning,
        Self::ReadVerification,
        Self::AtomicPublication,
        Self::InvestigationAnalysis,
    ];

    pub fn all() -> Vec<Self> {
        Self::ALL.to_vec()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StructurePlanError {
    #[error("structure graph is invalid: {0}")]
    Graph(String),
    #[error("structure plan contains duplicate adapters")]
    DuplicateAdapter,
    #[error("structure plan contains duplicate work items")]
    DuplicateWorkItem,
    #[error("structure plan references an unknown adapter: {0}")]
    UnknownAdapter(String),
    #[error("structure plan adapter version is inconsistent: {0}")]
    AdapterVersionMismatch(String),
    #[error("structure plan adapter cannot plan reads: {0}")]
    UnsupportedOperation(String),
    #[error("structure plan work item depends on a missing item: {0}")]
    MissingDependency(String),
    #[error("structure plan target is empty: {0}")]
    EmptyTarget(String),
    #[error("structure plan range is invalid: {0}")]
    Range(String),
    #[error("structure plan execution stages are incomplete or out of order")]
    InvalidStages,
}
