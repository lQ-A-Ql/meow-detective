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

/// Resource limits for the batch execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResourceLimits {
    pub max_memory_mb: Option<u64>,
    pub max_threads: Option<u32>,
}

/// The plan that describes what a batch job will execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPlan {
    pub data_source_refs: Vec<String>,
    pub phases: Vec<PhaseKind>,
    pub resource_limits: BatchResourceLimits,
}
