use app_services::{
    analysis_service, case_service,
    import_analysis::ImportAnalysisMode,
    import_job_coordinator::{run_single_import, SingleImportRequest},
    import_precheck,
    mcp_host_service::McpHostServiceError,
    registry_browser_service::RegistryBrowserError,
};
use infrastructure::logging::{LogConfig, LogDirection, LogFilter, LogQuery, LogRuntime};
use persistence_sqlite::repositories::{
    analysis_scan_repo::AnalysisScanRepo, datasource_repo::DataSourceRepo, job_repo::JobRepo,
};
use std::sync::{atomic::AtomicBool, Arc};
use tracing_subscriber::prelude::*;
use transport::{ErrorCategory, ServiceErrorCategory};

#[test]
fn registry_pipeline_persists_failure_logs_and_preserves_browser_errors() {
    let root = tempfile::tempdir().unwrap();
    let evidence = root.path().join("evidence");
    let software = "Windows/System32/config/SOFTWARE";
    std::fs::create_dir_all(evidence.join("Windows/System32/config")).unwrap();
    std::fs::write(
        evidence.join(software),
        testing::builders::registry::synthetic_software_hive(),
    )
    .unwrap();
    for slot in ["LOG1", "LOG2"] {
        std::fs::write(evidence.join(format!("{software}.{slot}")), []).unwrap();
    }
    let (logs, _guard) = LogRuntime::start(LogConfig::new(root.path().join("logs"))).unwrap();
    tracing_subscriber::registry().with(logs.layer()).init();
    let active =
        case_service::create_case(&root.path().join("cases"), "registry-logs", None).unwrap();
    let connection = active.connection().unwrap();
    let job = JobRepo::new(&connection)
        .create(&active.meta.id.0, "Registry logs")
        .unwrap();
    let config = import_precheck::prepare_import_source_config_from_path(
        evidence.to_str().unwrap(),
        domain::DataSourcePlatform::Windows,
    )
    .unwrap();
    run_single_import(SingleImportRequest {
        connection: &connection,
        case_id: &active.meta.id,
        case_root: &active.case_root,
        job_id: &job,
        import_config: config,
        max_import_workers: Some(1),
        max_analysis_workers: Some(1),
        analysis_mode: ImportAnalysisMode::MetadataOnly,
        cancel_token: &Arc::new(AtomicBool::new(false)),
        event_sink: None,
    })
    .unwrap();
    let source = DataSourceRepo::new(&connection)
        .find_by_case(&active.meta.id)
        .unwrap()
        .remove(0)
        .id;
    std::fs::remove_file(evidence.join(format!("{software}.LOG1"))).unwrap();
    let execution = analysis_service::run_source_analysis_extraction_execution_with_cancel(
        &connection,
        &active.case_root,
        &active.meta.id,
        &source,
        &["Registry"],
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(execution.retryable_failure_count, 1);
    let source_connection = app_services::connection::open_existing_case_read_only(
        &active
            .case_root
            .join("sources")
            .join(&source.0)
            .join("source.db"),
    )
    .unwrap();
    let diagnostics = AnalysisScanRepo::new(&source_connection)
        .list_diagnostics_for_version("2.1.0")
        .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].diagnostic_counts, [0, 0, 1, 0, 0]);
    let snapshot = logs
        .read(LogQuery {
            direction: LogDirection::Latest,
            positions: [None; 3],
            filter: LogFilter {
                data_source_id: source.0.clone(),
                ..Default::default()
            },
            limit_per_level: 100,
        })
        .unwrap();
    let errors = &snapshot.pages[2].records;
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("LOG1 read failed"));
    assert_eq!(
        errors[0].fields.get("source_object_id"),
        Some(&diagnostics[0].source_object_id)
    );
    assert!(errors[0]
        .fields
        .get("evidence_path")
        .unwrap()
        .ends_with(software));
    logs.flush().unwrap();
    let log_text = std::fs::read_to_string(
        root.path()
            .join("logs")
            .join(logs.session_id())
            .join("runtime_rCURRENT.jsonl"),
    )
    .unwrap();
    assert!(log_text.contains("LOG1 read failed"));
    let error = McpHostServiceError::Registry(RegistryBrowserError::DeletedHive);
    assert!(matches!(error.category(), ErrorCategory::Unsupported));
    assert_eq!(error.code(), Some("deletedHive"));
    assert_eq!(
        error.user_message(),
        Some("The selected registry hive is deleted.")
    );
    assert_eq!(error.recoverable(), Some(false));
    let response = transport::CommandError::from_typed_service_error(error);
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["code"], "deletedHive");
    assert_eq!(response["category"], "unsupported");
    assert_eq!(
        response["message"],
        "The selected registry hive is deleted."
    );
    assert_eq!(response["recoverable"], false);
}
