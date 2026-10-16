use serde::{Deserialize, Serialize};

/// Internal benchmark phases; these do not add import IPC event topics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportBenchmarkPhaseDto {
    Workflow,
    Probe,
    ReaderBuild,
    Enumeration,
    MftReaderOpen,
    MftParams,
    MftRead,
    MftParse,
    MftStage,
    MftDirReaderBuild,
    MftSizeReconcile,
    MftDirBackfill,
    MftPathResolve,
    MftValidate,
    MftCommit,
    FallbackReaderBuild,
    FallbackEnumeration,
    StagingMerge,
    SourceIndexBuild,
    Graph,
    Analysis,
    Timeline,
    TimelineEvents,
    TimelineGraph,
    Checkpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportBenchmarkStateDto {
    NotApplicable,
    Completed,
    Cancelled,
    Failed,
}

/// Observations are scoped to one workflow, without evidence paths or payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBenchmarkSampleDto {
    pub phase: ImportBenchmarkPhaseDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_index: Option<usize>,
    pub state: ImportBenchmarkStateDto,
    /// Parent wall times include children and must not be summed with them.
    pub inclusive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ns: Option<u64>,
    pub scanned_records: u64,
    pub parsed_records: u64,
    pub staged_rows: u64,
    pub logical_bytes_read: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_bytes_read: Option<u64>,
    pub full_image_opens: u64,
    pub filesystem_builds: u64,
    pub failures: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub records_per_sec: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical_mb_per_sec: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBenchmarkReportDto {
    pub schema_version: u32,
    /// Direct source wall measurements and all partition measurements.
    pub samples: Vec<ImportBenchmarkSampleDto>,
    /// Sum of partition work durations, not parallel source wall time.
    pub partition_aggregates: Vec<ImportBenchmarkSampleDto>,
    pub rss_at_end_mb: u64,
    pub process_peak_rss_mb: u64,
}
