use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResourceLimitsDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_threads: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPhaseDto {
    pub kind: String,
    pub state: String,
    pub progress: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub error_count: u32,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPlanDto {
    pub data_source_refs: Vec<String>,
    pub phases: Vec<String>,
    pub resource_limits: BatchResourceLimitsDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchJobDto {
    pub id: String,
    pub case_id: String,
    pub label: String,
    pub plan: BatchPlanDto,
    pub phases: Vec<BatchPhaseDto>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub status: String,
}

/// Request payload to resume a paused batch job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResumeDto {
    pub batch_id: String,
    /// Optional override of resource limits on resume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_limits: Option<BatchResourceLimitsDto>,
}
