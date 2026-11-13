use app_services::{
    active_case::ActiveCase,
    case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{
        execute_import_job_profiled, ImportEventSink, ImportJobOptions, ImportProfiler,
    },
    import_precheck, source_db,
};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, UNIX_EPOCH},
};
use transport::dto::{
    DataSourceSummaryDto, ImportPhaseProgressDto, IndexCacheStatusDto, JobCancellationDto,
    PartialResultDto,
};

#[path = "support/real_sample_analysis_diagnostic.rs"]
mod analysis_diagnostic;
#[path = "support/real_sample_chromium_metadata.rs"]
mod chromium_metadata;
#[path = "support/real_sample_query_checks.rs"]
mod query_checks;

#[test]
#[ignore = "requires explicit FORENSICS_ANALYSIS_CASE_ROOT, FORENSICS_ANALYSIS_SOURCE_ID and FORENSICS_PERF_OUTPUT"]
fn retained_source_analysis_diagnostic() {
    analysis_diagnostic::run();
}

#[test]
#[ignore = "requires explicit FORENSICS_ANALYSIS_CASE_ROOT, FORENSICS_ANALYSIS_SOURCE_ID and FORENSICS_PERF_OUTPUT"]
fn retained_source_registry_metadata() {
    analysis_diagnostic::dump_registry_metadata();
}

#[test]
#[ignore = "requires explicit FORENSICS_ANALYSIS_CASE_ROOT, FORENSICS_ANALYSIS_SOURCE_ID and FORENSICS_PERF_OUTPUT"]
fn retained_source_chromium_metadata() {
    chromium_metadata::run();
}

#[test]
#[ignore = "requires explicit FORENSICS_PERF_WINDOWS_E01 and FORENSICS_PERF_OUTPUT"]
fn windows_real_sample_performance_acceptance() {
    run_sample("FORENSICS_PERF_WINDOWS_E01", DataSourcePlatform::Windows);
}

#[test]
#[ignore = "requires explicit FORENSICS_PERF_LINUX_E01 and FORENSICS_PERF_OUTPUT"]
fn linux_real_sample_performance_acceptance() {
    run_sample("FORENSICS_PERF_LINUX_E01", DataSourcePlatform::Linux);
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct InputMetadata {
    path: PathBuf,
    bytes: u64,
    modified_unix_ns: u128,
    read_only: bool,
}

fn input_metadata(path: &Path) -> Vec<InputMetadata> {
    let plan = image_e01::E01Reader::open(path)
        .unwrap()
        .open_plan()
        .unwrap();
    plan.segment_paths()
        .into_iter()
        .map(|path| {
            let metadata = fs::metadata(&path).unwrap();
            InputMetadata {
                path,
                bytes: metadata.len(),
                modified_unix_ns: metadata
                    .modified()
                    .unwrap()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                read_only: metadata.permissions().readonly(),
            }
        })
        .collect()
}

fn output_directory(platform: DataSourcePlatform) -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("FORENSICS_PERF_OUTPUT").expect("set FORENSICS_PERF_OUTPUT"),
    );
    fs::create_dir_all(&root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    for variable in ["TEMP", "TMP"] {
        let configured =
            PathBuf::from(std::env::var_os(variable).expect("set TEMP and TMP under output root"));
        assert!(
            fs::canonicalize(configured).unwrap().starts_with(&root),
            "{variable} must be inside FORENSICS_PERF_OUTPUT"
        );
    }
    let run = root.join(format!(
        "{}-{}",
        platform.as_storage_str(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&run).unwrap();
    run
}

fn write_report(path: &Path, value: &Value) {
    let mut value = value.clone();
    let object = value.as_object_mut().unwrap();
    object.insert(
        "buildProfile".to_owned(),
        json!(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }),
    );
    object.insert("isDebug".to_owned(), json!(cfg!(debug_assertions)));
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    serde_json::to_writer_pretty(file, &value).unwrap();
}

fn run_sample(variable: &str, platform: DataSourcePlatform) {
    let fixture = fs::canonicalize(PathBuf::from(
        std::env::var_os(variable).expect("set explicit E01 fixture"),
    ))
    .unwrap();
    assert!(fixture.is_file());
    let output = output_directory(platform);
    let before = input_metadata(&fixture);
    write_report(
        &output.join("input.json"),
        &json!({"fixture":before, "identity":"metadata-only"}),
    );
    let active =
        case_service::create_case(&output.join("cases"), "real-sample-performance", None).unwrap();
    write_report(
        &output.join("case-location.json"),
        &json!({"caseRoot":active.case_root, "caseId":active.meta.id.0}),
    );
    eprintln!("REAL_SAMPLE_CASE {}", active.case_root.display());

    let profiler = ImportProfiler::with_output_directory(output.join("profile"))
        .expect("create persistent import profile");
    let sink = ProgressSink::default();
    let memory = RssSampler::start();
    let started = Instant::now();
    let imported = run_import(&active, &fixture, platform, &profiler, &sink);
    let import_elapsed_ms = started.elapsed().as_millis();
    let import_memory = memory.finish();
    let after_import = input_metadata(&fixture);
    write_report(
        &output.join("import.json"),
        &json!({
            "schemaVersion":1, "platform":platform.as_storage_str(), "elapsedMs":import_elapsed_ms,
            "memory":import_memory, "profile":profiler.report(), "phaseEvents":sink.report(),
            "analysisAudit":sink.analysis_report(),
            "inputMetadataUnchanged":before == after_import,
            "inputBefore":before, "inputAfter":after_import,
            "outcome":imported.as_ref().map(|(_, summary)| summary).map_err(|error| error.to_string())
        }),
    );
    let (source_id, summary) =
        imported.expect("real source import failed; case and import report retained");
    let analysis = json!({"mode":"automatic-platform-analysis", "deferred":false,
        "warningCount":summary["warningCount"], "failedCount":summary["failedCount"],
        "warningScope":"import-including-automatic-analysis"});
    write_report(&output.join("analysis.json"), &analysis);
    let queries = active
        .with_conn(|connection| {
            Ok(query_checks::run(
                connection,
                &active.case_root,
                &active.meta.id,
                &source_id,
            ))
        })
        .unwrap();
    let after = input_metadata(&fixture);
    let required_passed = summary["sourceCount"] == 1
        && summary["state"] == "ready"
        && summary["platform"] == platform.as_storage_str()
        && summary["failedCount"] == 0
        && before == after_import
        && before == after
        && queries["allRequiredPassed"] == true;
    let status = if !required_passed {
        "failed"
    } else if summary["warningCount"].as_u64().unwrap() > 0
        || summary["skippedCount"].as_u64().unwrap() > 0
    {
        "passedWithWarnings"
    } else {
        "passed"
    };
    let report = json!({
        "schemaVersion":1, "status":status, "caseRoot":active.case_root, "sourceId":source_id.0,
        "platform":platform.as_storage_str(), "import":summary, "analysis":analysis,
        "queries":queries, "inputMetadataUnchanged":before == after,
        "observationLimits":["Input metadata equality does not replace a full evidence digest", "RSS is a sampled observation"]
    });
    let report_path = output.join("acceptance.json");
    write_report(&report_path, &report);
    eprintln!("REAL_SAMPLE_ACCEPTANCE {}", report_path.display());
    assert_eq!(
        before, after_import,
        "evidence metadata changed during import"
    );
    assert_eq!(before, after, "evidence metadata changed during acceptance");
    assert_eq!(summary["sourceCount"], 1);
    assert_eq!(summary["state"], "ready");
    assert_eq!(summary["platform"], platform.as_storage_str());
    assert_eq!(
        summary["failedCount"], 0,
        "analysis failures; report retained"
    );
    assert_eq!(
        report["queries"]["allRequiredPassed"], true,
        "query acceptance failed; report retained"
    );
}

fn run_import(
    active: &ActiveCase,
    fixture: &Path,
    platform: DataSourcePlatform,
    profiler: &ImportProfiler,
    sink: &ProgressSink,
) -> persistence_sqlite::DbResult<(DataSourceId, Value)> {
    active.with_conn(|connection| {
        let job = JobRepo::new(connection).create(&active.meta.id.0, "real sample acceptance")?;
        JobRepo::new(connection).start(&job)?;
        let config = import_precheck::prepare_import_source_config_from_path(fixture.to_str().unwrap(), platform)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        let cancel = Arc::new(AtomicBool::new(false));
        let (message, counts) = execute_import_job_profiled(
            connection, &active.meta.id, &active.case_root, config, &job,
            ImportJobOptions {
                projection_policy: Default::default(), event_sink: Some(sink), cancel_token: &cancel,
                max_import_workers: Some(2), max_analysis_workers: Some(2),
                analysis_mode: ImportAnalysisMode::MetadataOnly, defer_linux_artifacts: false,
            }, profiler,
        ).map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        JobRepo::new(connection).complete_committed(&job, &message)?;
        let sources = DataSourceRepo::new(connection).find_by_case(&active.meta.id)?;
        let source = sources.first().expect("import must register a source");
        let storage = DataSourceRepo::new(connection).find_storage(&source.id)?.unwrap();
        let source_conn = source_db::open_registered_source_db_read_only(connection, &active.case_root, &source.id)?;
        let owner = persistence_source::detail::source_id(&source_conn)?.unwrap();
        assert_eq!(owner, source.id.0, "source storage owner mismatch");
        let source_path = source_db::source_db_path(&active.case_root, &source.id);
        assert!(source_path.starts_with(active.case_root.join("sources")));
        Ok((source.id.clone(), json!({
            "sourceCount":sources.len(), "state":storage.import_state, "platform":storage.platform,
            "sourceOwner":owner, "sourceDatabase":source_path, "warningCount":counts.warning_count,
            "skippedCount":counts.skipped_count, "failedCount":counts.failed_count,
            "workersRequested":2, "analysisMode":"metadataOnly", "deferLinuxArtifacts":false
        })))
    })
}

#[derive(Default)]
struct ProgressSink {
    last: Mutex<Option<Instant>>,
    phases: Mutex<BTreeMap<String, Value>>,
    analysis_audit: Mutex<Vec<String>>,
}

impl ProgressSink {
    fn emit(&self, value: Value) {
        let mut last = self.last.lock().unwrap();
        if last.is_none_or(|instant| instant.elapsed() >= Duration::from_secs(2)) {
            eprintln!("REAL_SAMPLE_PROGRESS {value}");
            *last = Some(Instant::now());
        }
    }

    fn report(&self) -> Vec<Value> {
        self.phases.lock().unwrap().values().cloned().collect()
    }

    fn analysis_report(&self) -> Vec<String> {
        self.analysis_audit.lock().unwrap().clone()
    }
}

impl ImportEventSink for ProgressSink {
    fn job_progress(&self, _: &str, progress: u32, detail: &str) {
        if detail.starts_with("analysis-audit: phase=")
            || detail.starts_with("analysis-audit: category=")
        {
            self.analysis_audit.lock().unwrap().push(detail.to_owned());
            eprintln!("REAL_SAMPLE_ANALYSIS {detail}");
            return;
        }
        self.emit(json!({"phase":"import","percent":progress}));
    }

    fn partition_progress(&self, _: &str, _: &str, completed: u32, total: u32, percent: u32) {
        self.emit(
            json!({"phase":"partition","completed":completed,"total":total,"percent":percent}),
        );
    }

    fn timeline_updated(&self, event_count: u64) {
        self.emit(json!({"phase":"timeline","events":event_count}));
    }

    fn search_index_progress(&self, progress: u32, _: &str) {
        self.emit(json!({"phase":"searchIndex","percent":progress}));
    }

    fn data_source_imported(&self, _: &str, _: &DataSourceSummaryDto, _: &str) {
        self.emit(json!({"phase":"sourceReady"}));
    }

    fn import_phase_progress(&self, progress: &ImportPhaseProgressDto) {
        let value = json!({"phase":progress.phase,"state":progress.state,"percent":progress.percent,
            "rows":progress.metrics.rows_processed,"bytes":progress.metrics.bytes_processed,
            "elapsedMs":progress.metrics.elapsed_ms,"warnings":progress.metrics.warnings,
            "skipped":progress.metrics.skipped,"failed":progress.metrics.failed});
        self.phases.lock().unwrap().insert(
            serde_json::to_string(&progress.phase).unwrap(),
            value.clone(),
        );
        self.emit(value);
    }

    fn import_partial_result(&self, progress: &PartialResultDto) {
        self.emit(json!({"phase":"partialReady","records":progress.ready_count}));
    }

    fn cache_index_status(&self, _: &IndexCacheStatusDto) {}

    fn job_cancellation(&self, _: &JobCancellationDto) {}
}

struct RssSampler {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<u64>>,
    start_mib: u64,
}

impl RssSampler {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let start_mib = app_services::runtime_resources::current_rss_mb();
        let worker = thread::spawn(move || {
            let mut peak = start_mib;
            while !worker_stop.load(Ordering::Acquire) {
                peak = peak.max(app_services::runtime_resources::current_rss_mb());
                thread::sleep(Duration::from_millis(2));
            }
            peak.max(app_services::runtime_resources::current_rss_mb())
        });
        Self {
            stop,
            worker: Some(worker),
            start_mib,
        }
    }

    fn finish(mut self) -> Value {
        let end_mib = app_services::runtime_resources::current_rss_mb();
        self.stop.store(true, Ordering::Release);
        let peak = self.worker.take().unwrap().join().unwrap();
        json!({"rssAtStartMiB":self.start_mib,"rssAtEndMiB":end_mib,"sampledPeakRssMiB":peak.max(end_mib),
            "peakDeltaMiB":peak.max(end_mib).saturating_sub(self.start_mib),"sampleIntervalMs":2,
            "processLifetimePeakRssMiB":app_services::runtime_resources::peak_rss_mb()})
    }
}

impl Drop for RssSampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
