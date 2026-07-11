use std::cell::Cell;
use std::io::{Cursor, Read};
use std::path::PathBuf;

use app_services::analysis_service::{
    get_evidence_classification_summary, resolve_data_source_platform, run_analysis_extraction,
    select_evidence_scan_categories, validate_analysis_categories,
    validate_data_source_analysis_categories, AnalysisServiceError,
};
use domain::{
    CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourcePlatform,
    DataSourceProvenance, EntryType, FileEntry, FileEntryId,
};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};
use transport::{dto::AnalysisParseStatusDto, ErrorCategory, ServiceErrorCategory};

fn source_connection() -> rusqlite::Connection {
    let conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&conn).expect("run source migrations");
    conn
}

fn empty_reader(_: &domain::FileEntryId) -> Result<Box<dyn Read>, std::io::Error> {
    Ok(Box::new(Cursor::new(Vec::<u8>::new())))
}

fn section_keys(run: &transport::dto::AnalysisExtractionRunDto) -> Vec<&str> {
    run.sections
        .iter()
        .map(|section| section.key.as_str())
        .collect()
}

fn insert_source_file(conn: &rusqlite::Connection, id: &str, path: &str) {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    FileRepo::new(conn)
        .insert_batch(&[FileEntry {
            id: FileEntryId(id.to_string()),
            parent_id: None,
            data_source_id: DataSourceId("source-test".to_string()),
            path: path.to_string(),
            name: name.to_string(),
            entry_type: EntryType::File,
            size: Some(16),
            ext: path.rsplit_once('.').map(|(_, ext)| ext.to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
        .expect("insert source file");
}

#[test]
fn empty_category_selection_is_scoped_to_windows() {
    let conn = source_connection();
    let run = run_analysis_extraction(
        &conn,
        "case-windows",
        DataSourcePlatform::Windows,
        &[],
        empty_reader,
    )
    .expect("run Windows analysis");

    assert_eq!(
        section_keys(&run),
        vec!["BrowserHistory", "Email", "EventLogs", "Registry"]
    );
    assert!(run
        .sections
        .iter()
        .all(|section| !section.key.starts_with("Linux")));
}

#[test]
fn empty_category_selection_is_scoped_to_linux() {
    let conn = source_connection();
    let run = run_analysis_extraction(
        &conn,
        "case-linux",
        DataSourcePlatform::Linux,
        &[],
        empty_reader,
    )
    .expect("run Linux analysis");

    assert_eq!(run.sections.len(), 9);
    assert!(run
        .sections
        .iter()
        .all(|section| section.key.starts_with("Linux")));
    assert!(!section_keys(&run).contains(&"Registry"));
}

#[test]
fn cross_platform_capability_is_rejected_before_database_or_evidence_access() {
    let conn = rusqlite::Connection::open_in_memory().expect("open bare database");
    let reader_called = Cell::new(false);
    let validation = validate_analysis_categories(DataSourcePlatform::Windows, &["LinuxJournal"])
        .expect_err("service validation must reject cross-platform capability");
    assert!(matches!(validation, AnalysisServiceError::Unsupported(_)));
    let linux_on_windows = run_analysis_extraction(
        &conn,
        "case-windows",
        DataSourcePlatform::Windows,
        &["LinuxJournal"],
        |_| {
            reader_called.set(true);
            empty_reader(&domain::FileEntryId("unused".to_string()))
        },
    )
    .expect_err("cross-platform capability must fail closed");

    assert!(matches!(
        &linux_on_windows,
        AnalysisServiceError::Unsupported(_)
    ));
    assert!(matches!(
        linux_on_windows.category(),
        ErrorCategory::Unsupported
    ));
    let windows_on_linux = run_analysis_extraction(
        &conn,
        "case-linux",
        DataSourcePlatform::Linux,
        &["Registry"],
        |_| {
            reader_called.set(true);
            empty_reader(&domain::FileEntryId("unused".to_string()))
        },
    )
    .expect_err("Windows capability must not run on Linux");
    assert!(matches!(
        &windows_on_linux,
        AnalysisServiceError::Unsupported(_)
    ));
    assert!(matches!(
        windows_on_linux.category(),
        ErrorCategory::Unsupported
    ));
    assert!(!reader_called.get());
}

#[test]
fn retired_capability_remains_typed_unsupported() {
    let conn = rusqlite::Connection::open_in_memory().expect("open bare database");
    let error = run_analysis_extraction(
        &conn,
        "case-windows",
        DataSourcePlatform::Windows,
        &[" MacArtifacts "],
        empty_reader,
    )
    .expect_err("retired capability must fail closed");

    assert!(matches!(
        &error,
        AnalysisServiceError::Unsupported(capability) if capability == "MacArtifacts"
    ));
    assert!(matches!(error.category(), ErrorCategory::Unsupported));
}

#[test]
fn targeted_evidence_scan_rejects_linux_with_typed_unsupported() {
    let error = select_evidence_scan_categories(DataSourcePlatform::Linux, &[])
        .expect_err("Linux structured extraction must use run_analysis_extraction");
    assert!(matches!(error, AnalysisServiceError::Unsupported(_)));
    assert!(matches!(error.category(), ErrorCategory::Unsupported));

    let windows = select_evidence_scan_categories(DataSourcePlatform::Windows, &[])
        .expect("select Windows defaults");
    assert!(!windows.contains(&"EventLogs"));
    assert!(!windows.contains(&"LinuxArtifacts"));
}

#[test]
fn evidence_summary_categories_are_scoped_to_platform() {
    let conn = source_connection();
    let windows = get_evidence_classification_summary(&conn, DataSourcePlatform::Windows)
        .expect("build Windows evidence summary");
    let windows_keys = windows
        .categories
        .iter()
        .map(|category| category.category.as_str())
        .collect::<Vec<_>>();
    assert!(windows_keys.contains(&"EventLogs"));
    assert!(windows_keys.contains(&"FileTypeInventory"));
    assert!(!windows_keys.contains(&"LinuxArtifacts"));

    let linux = get_evidence_classification_summary(&conn, DataSourcePlatform::Linux)
        .expect("build Linux evidence summary");
    let linux_keys = linux
        .categories
        .iter()
        .map(|category| category.category.as_str())
        .collect::<Vec<_>>();
    assert_eq!(linux_keys, vec!["FileTypeInventory", "LinuxArtifacts"]);
}

#[test]
fn extraction_status_distinguishes_not_found_failed_and_partial() {
    let empty = source_connection();
    let not_found = run_analysis_extraction(
        &empty,
        "case-empty",
        DataSourcePlatform::Windows,
        &["Email"],
        empty_reader,
    )
    .expect("run empty extraction");
    assert_eq!(not_found.status, AnalysisParseStatusDto::NotFound);

    let failed = source_connection();
    insert_source_file(&failed, "mail-failed", "mailbox/message.eml");
    let failed_run = run_analysis_extraction(
        &failed,
        "case-failed",
        DataSourcePlatform::Windows,
        &["Email"],
        |_| Err::<Box<dyn Read>, _>(std::io::Error::other("fixture read failure")),
    )
    .expect("return failed extraction status");
    assert_eq!(failed_run.status, AnalysisParseStatusDto::Failed);
    assert_eq!(
        failed_run.sections[0].status,
        AnalysisParseStatusDto::Failed
    );

    let partial = source_connection();
    insert_source_file(&partial, "mail-partial", "mailbox/archive.pst");
    let partial_run = run_analysis_extraction(
        &partial,
        "case-partial",
        DataSourcePlatform::Windows,
        &["Email"],
        empty_reader,
    )
    .expect("return partial extraction status");
    assert_eq!(partial_run.scanned_count, 1);
    assert_eq!(partial_run.status, AnalysisParseStatusDto::Partial);
    assert_eq!(
        partial_run.sections[0].status,
        AnalysisParseStatusDto::Partial
    );
}

#[test]
fn registry_preload_warnings_are_attributed_once_to_registry_section() {
    let conn = source_connection();
    insert_source_file(&conn, "registry-system", "Windows/System32/config/SYSTEM");
    let run = run_analysis_extraction(
        &conn,
        "case-registry-failed",
        DataSourcePlatform::Windows,
        &["Registry"],
        |_| Err::<Box<dyn Read>, _>(std::io::Error::other("preload failure")),
    )
    .expect("return failed Registry extraction status");

    let preload_warning = "Windows/System32/config/SYSTEM read failed: preload failure";
    assert_eq!(run.status, AnalysisParseStatusDto::Failed);
    assert_eq!(run.sections.len(), 1);
    assert_eq!(run.sections[0].status, AnalysisParseStatusDto::Failed);
    assert_eq!(
        run.warnings
            .iter()
            .filter(|warning| warning.as_str() == preload_warning)
            .count(),
        1
    );
    assert_eq!(
        run.sections[0]
            .warnings
            .iter()
            .filter(|warning| warning.as_str() == preload_warning)
            .count(),
        1
    );
}

#[test]
fn persisted_platform_resolution_uses_domain_platform_type() {
    let conn = persistence_sqlite::open_in_memory().expect("open case database");
    persistence_sqlite::runner::run_all(&conn).expect("run case migrations");
    let case_id = CaseId("case-platform-resolution".to_string());
    CaseRepo::new(&conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "Platform resolution".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("create case");
    let source = DataSource {
        id: DataSourceId("source-linux".to_string()),
        name: "Linux source".to_string(),
        kind: DataSourceKind::E01,
        source_path: PathBuf::from("fixture.E01"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source.id.0, Some(" LiNuX "), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(&conn)
        .insert_with_storage(&case_id, &source, &storage)
        .expect("register source");

    assert_eq!(
        resolve_data_source_platform(&conn, &case_id, &source.id).expect("resolve platform"),
        DataSourcePlatform::Linux
    );
    assert_eq!(
        validate_data_source_analysis_categories(&conn, &case_id, &source.id, &["LinuxJournal"],)
            .expect("validate persisted Linux capability"),
        DataSourcePlatform::Linux
    );
    assert!(matches!(
        validate_data_source_analysis_categories(&conn, &case_id, &source.id, &["Registry"]),
        Err(AnalysisServiceError::Unsupported(_))
    ));
}

#[test]
fn persisted_platform_resolution_rejects_non_ready_sources() {
    let conn = persistence_sqlite::open_in_memory().expect("open case database");
    persistence_sqlite::runner::run_all(&conn).expect("run case migrations");
    let case_id = CaseId("case-pending-platform".to_string());
    CaseRepo::new(&conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "Pending platform".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("create case");
    let source = DataSource {
        id: DataSourceId("source-pending".to_string()),
        name: "Pending source".to_string(),
        kind: DataSourceKind::E01,
        source_path: PathBuf::from("pending.E01"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    DataSourceRepo::new(&conn)
        .insert_with_storage(
            &case_id,
            &source,
            &DataSourceStorage::source_db(&source.id.0, Some("linux"), None),
        )
        .expect("register pending source");

    let error = resolve_data_source_platform(&conn, &case_id, &source.id)
        .expect_err("pending source must not be analyzed");
    assert!(matches!(error, AnalysisServiceError::InvalidInput(_)));
    assert!(matches!(error.category(), ErrorCategory::Validation));
}
