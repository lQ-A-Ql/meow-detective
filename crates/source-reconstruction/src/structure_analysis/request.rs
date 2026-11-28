use std::collections::BTreeSet;

use domain::{
    structure::{ReadIntent, ReadPriority, StructureAnalysisPlan, StructureAttemptRef},
    CaseId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureAnalysisRequest {
    pub case_id: CaseId,
    pub source_ids: Vec<String>,
    pub intent: ReadIntent,
    pub priority: ReadPriority,
    pub consumers: Vec<String>,
    pub attempt: Option<StructureAttemptRef>,
    pub required_features: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureGraphRequest {
    pub case_id: CaseId,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureAnalysisRun {
    pub plan: StructureAnalysisPlan,
    pub work_results: Vec<StructureWorkResult>,
    pub verification_stats: source_kernel::source_runtime::verification::VerificationBatchStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureWorkResult {
    pub work_item_id: String,
    pub bytes_read: u64,
    pub status: StructureWorkStatus,
    pub receipt: domain::structure::VerificationReceipt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureWorkStatus {
    Verified,
}

#[derive(Debug, thiserror::Error)]
pub enum StructureAnalysisError {
    #[error(transparent)]
    Database(#[from] persistence_sqlite::DbError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    MdRaid(#[from] evidence_access::mdraid::MdRaidPlanError),
    #[error(transparent)]
    Lvm(#[from] evidence_access::lvm::LvmPlanError),
    #[error(transparent)]
    Access(#[from] evidence_access::structure_access::TargetAccessError),
    #[error(transparent)]
    Observation(#[from] evidence_access::structure_observation::StructureObservationError),
    #[error("structure graph is invalid: {0}")]
    Graph(String),
    #[error("structure plan is invalid: {0}")]
    Plan(String),
    #[error("requested source is missing: {0}")]
    MissingSource(String),
    #[error("source is not registered in the requested case: {0}")]
    SourceCaseMismatch(String),
    #[error("source revision is missing: {0}")]
    MissingRevision(String),
    #[error("source has no addressable size: {0}")]
    EmptySource(String),
    #[error("structure adapter is unsupported: {0}")]
    UnsupportedAdapter(String),
    #[error("structure adapter feature is unsupported: {adapter}:{feature}")]
    UnsupportedFeature { adapter: String, feature: String },
    #[error("structure adapter registry is invalid: {0}")]
    AdapterRegistry(String),
    #[error("structure work-item bound was exceeded")]
    WorkItemLimit,
    #[error("structure analysis cancelled")]
    Cancelled,
}

impl From<rusqlite::Error> for StructureAnalysisError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.into())
    }
}
