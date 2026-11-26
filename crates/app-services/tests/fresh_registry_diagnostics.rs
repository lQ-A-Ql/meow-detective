use app_services::{
    analysis_service, case_service,
    import_analysis::ImportAnalysisMode,
    import_job_coordinator::{run_single_import, SingleImportRequest},
    import_precheck, registry_browser_service,
};
use domain::DataSourcePlatform;
use infrastructure::logging::{LogConfig, LogDirection, LogFilter, LogQuery, LogRuntime};
use persistence_sqlite::repositories::{
    analysis_scan_repo::AnalysisScanRepo, datasource_repo::DataSourceRepo, job_repo::JobRepo,
};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
use tracing_subscriber::prelude::*;
use transport::{dto::*, ErrorCategory, ServiceErrorCategory};

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE and FORENSICS_DIAGNOSTIC_OUTPUT"]
fn fresh_windows_import_classifies_registry_damage_and_preserves_diagnostics() {
    let image = PathBuf::from(std::env::var_os("FORENSICS_E01_FIXTURE").unwrap());
    let output = PathBuf::from(std::env::var_os("FORENSICS_DIAGNOSTIC_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let before = segment_metadata(&image);
    let (logs, _guard) = LogRuntime::start(LogConfig::new(output.join("logs"))).unwrap();
    tracing_subscriber::registry().with(logs.layer()).init();
    let active =
        case_service::create_case(&output.join("cases"), "registry-diagnostics", None).unwrap();
    let connection = active.connection().unwrap();
    let started = Instant::now();
    let config = import_precheck::prepare_import_source_config_from_path(
        image.to_str().unwrap(),
        DataSourcePlatform::Windows,
    )
    .unwrap();
    let job = JobRepo::new(&connection)
        .create(&active.meta.id.0, "Registry diagnostic acceptance")
        .unwrap();
    run_single_import(SingleImportRequest {
        connection: &connection,
        case_id: &active.meta.id,
        case_root: &active.case_root,
        job_id: &job,
        import_config: config,
        max_import_workers: Some(2),
        max_analysis_workers: Some(2),
        analysis_mode: ImportAnalysisMode::MetadataOnly,
        cancel_token: &Arc::new(AtomicBool::new(false)),
        event_sink: None,
    })
    .inspect_err(|_| logs.flush().unwrap())
    .unwrap();
    let import_ms = started.elapsed().as_millis();
    let source = DataSourceRepo::new(&connection)
        .find_by_case(&active.meta.id)
        .unwrap()
        .remove(0)
        .id;
    let span = tracing::info_span!(
        "registry_acceptance",
        case_id = %active.meta.id.0,
        job_id = %job.0,
    );
    let _entered = span.enter();
    let extraction_started = Instant::now();
    let execution = analysis_service::run_source_analysis_extraction_execution_with_cancel(
        &connection,
        &active.case_root,
        &active.meta.id,
        &source,
        &["Registry"],
        Arc::new(AtomicBool::new(false)),
    )
    .inspect_err(|_| logs.flush().unwrap())
    .unwrap();
    let extraction_ms = extraction_started.elapsed().as_millis();
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
    let diagnostic = diagnostics
        .iter()
        .find(|record| record.source_object_id == "mft:1:128009");
    let preview = app_services::file_service::PreviewRuntimeRegistry::default();
    let bitlocker = Arc::new(app_services::bitlocker_runtime::BitLockerUnlockRegistry::default());
    let registry = registry_browser_service::RegistryBrowserRegistry::default();
    let context = registry_browser_service::RegistryBrowserContext {
        case_conn: &connection,
        case_root: &active.case_root,
        case_id: &active.meta.id,
        bitlocker: &bitlocker,
        preview: &preview,
    };
    let browser_error = registry_browser_service::open_hive(
        &registry,
        &context,
        RegistryHiveOpenRequestDto {
            file_id: format!("ds:{}:mft:1:128009", source.0),
            view: RegistryHiveViewDto::Recovered,
        },
    )
    .unwrap_err();
    let snapshot = logs
        .read(LogQuery {
            direction: LogDirection::Latest,
            positions: [None; 3],
            filter: LogFilter::default(),
            limit_per_level: 100,
        })
        .unwrap();
    let diagnostic_logs = snapshot.pages[1]
        .records
        .iter()
        .filter(|record| record.field("sourceObjectId", "source_object_id") == Some("mft:1:128009"))
        .map(|record| record.as_ref())
        .collect::<Vec<_>>();
    let report = serde_json::json!({
        "importMs": import_ms,
        "extractionMs": extraction_ms,
        "retryableFailureCount": execution.retryable_failure_count,
        "extraction": execution.dto,
        "storedDiagnostic": diagnostic.map(|record| serde_json::json!({
            "fileId": record.source_object_id,
            "warnings": record.warnings,
            "diagnosticCounts": record.diagnostic_counts,
        })),
        "diagnosticLogs": diagnostic_logs,
        "browserErrorCode": browser_error.code(),
        "browserErrorCategory": browser_error.category(),
    });
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    logs.flush().unwrap();
    assert_eq!(execution.retryable_failure_count, 0);
    assert_eq!(execution.dto.diagnostic_counts.corrupt_inputs, 1);
    assert_eq!(execution.dto.diagnostic_counts.read_failures, 0);
    let diagnostic = diagnostic.unwrap();
    assert_eq!(diagnostic.diagnostic_counts, [0, 1, 0, 0, 0]);
    assert!(diagnostic.warnings.iter().any(|warning| warning
        .contains("attribute-list FILE sequence mismatch for record 146498: expected 3, found 4")));
    assert!(diagnostic_logs
        .iter()
        .any(|record| record.message.contains("LOG1 read failed")));
    assert_eq!(browser_error.code(), Some("deletedHive"));
    assert!(matches!(
        browser_error.category(),
        ErrorCategory::Unsupported
    ));
    assert_eq!(before, segment_metadata(&image));
}

fn segment_metadata(image: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    (1..=512)
        .map(|index| image_e01::segment_path(image, index))
        .take_while(|path| path.is_file())
        .map(|path| {
            let metadata = std::fs::metadata(&path).unwrap();
            (path, metadata.len(), metadata.modified().unwrap())
        })
        .collect()
}
