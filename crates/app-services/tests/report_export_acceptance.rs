use app_services::{
    analysis_service, case_service,
    import_analysis::ImportAnalysisMode,
    import_job_coordinator::{run_single_import, SingleImportRequest},
    import_precheck, report, timeline_service,
};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};
use transport::{
    commands::{ExportReportRequest, ReportScopeDto, SaveReportRequest},
    dto::{ReportFormatDto, ReportStatusDto},
};

fn output_root() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("FORENSICS_REPORT_ACCEPTANCE_OUTPUT")
            .expect("set FORENSICS_REPORT_ACCEPTANCE_OUTPUT"),
    );
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn import_source(
    root: &Path,
    evidence: &Path,
) -> (app_services::active_case::ActiveCase, DataSourceId) {
    let active = case_service::create_case(&root.join("cases"), "structured-report", None).unwrap();
    let connection = active.connection().unwrap();
    let job = JobRepo::new(&connection)
        .create(&active.meta.id.0, "Structured report acceptance")
        .unwrap();
    let config = import_precheck::prepare_import_source_config_from_path(
        evidence.to_str().unwrap(),
        DataSourcePlatform::Windows,
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
    let execution = analysis_service::run_source_analysis_extraction_execution_with_cancel(
        &connection,
        &active.case_root,
        &active.meta.id,
        &source,
        &["Registry", "BrowserHistory", "Email"],
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    println!("extracted {} artifacts", execution.dto.artifact_count);
    drop(connection);
    (active, source)
}

fn verify_reports(
    root: &Path,
    active: &app_services::active_case::ActiveCase,
    source: &DataSourceId,
) -> serde_json::Value {
    let connection = active.connection().unwrap();
    let cancel = AtomicBool::new(false);
    let mut exports = Vec::new();
    let mut source_summary = None;
    for format in [
        ReportFormatDto::Html,
        ReportFormatDto::Csv,
        ReportFormatDto::Json,
        ReportFormatDto::Markdown,
    ] {
        let request = ExportReportRequest {
            case_id: active.meta.id.0.clone(),
            format,
            scope: ReportScopeDto::default(),
            data_source_id: None,
        };
        let context = report::ReportExportContext {
            connection: &connection,
            case: &active.meta,
            case_root: &active.case_root,
            request: &request,
            cancel: &cancel,
        };
        let document = report::read_report_document_for_case(&context).unwrap();
        assert!(document.sources[0].summary_available);
        source_summary = Some(document.sources[0].summary.clone());
        let started = std::time::Instant::now();
        let exported = report::export_report_for_case(context).unwrap();
        assert_eq!(exported.status, ReportStatusDto::Completed);
        let path = active.case_root.join("reports").join(&exported.file_name);
        let bytes = std::fs::read(&path).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(!text.contains("report-raw-row.example"));
        assert!(!text.contains("This is a plain-text test message"));
        let save = SaveReportRequest {
            case_id: active.meta.id.0.clone(),
            report_id: exported.id.clone(),
            destination: root
                .join(format!("saved.{}", format.extension()))
                .to_string_lossy()
                .into_owned(),
            overwrite: false,
        };
        report::save_report_for_case(&connection, &active.case_root, &save).unwrap();
        assert_eq!(std::fs::read(&save.destination).unwrap(), bytes);
        exports.push(serde_json::json!({"format": format, "bytes": bytes.len(), "elapsedMs": started.elapsed().as_millis(), "report": exported}));
    }
    let request = ExportReportRequest {
        case_id: active.meta.id.0.clone(),
        format: ReportFormatDto::Json,
        scope: ReportScopeDto::default(),
        data_source_id: Some(source.0.clone()),
    };
    let document = report::read_report_document_for_case(&report::ReportExportContext {
        connection: &connection,
        case: &active.meta,
        case_root: &active.case_root,
        request: &request,
        cancel: &cancel,
    })
    .unwrap();
    assert_eq!(document.sources.len(), 1);
    assert_eq!(Some(&document.sources[0].summary), source_summary.as_ref());
    let summary = &document.sources[0].summary;
    let indexed = app_services::artifact_service::get_artifact_family_counts_for_case(
        &connection,
        &active.case_root,
        &active.meta.id,
    )
    .unwrap();
    for (section, families) in [
        (
            &summary.hardware,
            &["RegistryHardwareDevice", "RegistryNetworkAdapter"][..],
        ),
        (
            &summary.browsers,
            &[
                "BrowserHistory",
                "BrowserDownload",
                "BrowserCookie",
                "BrowserSessionTab",
                "BrowserPassword",
                "BrowserExtension",
            ][..],
        ),
        (&summary.email, &["EmailMessage", "EmailAccount"][..]),
    ] {
        for family in families {
            let expected = indexed
                .iter()
                .find(|item| item.family == *family)
                .map(|item| item.count)
                .unwrap_or(0);
            assert_eq!(
                section.counts.get(*family).copied().unwrap_or(0),
                expected,
                "report count for {family}"
            );
        }
    }
    let request = ExportReportRequest {
        scope: ReportScopeDto {
            operating_system: false,
            hardware: false,
            browsers: false,
            email: false,
        },
        ..request
    };
    let exported = report::export_report_for_case(report::ReportExportContext {
        connection: &connection,
        case: &active.meta,
        case_root: &active.case_root,
        request: &request,
        cancel: &cancel,
    })
    .unwrap();
    let json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(active.case_root.join("reports").join(exported.file_name)).unwrap(),
    )
    .unwrap();
    assert_eq!(
        json["sources"][0]["summary"]["operatingSystem"]["recordCount"],
        0
    );
    assert_eq!(json["sources"][0]["summary"]["email"]["recordCount"], 0);
    assert!(json.get("timeline_events").is_none());
    assert!(json.get("artifacts").is_none());
    serde_json::json!({"sourceId":source.0,"summary":source_summary,"exports":exports,"historyCount":report::get_report_history(&connection,&active.meta.id.0).unwrap().len()})
}

fn create_logical_evidence(root: &Path) -> PathBuf {
    let evidence = root.join("evidence");
    let files = evidence.join("timeline");
    std::fs::create_dir_all(&files).unwrap();
    for index in 0..34_000 {
        std::fs::write(
            files.join(format!("file-{index:05}.txt")),
            b"report acceptance\n",
        )
        .unwrap();
    }
    for index in 0..201 {
        let directory = evidence.join(format!("snapshot-{index:03}/Windows/System32/config"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SOFTWARE"),
            testing::builders::registry::synthetic_software_hive(),
        )
        .unwrap();
    }
    let browser = evidence.join("Users/Alice/AppData/Local/Google/Chrome/User Data/Default");
    std::fs::create_dir_all(&browser).unwrap();
    let mut database = rusqlite::Connection::open(browser.join("History")).unwrap();
    database.execute_batch("CREATE TABLE urls(id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, last_visit_time INTEGER); CREATE TABLE visits(id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);").unwrap();
    let transaction = database.transaction().unwrap();
    for index in 1..=1003 {
        transaction
            .execute(
                "INSERT INTO urls VALUES (?1, ?2, 'report raw visit', 1, ?3)",
                rusqlite::params![
                    index,
                    format!("https://report-raw-row.example/{index}"),
                    13_348_540_800_000_000i64 + index
                ],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO visits VALUES (?1, ?1, ?2)",
                rusqlite::params![index, 13_348_540_800_000_000i64 + index],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
    drop(database);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/email/multipart.eml");
    std::fs::copy(fixture, evidence.join("message.eml")).unwrap();
    evidence
}

#[test]
#[ignore = "requires FORENSICS_REPORT_ACCEPTANCE_OUTPUT opt-in fresh import"]
fn fresh_large_logical_reports_use_structured_summaries() {
    let root = output_root();
    let evidence = create_logical_evidence(&root);
    let (active, source) = import_source(&root, &evidence);
    let connection = active.connection().unwrap();
    timeline_service::rebuild_timeline_projection(
        &connection,
        &active.case_root,
        &active.meta.id,
        Some(&source.0),
        Default::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let timeline = timeline_service::query_timeline_for_case(
        &connection,
        &active.case_root,
        &active.meta.id,
        0,
        1,
    )
    .unwrap();
    assert!(timeline.total > 100_000);
    drop(connection);
    let results = verify_reports(&root, &active, &source);
    assert_eq!(results["summary"]["operatingSystem"]["recordCount"], 1206);
    assert_eq!(
        results["summary"]["browsers"]["counts"]["BrowserHistory"],
        1003
    );
    assert_eq!(results["summary"]["email"]["counts"]["EmailMessage"], 1);
    std::fs::write(
        root.join("acceptance.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"timelineCount":timeline.total,"results":results}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample and FORENSICS_REPORT_ACCEPTANCE_OUTPUT"]
fn fresh_e01_reports_use_structured_summaries() {
    let evidence = PathBuf::from(
        std::env::var_os("FORENSICS_E01_FIXTURE").expect("set FORENSICS_E01_FIXTURE"),
    );
    let root = output_root();
    let (active, source) = import_source(&root, &evidence);
    let results = verify_reports(&root, &active, &source);
    assert!(
        results["summary"]["operatingSystem"]["recordCount"]
            .as_u64()
            .unwrap()
            > 0
    );
    std::fs::write(
        root.join("acceptance.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}
