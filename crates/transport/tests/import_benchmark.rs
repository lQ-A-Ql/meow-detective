use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkReportDto, ImportBenchmarkSampleDto,
    ImportBenchmarkStateDto as State,
};

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
            records_per_sec: None,
            logical_mb_per_sec: None,
        }],
        partition_aggregates: Vec::new(),
        rss_at_end_mb: 128,
        process_peak_rss_mb: 256,
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
fn all_benchmark_phases_and_states_round_trip() {
    for phase in [
        Phase::Workflow,
        Phase::Probe,
        Phase::ReaderBuild,
        Phase::Enumeration,
        Phase::MftReaderOpen,
        Phase::MftParams,
        Phase::MftRead,
        Phase::MftParse,
        Phase::MftStage,
        Phase::MftDirReaderBuild,
        Phase::MftSizeReconcile,
        Phase::MftDirBackfill,
        Phase::MftPathResolve,
        Phase::MftValidate,
        Phase::MftCommit,
        Phase::FallbackReaderBuild,
        Phase::FallbackEnumeration,
        Phase::StagingMerge,
        Phase::SourceIndexBuild,
        Phase::Graph,
        Phase::Analysis,
        Phase::Timeline,
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
