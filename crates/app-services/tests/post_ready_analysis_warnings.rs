use app_services::{
    active_case::ActiveCase,
    analysis_service, case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{
        execute_import_job_profiled, ImportJobOptions, ImportProfiler, JobOutcomeCounts,
    },
    import_precheck,
};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    job_repo::{JobRepo, JobSummaryRow},
};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

struct Imported {
    active: ActiveCase,
    source: DataSourceId,
    message: String,
    counts: JobOutcomeCounts,
    job: JobSummaryRow,
    profile: PathBuf,
}

fn import(evidence: &Path, cases: &Path, deferred: bool) -> Imported {
    let active = case_service::create_case(cases, "analysis-warnings", None).unwrap();
    let profile = cases.join("profile");
    let profiler = ImportProfiler::with_output_directory(&profile).unwrap();
    let (source, message, counts, job) = active
        .with_conn(|connection| {
            let job_id =
                JobRepo::new(connection).create(&active.meta.id.0, "warning regression")?;
            assert!(JobRepo::new(connection).start(&job_id)?);
            let config = import_precheck::prepare_import_source_config_from_path(
                evidence.to_str().unwrap(),
                DataSourcePlatform::Windows,
            )
            .unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            let (message, counts) = execute_import_job_profiled(
                connection,
                &active.meta.id,
                &active.case_root,
                config,
                &job_id,
                ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(2),
                    max_analysis_workers: Some(2),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: deferred,
                },
                &profiler,
            )
            .unwrap();
            JobRepo::new(connection).complete_committed(&job_id, &message)?;
            let sources = DataSourceRepo::new(connection).find_by_case(&active.meta.id)?;
            assert_eq!(sources.len(), 1);
            let source = sources[0].id.clone();
            let storage = DataSourceRepo::new(connection)
                .find_storage(&source)?
                .unwrap();
            assert_eq!(storage.import_state, "ready");
            let job = JobRepo::new(connection).list_recent(1)?.remove(0);
            Ok((source, message, counts, job))
        })
        .unwrap();
    Imported {
        active,
        source,
        message,
        counts,
        job,
        profile,
    }
}

#[test]
fn automatic_analysis_persists_real_parser_warnings_in_job_outcome() {
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/test-scratch/post-ready-analysis-warnings");
    std::fs::create_dir_all(&scratch).unwrap();
    let directory = tempfile::tempdir_in(scratch).unwrap();
    let evidence = directory.path().join("evidence");
    let config = evidence.join("Windows/System32/config");
    std::fs::create_dir_all(&config).unwrap();
    let invalid_hive = config.join("SYSTEM");
    std::fs::write(&invalid_hive, b"regf").unwrap();
    let original = std::fs::read(&invalid_hive).unwrap();
    let deferred = import(&evidence, &directory.path().join("deferred-cases"), true);
    let (analysis_result, retryable_failures) = deferred
        .active
        .with_conn(|connection| {
            let execution = analysis_service::run_source_analysis_extraction_execution_with_cancel(
                connection,
                &deferred.active.case_root,
                &deferred.active.meta.id,
                &deferred.source,
                &["Registry", "BrowserHistory", "Email", "EventLogs"],
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            Ok((execution.dto, execution.retryable_failure_count))
        })
        .unwrap();
    let analysis_warnings = analysis_result.warnings.len();
    assert!(
        analysis_warnings > 0,
        "malformed SYSTEM must produce a parser warning"
    );
    assert!(
        retryable_failures > 0,
        "invalid registry preload must remain retryable"
    );
    let automatic = import(&evidence, &directory.path().join("automatic-cases"), false);
    assert!(analysis_results(&deferred.profile).is_empty());
    let recorded = analysis_results(&automatic.profile);
    assert_eq!(recorded.len(), 1);
    let recorded = &recorded[0];
    assert_eq!(recorded["caseId"], automatic.active.meta.id.0);
    assert_eq!(recorded["sourceId"], automatic.source.0);
    assert_eq!(recorded["runId"], automatic.job.id.0);
    assert_eq!(recorded["retryableFailureCount"], retryable_failures);
    let recorded_dto: transport::dto::AnalysisExtractionRunDto =
        serde_json::from_value(recorded["result"].clone()).unwrap();
    assert_eq!(recorded_dto.warnings, analysis_result.warnings);
    assert_eq!(recorded_dto.sections.len(), analysis_result.sections.len());
    assert_eq!(recorded_dto.status, analysis_result.status);
    assert_eq!(recorded_dto.scanned_count, analysis_result.scanned_count);
    assert_eq!(
        automatic.counts.warning_count,
        deferred.counts.warning_count + u32::try_from(analysis_warnings).unwrap()
    );
    assert_eq!(
        automatic.counts.failed_count,
        deferred.counts.failed_count + u32::try_from(retryable_failures).unwrap()
    );
    assert_eq!(
        automatic.counts.skipped_count,
        deferred.counts.skipped_count
    );
    assert_eq!(automatic.job.warning_count, automatic.counts.warning_count);
    assert_eq!(automatic.job.failed_count, automatic.counts.failed_count);
    assert!(automatic.job.partial);
    assert_eq!(automatic.job.status, "completed");
    assert!(automatic
        .message
        .contains(&format!("warnings={analysis_warnings}")));
    assert!(automatic
        .message
        .contains(&format!("failed={retryable_failures}")));
    assert_eq!(std::fs::read(invalid_hive).unwrap(), original);
}

fn analysis_results(profile: &Path) -> Vec<serde_json::Value> {
    std::fs::read_dir(profile)
        .unwrap()
        .flat_map(|entry| std::fs::read_dir(entry.unwrap().path()).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-analysis-result.json")
        })
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .collect()
}
