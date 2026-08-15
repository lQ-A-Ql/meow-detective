use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportPhaseDto {
    Queued,
    Attach,
    Probe,
    Enumerate,
    MergeEnumeration,
    Analyze,
    MergeAnalysis,
    HashEvidence,
    BuildIndexes,
    Finalize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportPhaseStateDto {
    Pending,
    Running,
    Completed,
    Skipped,
    Cancelling,
    Cancelled,
    Failed,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPhaseMetricsDto {
    pub elapsed_ms: u64,
    pub rss_mb: u64,
    pub workers: u32,
    pub rows_processed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows_per_sec: Option<f64>,
    pub bytes_processed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mb_per_sec: Option<f64>,
    pub warnings: u32,
    pub skipped: u32,
    pub failed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPhaseProgressDto {
    pub job_id: String,
    pub case_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source_id: Option<String>,
    pub phase: ImportPhaseDto,
    pub state: ImportPhaseStateDto,
    pub percent: u32,
    pub detail: String,
    pub metrics: ImportPhaseMetricsDto,
    pub partial_results: Vec<PartialResultDto>,
    pub cancellable: bool,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartialResultKindDto {
    FileTree,
    FileRows,
    Partition,
    TimelineEvents,
    TimelineBuckets,
    ArtifactFamily,
    SearchIndex,
    EvidenceHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultFreshnessDto {
    Ready,
    Partial,
    Deferred,
    Stale,
    Invalidated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialResultDto {
    pub kind: PartialResultKindDto,
    pub scope_id: String,
    pub ready_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_estimate: Option<u64>,
    pub query_key: String,
    pub freshness: ResultFreshnessDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancelReasonDto {
    UserRequested,
    CaseClosing,
    MemoryLimit,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancellationStateDto {
    NotRequested,
    Requested,
    Acknowledged,
    Draining,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobCancellationDto {
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_at: Option<String>,
    pub state: CancellationStateDto,
    pub safe_to_close: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexCacheStatusDto {
    pub cache_key: String,
    pub state: String,
    pub indexed_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_count: Option<u64>,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceReportSummaryDto {
    pub report_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub generated_at: String,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_memory_bytes: Option<u64>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceMetricDto {
    pub key: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceReportDto {
    pub summary: PerformanceReportSummaryDto,
    pub metrics: Vec<PerformanceMetricDto>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/import.rs"]
mod tests;
