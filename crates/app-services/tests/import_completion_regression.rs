use app_services::{
    case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{
        execute_import_job_profiled, ImportEventSink, ImportJobOptions, ImportProfiler,
    },
    import_precheck,
};
use domain::DataSourcePlatform;
use persistence_sqlite::repositories::{
    job_repo::JobRepo,
    notebook_repo::{NotebookRepo, StepFilters},
};
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use transport::dto::{
    DataSourceSummaryDto, ImportPhaseProgressDto, IndexCacheStatusDto, JobCancellationDto,
    PartialResultDto,
};

struct CancelAtAutomaticAnalysis {
    token: Arc<AtomicBool>,
    ready: AtomicUsize,
}

struct FailJournalAtAutomaticAnalysis {
    journal: std::path::PathBuf,
    failed: AtomicBool,
    ready: AtomicUsize,
}

impl ImportEventSink for FailJournalAtAutomaticAnalysis {
    fn job_progress(&self, _: &str, _: u32, detail: &str) {
        if detail.ends_with("artifact analysis started automatically")
            && !self.failed.swap(true, Ordering::AcqRel)
        {
            fs::rename(&self.journal, self.journal.with_extension("retained")).unwrap();
            fs::write(&self.journal, b"journal directory is no longer writable").unwrap();
        }
    }
    fn data_source_imported(&self, _: &str, _: &DataSourceSummaryDto, _: &str) {
        self.ready.fetch_add(1, Ordering::AcqRel);
    }
    fn partition_progress(&self, _: &str, _: &str, _: u32, _: u32, _: u32) {}
    fn timeline_updated(&self, _: u64) {}
    fn search_index_progress(&self, _: u32, _: &str) {}
    fn import_phase_progress(&self, _: &ImportPhaseProgressDto) {}
    fn import_partial_result(&self, _: &PartialResultDto) {}
    fn cache_index_status(&self, _: &IndexCacheStatusDto) {}
    fn job_cancellation(&self, _: &JobCancellationDto) {}
}

impl ImportEventSink for CancelAtAutomaticAnalysis {
    fn job_progress(&self, _: &str, _: u32, detail: &str) {
        if detail.ends_with("artifact analysis started automatically") {
            self.token.store(true, Ordering::Release);
        }
    }
    fn data_source_imported(&self, _: &str, _: &DataSourceSummaryDto, _: &str) {
        self.ready.fetch_add(1, Ordering::AcqRel);
    }
    fn partition_progress(&self, _: &str, _: &str, _: u32, _: u32, _: u32) {}
    fn timeline_updated(&self, _: u64) {}
    fn search_index_progress(&self, _: u32, _: &str) {}
    fn import_phase_progress(&self, _: &ImportPhaseProgressDto) {}
    fn import_partial_result(&self, _: &PartialResultDto) {}
    fn cache_index_status(&self, _: &IndexCacheStatusDto) {}
    fn job_cancellation(&self, _: &JobCancellationDto) {}
}

#[test]
fn automatic_analysis_cancellation_retains_cancelled_outcome_and_no_ready_event() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/import");
    fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let evidence = root.path().join("evidence");
    fs::create_dir(&evidence).unwrap();
    fs::write(
        evidence.join("notes.txt"),
        b"physical logical-directory sample",
    )
    .unwrap();
    let active =
        case_service::create_case(&root.path().join("cases"), "cancel-post-ready", None).unwrap();
    let token = Arc::new(AtomicBool::new(false));
    let sink = CancelAtAutomaticAnalysis {
        token: token.clone(),
        ready: AtomicUsize::new(0),
    };
    let profiler = ImportProfiler::with_output_directory(root.path().join("profile")).unwrap();
    active
        .with_conn(|connection| {
            let job = JobRepo::new(connection)
                .create(&active.meta.id.0, "Import")
                .unwrap();
            JobRepo::new(connection).start(&job).unwrap();
            let config = import_precheck::prepare_import_source_config(
                &evidence.to_string_lossy(),
                DataSourcePlatform::Windows,
                None,
            )
            .unwrap();
            let result = execute_import_job_profiled(
                connection,
                &active.meta.id,
                &active.case_root,
                config,
                &job,
                ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: Some(&sink),
                    cancel_token: &token,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: false,
                },
                &profiler,
            );
            assert_eq!(
                result.expect_err("automatic analysis must cancel").category,
                "cancelled"
            );
            let steps = NotebookRepo::new(connection)
                .list_steps(&active.meta.id.0, &StepFilters::default())
                .unwrap();
            assert_eq!(steps.len(), 1);
            assert_eq!(steps[0].success, Some(false));
            assert_eq!(steps[0].error_code.as_deref(), Some("CANCELLED"));
            Ok(())
        })
        .unwrap();
    assert!(token.load(Ordering::Acquire));
    assert_eq!(sink.ready.load(Ordering::Acquire), 0);
    let report = profiler.report();
    for phase in [
        transport::dto::import_benchmark::ImportBenchmarkPhaseDto::PostReady,
        transport::dto::import_benchmark::ImportBenchmarkPhaseDto::Workflow,
    ] {
        assert_eq!(
            report
                .samples
                .iter()
                .find(|sample| sample.phase == phase)
                .unwrap()
                .state,
            transport::dto::import_benchmark::ImportBenchmarkStateDto::Cancelled
        );
    }
}

#[test]
fn automatic_analysis_journal_failure_prevents_ready_and_successful_outcome() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/import");
    fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let evidence = root.path().join("evidence");
    fs::create_dir(&evidence).unwrap();
    fs::write(
        evidence.join("notes.txt"),
        b"physical journal failure input",
    )
    .unwrap();
    let active =
        case_service::create_case(&root.path().join("cases"), "failed-journal", None).unwrap();
    let token = Arc::new(AtomicBool::new(false));
    let output = root.path().join("profile");
    let profiler = ImportProfiler::with_output_directory(&output).unwrap();
    let sink = FailJournalAtAutomaticAnalysis {
        journal: fs::read_dir(&output)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
        failed: AtomicBool::new(false),
        ready: AtomicUsize::new(0),
    };
    active
        .with_conn(|connection| {
            let job = JobRepo::new(connection)
                .create(&active.meta.id.0, "Import")
                .unwrap();
            JobRepo::new(connection).start(&job).unwrap();
            let config = import_precheck::prepare_import_source_config(
                &evidence.to_string_lossy(),
                DataSourcePlatform::Windows,
                None,
            )
            .unwrap();
            let failure = execute_import_job_profiled(
                connection,
                &active.meta.id,
                &active.case_root,
                config,
                &job,
                ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: Some(&sink),
                    cancel_token: &token,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: false,
                },
                &profiler,
            )
            .expect_err("journal I/O error must fail the import");
            assert_eq!(failure.category, "io");
            assert_eq!(failure.code, "IMPORT_PROFILE_OUTPUT");
            let steps = NotebookRepo::new(connection)
                .list_steps(&active.meta.id.0, &StepFilters::default())
                .unwrap();
            assert_eq!(steps.len(), 1);
            assert_eq!(steps[0].success, Some(false));
            assert_eq!(
                steps[0].error_code.as_deref(),
                Some("IMPORT_PROFILE_OUTPUT")
            );
            Ok(())
        })
        .unwrap();
    assert!(sink.failed.load(Ordering::Acquire));
    assert_eq!(sink.ready.load(Ordering::Acquire), 0);
    assert!(profiler.output_error().is_some());
}
