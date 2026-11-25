use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkReportDto, ImportBenchmarkSampleDto,
    ImportBenchmarkStateDto as State,
};

#[test]
fn writer_and_path_metrics_keep_supported_zero_and_legacy_absence() {
    let legacy = serde_json::json!({"phase":"mftCommit","state":"failed","inclusive":false,"elapsedNs":1,
        "scannedRecords":0,"parsedRecords":0,"stagedRows":0,"logicalBytesRead":0,"fullImageOpens":0,"filesystemBuilds":0,"failures":1});
    let old: ImportBenchmarkSampleDto = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(old.writer_batches, None);
    assert_eq!(old.partition_commits, None);
    assert_eq!(old.max_reserved_mft_bytes, None);
    assert_eq!(old.mft_readers_actual, None);
    assert_eq!(old.actual_chunk_records, None);
    let mut current = legacy;
    current["maxReservedMftBytes"] = serde_json::json!(0);
    current["mftReadersActual"] = serde_json::json!(0);
    current["actualChunkRecords"] = serde_json::json!(0);
    current["partitionCommits"] = serde_json::json!(0);
    current["writerBatches"] = serde_json::json!(0);
    current["materializedPathRows"] = serde_json::json!(0);
    let sample: ImportBenchmarkSampleDto = serde_json::from_value(current).unwrap();
    let encoded = serde_json::to_value(sample).unwrap();
    assert_eq!(encoded["maxReservedMftBytes"], 0);
    assert_eq!(encoded["mftReadersActual"], 0);
    assert_eq!(encoded["actualChunkRecords"], 0);
    assert_eq!(encoded["partitionCommits"], 0);
    assert_eq!(encoded["writerBatches"], 0);
    assert_eq!(encoded["materializedPathRows"], 0);
    assert!(encoded.get("readChunkRecords").is_none());
}

#[test]
fn benchmark_report_round_trip_preserves_scopes_and_missing_measurements() {
    let report = ImportBenchmarkReportDto {
        schema_version: 1,
        samples: vec![ImportBenchmarkSampleDto {
            phase: Phase::SourceIndexBuild,
            partition_index: None,
            state: State::NotApplicable,
            inclusive: false,
            elapsed_ns: None,
            scanned_records: 0,
            parsed_records: 0,
            staged_rows: 0,
            logical_bytes_read: 0,
            physical_bytes_read: None,
            full_image_opens: 0,
            filesystem_builds: 0,
            failures: 0,
            max_buffered_batches: None,
            max_reserved_mft_bytes: None,
            mft_readers_actual: None,
            actual_chunk_records: None,
            max_buffered_records: None,
            main_table_updated_rows: None,
            materialized_path_rows: None,
            writer_batches: None,
            writer_batch_max_records: None,
            read_chunk_records: None,
            partition_commits: None,
            staging_logical_bytes: None,
            sampled_wal_max_bytes: None,
            records_per_sec: None,
            logical_mb_per_sec: None,
        }],
        partition_aggregates: Vec::new(),
        rss_at_end_mb: 128,
        process_peak_rss_mb: 256,
        process_usage: None,
        observation_limits: Vec::new(),
    };
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["samples"][0]["state"], "notApplicable");
    assert!(value["samples"][0].get("elapsedNs").is_none());
    assert!(value["samples"][0].get("physicalBytesRead").is_none());
    assert_eq!(
        serde_json::from_value::<ImportBenchmarkReportDto>(value).unwrap(),
        report
    );
}

#[test]
fn workflow_process_usage_preserves_unavailable_and_supported_zero_counters() {
    use transport::dto::import_benchmark::{
        ImportBenchmarkProcessScopeDto, ImportBenchmarkProcessUsageDto,
    };
    let value = serde_json::json!({
        "scope":"workflowProcessWindow", "wallTimeNs":1000000000,
        "userCpuNs":1500000000, "kernelCpuNs":500000000, "cpuCoreEquivalents":2.0,
        "processReadTransferBytes":0, "processWriteTransferBytes":10,
        "handlesAtStart":40, "handlesAtEnd":42
    });
    let usage: ImportBenchmarkProcessUsageDto = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        usage.scope,
        ImportBenchmarkProcessScopeDto::WorkflowProcessWindow
    );
    assert_eq!(usage.process_read_transfer_bytes, Some(0));
    assert_eq!(usage.process_read_operations, None);
    assert_eq!(serde_json::to_value(usage).unwrap(), value);
}

#[test]
fn observation_limits_round_trip_without_becoming_zero_measurements() {
    use transport::dto::import_benchmark::ImportBenchmarkObservationLimitDto as Limit;
    for limit in [
        Limit::ProcessCountersSharedAcrossConcurrentJobs,
        Limit::DevicePhysicalIoNotMeasured,
        Limit::SqliteWalSizeSampledAtBatchBoundaries,
        Limit::AnonymousSortFilePeakNotMeasured,
        Limit::ChannelQueueDelayNotApplicableToWavePipeline,
    ] {
        assert_eq!(
            serde_json::from_value::<Limit>(serde_json::to_value(limit).unwrap()).unwrap(),
            limit
        );
    }
}

#[test]
fn all_benchmark_phases_and_states_round_trip() {
    for phase in [
        Phase::Workflow,
        Phase::Attach,
        Phase::SourceDatabaseOpen,
        Phase::SourceMetadataInitialize,
        Phase::RuntimePrepare,
        Phase::WorkspaceAdmission,
        Phase::BulkLoadBegin,
        Phase::Catalog,
        Phase::Finalize,
        Phase::OutcomePersistence,
        Phase::PostReady,
        Phase::Cleanup,
        Phase::Probe,
        Phase::ReaderBuild,
        Phase::Enumeration,
        Phase::MftReaderOpen,
        Phase::MftParams,
        Phase::MftRead,
        Phase::MftParse,
        Phase::MftReadParseWall,
        Phase::MftWriterWall,
        Phase::MftStage,
        Phase::MftDirReaderBuild,
        Phase::MftSizeReconcile,
        Phase::MftDirBackfill,
        Phase::MftPathResolve,
        Phase::MftPathCompute,
        Phase::MftPathSql,
        Phase::MftValidate,
        Phase::MftCommit,
        Phase::FallbackReaderBuild,
        Phase::FallbackEnumeration,
        Phase::StagingMerge,
        Phase::SourceIndexBuild,
        Phase::Graph,
        Phase::Analysis,
        Phase::Timeline,
        Phase::TimelineEvents,
        Phase::TimelineGraph,
        Phase::TimelineEventClear,
        Phase::TimelineEventCursor,
        Phase::TimelineEventInsert,
        Phase::TimelineEventIndexes,
        Phase::TimelineEventCommit,
        Phase::TimelineGraphPage,
        Phase::TimelineGraphNodePrepare,
        Phase::TimelineGraphEdgePrepare,
        Phase::TimelineGraphPagePrepare,
        Phase::TimelineGraphNodes,
        Phase::TimelineGraphEdges,
        Phase::TimelineGraphCommit,
        Phase::Checkpoint,
    ] {
        assert_eq!(
            serde_json::from_value::<Phase>(serde_json::to_value(phase).unwrap()).unwrap(),
            phase
        );
    }
    for state in [
        State::Completed,
        State::Cancelled,
        State::Failed,
        State::NotApplicable,
    ] {
        assert_eq!(
            serde_json::from_value::<State>(serde_json::to_value(state).unwrap()).unwrap(),
            state
        );
    }
}
