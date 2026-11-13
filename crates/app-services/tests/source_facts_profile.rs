use app_services::{
    case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{
        execute_import_job_profiled, ImportEventSink, ImportJobOptions, ImportProfiler,
    },
    import_precheck,
};
use domain::DataSourcePlatform;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use transport::dto::{
    import_benchmark::{
        ImportBenchmarkPhaseDto as Phase, ImportBenchmarkReportDto,
        ImportBenchmarkStateDto as State,
    },
    DataSourceSummaryDto, ImportPhaseDto, ImportPhaseProgressDto, ImportPhaseStateDto,
    IndexCacheStatusDto, JobCancellationDto, PartialResultDto,
};

#[derive(Default)]
struct RecordingSink(Mutex<Vec<ImportPhaseProgressDto>>);

impl ImportEventSink for RecordingSink {
    fn job_progress(&self, _: &str, _: u32, _: &str) {}
    fn data_source_imported(&self, _: &str, _: &DataSourceSummaryDto, _: &str) {}
    fn partition_progress(&self, _: &str, _: &str, _: u32, _: u32, _: u32) {}
    fn timeline_updated(&self, _: u64) {}
    fn search_index_progress(&self, _: u32, _: &str) {}
    fn import_phase_progress(&self, value: &ImportPhaseProgressDto) {
        self.0.lock().unwrap().push(value.clone());
    }
    fn import_partial_result(&self, _: &PartialResultDto) {}
    fn cache_index_status(&self, _: &IndexCacheStatusDto) {}
    fn job_cancellation(&self, _: &JobCancellationDto) {}
}

fn assert_facts_measurements(report: &ImportBenchmarkReportDto) {
    let mut child_ns = 0;
    let mut total_ns = 0;
    for phase in [
        Phase::SourceFactsCollect,
        Phase::SourceFactsPersist,
        Phase::SourceFacts,
    ] {
        let samples = report
            .samples
            .iter()
            .filter(|sample| sample.phase == phase)
            .collect::<Vec<_>>();
        assert_eq!(samples.len(), 1);
        let sample = samples[0];
        assert_eq!(sample.state, State::Completed);
        assert_eq!(sample.staged_rows, 1);
        let elapsed = sample.elapsed_ns.unwrap();
        assert!(elapsed > 0);
        assert_eq!(sample.inclusive, phase == Phase::SourceFacts);
        if phase == Phase::SourceFacts {
            total_ns = elapsed;
        } else {
            child_ns += elapsed;
        }
    }
    assert!(total_ns >= child_ns);
}

fn persisted_report(path: &Path) -> ImportBenchmarkReportDto {
    let directory = fs::read_dir(path)
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| entry.file_type().unwrap().is_dir())
        .unwrap()
        .path();
    let mut profiles = fs::read_dir(directory)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-profile.json")
        })
        .collect::<Vec<_>>();
    profiles.sort();
    serde_json::from_slice(&fs::read(profiles.last().unwrap()).unwrap()).unwrap()
}

#[test]
fn real_linux_import_records_and_persists_source_facts_stage_timings() {
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/benchmark_pipeline/source-facts");
    fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let evidence = root.path().join("evidence");
    fs::create_dir_all(evidence.join("etc")).unwrap();
    fs::write(evidence.join("etc/hostname"), b"fixture-host\n").unwrap();
    fs::write(
        evidence.join("etc/os-release"),
        b"NAME=FixtureLinux\nVERSION_ID=1\n",
    )
    .unwrap();
    let active =
        case_service::create_case(&root.path().join("cases"), "facts-profile", None).unwrap();
    let output = root.path().join("profile");
    let profiler = ImportProfiler::with_output_directory(&output).unwrap();
    let sink = RecordingSink::default();
    active.with_conn(|connection| {
        let job = JobRepo::new(connection).create(&active.meta.id.0, "facts profile")?;
        JobRepo::new(connection).start(&job)?;
        let config = import_precheck::prepare_import_source_config_from_path(
            &evidence.to_string_lossy(), DataSourcePlatform::Linux,
        ).unwrap();
        execute_import_job_profiled(
            connection, &active.meta.id, &active.case_root, config, &job,
            ImportJobOptions {
                projection_policy: Default::default(),
                event_sink: Some(&sink),
                cancel_token: &Arc::new(AtomicBool::new(false)),
                max_import_workers: Some(1),
                max_analysis_workers: Some(1),
                analysis_mode: ImportAnalysisMode::MetadataOnly,
                defer_linux_artifacts: false,
            },
            &profiler,
        ).unwrap();
        let sources = DataSourceRepo::new(connection).find_by_case(&active.meta.id)?;
        assert_eq!(sources.len(), 1);
        let source = app_services::source_db::open_registered_source_db_read_only(
            connection, &active.case_root, &sources[0].id,
        )?;
        let json = persistence_sqlite::repositories::linux_evidence_facts_repo::LinuxEvidenceFactsRepo::new(&source)
            .find_json(&sources[0].id)?.unwrap();
        let facts: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(facts["hostname"], "fixture-host");
        Ok(())
    }).unwrap();
    assert_facts_measurements(&profiler.report());
    assert_facts_measurements(&persisted_report(&output));
    let events = sink.0.lock().unwrap();
    let facts = events
        .iter()
        .filter(|event| event.detail.contains("phase=source-facts"))
        .collect::<Vec<_>>();
    assert_eq!(facts.len(), 5);
    for stage in ["collect", "persist", "total"] {
        let event = facts
            .iter()
            .find(|event| {
                event.detail.contains(&format!("stage={stage} "))
                    && event.state == ImportPhaseStateDto::Completed
            })
            .unwrap();
        assert_eq!(event.phase, ImportPhaseDto::PostReady);
        assert!(event.metrics.elapsed_ms.is_some());
        assert_eq!(event.metrics.rows_processed, 1);
    }
    assert_eq!(
        fs::read(evidence.join("etc/hostname")).unwrap(),
        b"fixture-host\n"
    );
}
