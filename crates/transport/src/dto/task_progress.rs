use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStageKeyDto {
    Validate,
    Register,
    Prepare,
    Probe,
    Enumerate,
    Analyze,
    Index,
    Publish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskProgressStateDto {
    Pending,
    Running,
    Completed,
    Skipped,
    Cancelling,
    Cancelled,
    Failed,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskProgressKindDto {
    Import,
    NestedImport,
    Analysis,
    Timeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskProgressModeDto {
    Determinate,
    Estimated,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskProgressUnitDto {
    Items,
    Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStageProgressDto {
    pub key: TaskStageKeyDto,
    pub weight: u32,
    pub state: TaskProgressStateDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgressDto {
    pub job_id: String,
    pub case_id: String,
    pub kind: TaskProgressKindDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source_id: Option<String>,
    pub seq: u64,
    pub state: TaskProgressStateDto,
    pub progress: u32,
    pub mode: TaskProgressModeDto,
    pub stage: TaskStageKeyDto,
    pub stages: Vec<TaskStageProgressDto>,
    pub completed_units: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_units: Option<u64>,
    pub unit: TaskProgressUnitDto,
    pub elapsed_ms: u64,
    pub stage_elapsed_ms: u64,
    pub heartbeat_at: String,
    pub heartbeat: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    /// Remaining time for the current work counter's known total.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u64>,
    pub cancellable: bool,
}
