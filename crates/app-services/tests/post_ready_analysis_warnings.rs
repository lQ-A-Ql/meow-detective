use app_services::{
    active_case, analysis_service, bitlocker_runtime, capability_service, case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_with_counts, ImportJobOptions},
    import_precheck, post_import_service, task_runtime,
};
use domain::DataSourcePlatform;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, job_repo::JobRepo,
    source_capability_repo::SourceCapabilityRepo,
};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::time::Duration;

#[path = "support/managed_capabilities.rs"]
mod managed_capabilities;

#[derive(Default)]
struct ProgressEvents(Mutex<Vec<transport::events::EventEnvelope<serde_json::Value>>>);

impl post_import_service::ApplicationEventSink for ProgressEvents {
    fn emit(
        &self,
        event: &transport::events::EventEnvelope<serde_json::Value>,
    ) -> std::io::Result<()> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }
}

#[test]
fn managed_analysis_preserves_corrupt_input_warnings_and_reuses_diagnostic_checkpoints() {
    let directory = tempfile::tempdir().unwrap();
    let evidence = directory.path().join("evidence");
    let config = evidence.join("Windows/System32/config");
    std::fs::create_dir_all(&config).unwrap();
    let invalid_hive = config.join("SYSTEM");
    std::fs::write(&invalid_hive, b"regf").unwrap();
    let original = std::fs::read(&invalid_hive).unwrap();
    let active = case_service::create_case(
        &directory.path().join("cases"),
        "managed-analysis-warnings",
        None,
    )
    .unwrap();
    let source = active
        .with_conn(|connection| {
            let job = JobRepo::new(connection).create(&active.meta.id.0, "warning regression")?;
            JobRepo::new(connection).start(&job)?;
            let config = import_precheck::prepare_import_source_config_from_path(
                evidence.to_str().unwrap(),
                DataSourcePlatform::Windows,
            )
            .unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            let (message, counts) = execute_import_job_with_counts(
                connection,
                &active.meta.id,
                &active.case_root,
                config,
                &job,
                ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: false,
                },
            )
            .unwrap();
            assert_eq!(counts.warning_count, 0);
            assert_eq!(counts.failed_count, 0);
            JobRepo::new(connection).complete_committed(&job, &message)?;
            let source = DataSourceRepo::new(connection)
                .find_by_case(&active.meta.id)?
                .remove(0)
                .id;
            let row = SourceCapabilityRepo::new(connection)
                .get(&active.meta.id.0, &source.0, "artifacts")?
                .unwrap();
            assert_eq!(row.state, "queued");
            Ok(source)
        })
        .unwrap();

    let events = Arc::new(ProgressEvents::default());
    managed_capabilities::run(&active, Some(events.clone()), Duration::from_secs(30));
    let progress = events.0.lock().unwrap();
    assert!(progress.iter().any(|event| {
        event.topic == transport::events::EventTopic::AnalysisExtractionProgress
            && event.payload["dataSourceId"] == source.0
            && event.payload["warningCandidates"].as_u64().unwrap_or(0) > 0
    }));
    drop(progress);

    active
        .with_conn(|connection| {
            let execution = analysis_service::run_source_analysis_extraction_execution_with_cancel(
                connection,
                &active.case_root,
                &active.meta.id,
                &source,
                &["Registry"],
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            assert_eq!(execution.retryable_failure_count, 0);
            assert!(execution.dto.diagnostic_counts.corrupt_inputs > 0);
            assert!(!execution.dto.warnings.is_empty());
            assert_eq!(execution.dto.checkpoint_hit_count, 1);
            let row = SourceCapabilityRepo::new(connection)
                .get(&active.meta.id.0, &source.0, "artifacts")?
                .unwrap();
            assert_eq!(row.state, "ready");
            assert!(row.error_code.is_none());
            Ok(())
        })
        .unwrap();
    assert_eq!(std::fs::read(invalid_hive).unwrap(), original);
}
