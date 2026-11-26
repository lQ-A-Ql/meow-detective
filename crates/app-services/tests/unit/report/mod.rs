use super::*;
use crate::{
    analysis_service, case_service,
    import_analysis::ImportAnalysisMode,
    import_job_coordinator::{run_single_import, SingleImportRequest},
    import_precheck,
};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::sync::Arc;
use transport::{
    commands::{ReportScopeDto, SaveReportRequest},
    dto::{ReportFormatDto, ReportStatusDto},
};

fn imported_case() -> (tempfile::TempDir, crate::active_case::ActiveCase) {
    let root = tempfile::tempdir().unwrap();
    let evidence = root.path().join("evidence/Windows/System32/config");
    std::fs::create_dir_all(&evidence).unwrap();
    std::fs::write(
        evidence.join("SOFTWARE"),
        testing::builders::registry::synthetic_software_hive(),
    )
    .unwrap();
    let active =
        case_service::create_case(&root.path().join("cases"), "report-acceptance", None).unwrap();
    {
        let connection = active.connection().unwrap();
        let job = JobRepo::new(&connection)
            .create(&active.meta.id.0, "report fixture import")
            .unwrap();
        let config = import_precheck::prepare_import_source_config_from_path(
            root.path().join("evidence").to_str().unwrap(),
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
        analysis_service::run_source_analysis_extraction_execution_with_cancel(
            &connection,
            &active.case_root,
            &active.meta.id,
            &source,
            &["Registry"],
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    }
    (root, active)
}

fn request(case: &domain::CaseMeta, format: ReportFormatDto) -> ExportReportRequest {
    ExportReportRequest {
        case_id: case.id.0.clone(),
        format,
        scope: ReportScopeDto::default(),
        data_source_id: None,
    }
}

#[test]
fn fresh_import_exports_four_formats_and_saves_verified_report() {
    let (root, active) = imported_case();
    let connection = active.connection().unwrap();
    let cancel = AtomicBool::new(false);
    for format in [
        ReportFormatDto::Html,
        ReportFormatDto::Csv,
        ReportFormatDto::Json,
        ReportFormatDto::Markdown,
    ] {
        let request = request(&active.meta, format);
        let context = ReportExportContext {
            connection: &connection,
            case: &active.meta,
            case_root: &active.case_root,
            request: &request,
            cancel: &cancel,
        };
        let document = read_report_document_for_case(&context).unwrap();
        assert!(document.sources[0].summary_available);
        assert_eq!(document.sources[0].summary.operating_system.record_count, 6);
        let exported = export_report_for_case(context).unwrap();
        assert_eq!(exported.status, ReportStatusDto::Completed);
        let content =
            std::fs::read(active.case_root.join("reports").join(&exported.file_name)).unwrap();
        assert_eq!(exported.file_size, Some(content.len() as u64));
        let save = SaveReportRequest {
            case_id: active.meta.id.0.clone(),
            report_id: exported.id,
            destination: root
                .path()
                .join(format!("saved.{}", format.extension()))
                .to_string_lossy()
                .into_owned(),
            overwrite: false,
        };
        save_report_for_case(&connection, &active.case_root, &save).unwrap();
        assert_eq!(std::fs::read(&save.destination).unwrap(), content);
        assert!(save_report_for_case(&connection, &active.case_root, &save).is_err());
        if format == ReportFormatDto::Json {
            let json: serde_json::Value = serde_json::from_slice(&content).unwrap();
            assert!(json.get("timeline_events").is_none());
            assert!(json.get("artifacts").is_none());
        }
    }
    assert_eq!(
        get_report_history(&connection, &active.meta.id.0)
            .unwrap()
            .len(),
        4
    );
    assert!(get_report_history(&connection, "another-case")
        .unwrap()
        .is_empty());
}

#[test]
fn output_failure_and_cancellation_are_persisted_as_terminal_states() {
    let (_root, active) = imported_case();
    let connection = active.connection().unwrap();
    std::fs::remove_dir(active.case_root.join("reports")).unwrap();
    std::fs::write(active.case_root.join("reports"), b"occupied").unwrap();
    let request = request(&active.meta, ReportFormatDto::Json);
    assert!(export_report_for_case(ReportExportContext {
        connection: &connection,
        case: &active.meta,
        case_root: &active.case_root,
        request: &request,
        cancel: &AtomicBool::new(false)
    })
    .is_err());
    assert!(matches!(
        export_report_for_case(ReportExportContext {
            connection: &connection,
            case: &active.meta,
            case_root: &active.case_root,
            request: &request,
            cancel: &AtomicBool::new(true)
        }),
        Err(ReportError::Cancelled)
    ));
    let history = get_report_history(&connection, &active.meta.id.0).unwrap();
    assert!(history
        .iter()
        .any(|item| item.status == ReportStatusDto::Failed && item.error_code.is_some()));
    assert!(history
        .iter()
        .any(|item| item.status == ReportStatusDto::Cancelled));
    assert!(history.iter().all(|item| item.progress != Some(100)));
}
