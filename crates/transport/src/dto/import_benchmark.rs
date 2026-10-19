use serde::{Deserialize, Serialize};

/// Internal benchmark phases; these do not add import IPC event topics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportBenchmarkPhaseDto {
    Workflow,
    Attach,
    SourceDatabaseOpen,
    SourceMetadataInitialize,
    RuntimePrepare,
    WorkspaceAdmission,
    BulkLoadBegin,
    Catalog,
    Finalize,
    OutcomePersistence,
    PostReady,
    Cleanup,
    Probe,
    ReaderBuild,
    Enumeration,
    MftReaderOpen,
    MftParams,
    MftRead,
    MftParse,
    MftReadParseWall,
    MftWriterWall,
    MftStage,
    MftDirReaderBuild,
    MftSizeReconcile,
    MftDirBackfill,
    MftPathResolve,
    MftPathCompute,
    MftPathSql,
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
    TimelineEventClear,
    TimelineEventCursor,
    TimelineEventInsert,
    TimelineEventIndexes,
    TimelineEventCommit,
    TimelineGraphPage,
    TimelineGraphNodePrepare,
    TimelineGraphEdgePrepare,
    TimelineGraphPagePrepare,
    TimelineGraphNodes,
    TimelineGraphEdges,
    TimelineGraphCommit,
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
    /// Maximum buffered batches within this partition; aggregates use max, not sum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_buffered_batches: Option<u64>,
    /// Conservative reservation covering read buffers and parsed payloads; not RSS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_reserved_mft_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mft_readers_actual: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_chunk_records: Option<u64>,
    /// Parsed records held by the wave; not allocated or physical bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_buffered_records: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_table_updated_rows: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialized_path_rows: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_batches: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_batch_max_records: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_chunk_records: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_commits: Option<u64>,
    /// Staging page_count * page_size; excludes WAL and is not peak temp usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staging_logical_bytes: Option<u64>,
    /// Sampled WAL file length, not physical writes or an exact instantaneous peak.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampled_wal_max_bytes: Option<u64>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_usage: Option<ImportBenchmarkProcessUsageDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observation_limits: Vec<ImportBenchmarkObservationLimitDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportBenchmarkObservationLimitDto {
    ProcessCountersSharedAcrossConcurrentJobs,
    DevicePhysicalIoNotMeasured,
    SqliteWalSizeSampledAtBatchBoundaries,
    AnonymousSortFilePeakNotMeasured,
    ChannelQueueDelayNotApplicableToWavePipeline,
}

/// Process counters bracket the workflow, excluding the benchmark oracle.
/// Concurrent jobs share these counters; overlapping windows must not be summed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBenchmarkProcessUsageDto {
    pub scope: ImportBenchmarkProcessScopeDto,
    pub wall_time_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_cpu_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_cpu_ns: Option<u64>,
    /// CPU seconds / wall seconds; 2.0 means two cores, not two percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_core_equivalents: Option<f64>,
    /// OS process transfer counters, including cached/device/network I/O, not physical disk bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_read_transfer_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_write_transfer_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_read_operations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_write_operations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handles_at_start: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handles_at_end: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportBenchmarkProcessScopeDto {
    WorkflowProcessWindow,
}
