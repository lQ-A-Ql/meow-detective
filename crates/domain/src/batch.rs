use serde::{Deserialize, Serialize};

/// Ordered phases in a batch pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PhaseKind {
    Mount,
    Catalog,
    ExtractArtifacts,
    Index,
    Correlate,
    Export,
}

/// Runtime state of a single phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PhaseState {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
}

/// Resource limits for the batch execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResourceLimits {
    pub max_memory_mb: Option<u64>,
    pub max_threads: Option<u32>,
}

/// A single phase within a batch job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPhase {
    pub kind: PhaseKind,
    pub state: PhaseState,
    pub progress: f64,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_count: u32,
    pub warnings: Vec<String>,
}

/// The plan that describes what a batch job will execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPlan {
    pub data_source_refs: Vec<String>,
    pub phases: Vec<PhaseKind>,
    pub resource_limits: BatchResourceLimits,
}

/// Top-level batch job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    pub id: String,
    pub case_id: String,
    pub label: String,
    pub plan: BatchPlan,
    pub phases: Vec<BatchPhase>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub status: String,
}
