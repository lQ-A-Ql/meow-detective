use std::cell::Cell;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use app_services::analysis_service::{
    get_evidence_classification_summary, resolve_data_source_platform, run_analysis_extraction,
    run_analysis_extraction_with_cancel, select_evidence_scan_categories,
    validate_analysis_categories, validate_data_source_analysis_categories, AnalysisServiceError,
    MAX_REGISTRY_ANALYSIS_BYTES,
};
use domain::{
    Artifact, ArtifactId, CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind,
    DataSourcePlatform, DataSourceProvenance, FileEntryId, TimelineEvent, TimelineEventId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    timeline_repo::TimelineRepo,
};
use transport::{dto::AnalysisParseStatusDto, ErrorCategory, ServiceErrorCategory};

#[path = "support/native_analysis_fixture.rs"]
mod native_analysis_fixture;

use native_analysis_fixture::{source_file, NativeAnalysisFixture};

const SOURCE_ID: &str = "source-test";

fn source_connection() -> NativeAnalysisFixture {
    NativeAnalysisFixture::new(SOURCE_ID, "case-analysis-platform")
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

fn insert_source_file(conn: &NativeAnalysisFixture, id: &str, path: &str) {
    conn.insert_file(source_file(SOURCE_ID, id, path, 16, false));
}

fn analysis_artifact(id: &str, producer: &str, version: &str) -> Artifact {
    Artifact {
        id: ArtifactId(id.to_string()),
        family: "LinuxWebFinding".to_string(),
        title: id.to_string(),
        summary: String::new(),
        source_object_id: Some(FileEntryId("web-stale".to_string())),
        extractor_id: Some(producer.to_string()),
        extractor_version: Some(version.to_string()),
        confidence: Some(0.9),
        source_attribution: Some("native fixture".to_string()),
        created_at: chrono::Utc::now(),
        attrs: std::collections::BTreeMap::new(),
    }
}

fn unrelated_timeline_event(id: &str, parser: &str) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(id.to_string()),
        source_object_id: "web-stale".to_string(),
        event_type: "linux.web".to_string(),
        timestamp: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("parse event timestamp")
            .with_timezone(&chrono::Utc),
        title: id.to_string(),
        description: String::new(),
        parser_id: Some(parser.to_string()),
        parser_version: Some("1.0.0".to_string()),
        confidence: Some(0.9),
        source_attribution: Some("native fixture".to_string()),
        attrs: std::collections::BTreeMap::new(),
    }
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
fn clean_linux_web_candidate_is_materialized_and_skipped_on_retry() {
    let conn = source_connection();
    insert_source_file(&conn, "web-clean", "var/www/html/index.php");
    let reader_calls = Cell::new(0usize);

    let first = run_analysis_extraction(
        &conn,
        "case-linux-web",
        DataSourcePlatform::Linux,
        &["LinuxWebServices"],
        |_| {
            reader_calls.set(reader_calls.get() + 1);
            Ok::<Box<dyn Read>, std::io::Error>(Box::new(Cursor::new(
                b"<?php echo 'hello';".to_vec(),
            )))
        },
    )
    .expect("scan clean web candidate");
    let second = run_analysis_extraction(
        &conn,
        "case-linux-web",
        DataSourcePlatform::Linux,
        &["LinuxWebServices"],
        |_| {
            reader_calls.set(reader_calls.get() + 1);
            Ok::<Box<dyn Read>, std::io::Error>(Box::new(Cursor::new(
                b"<?php echo 'hello';".to_vec(),
            )))
        },
    )
    .expect("reuse clean web scan");

    assert_eq!(first.scanned_count, 1);
    assert_eq!(second.scanned_count, 1);
    assert_eq!(first.checkpoint_hit_count, 0);
    assert_eq!(second.checkpoint_hit_count, 1);
    assert_eq!(reader_calls.get(), 1);
    let materialized_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM source_meta
             WHERE key LIKE 'analysis_candidate_scan:clean:%'",
            [],
            |row| row.get(0),
        )
        .expect("count materialized clean scan");
    assert_eq!(materialized_count, 1);
}

#[test]
fn stale_analysis_outputs_are_replaced_without_touching_other_producers() {
    let conn = source_connection();
    insert_source_file(&conn, "web-stale", "var/www/html/index.php");
    ArtifactRepo::new(&conn)
        .insert_batch(
            &[
                analysis_artifact("old-analysis", "linux.web.old", "0.9.0"),
                analysis_artifact("third-party", "vendor.web", "7.0.0"),
            ],
            "case-linux-web",
            SOURCE_ID,
        )
        .expect("insert stale and third-party artifacts");
    TimelineRepo::new(&conn)
        .insert_batch_with_case(
            &[
                unrelated_timeline_event("old-analysis-event", "linux.web.old"),
                unrelated_timeline_event("macb-event", "timeline.macb"),
            ],
            "case-linux-web",
        )
        .expect("insert stale and unrelated timeline events");
    let reader_calls = Cell::new(0usize);

    run_analysis_extraction(
        &conn,
        "case-linux-web",
        DataSourcePlatform::Linux,
        &["LinuxWebServices"],
        |_| {
            reader_calls.set(reader_calls.get() + 1);
            Ok::<Box<dyn Read>, std::io::Error>(Box::new(Cursor::new(
                b"<?php echo 'hello';".to_vec(),
            )))
        },
    )
    .expect("replace stale analysis outputs");
    run_analysis_extraction(
        &conn,
        "case-linux-web",
        DataSourcePlatform::Linux,
        &["LinuxWebServices"],
        |_| {
            reader_calls.set(reader_calls.get() + 1);
            Ok::<Box<dyn Read>, std::io::Error>(Box::new(Cursor::new(
                b"<?php echo 'hello';".to_vec(),
            )))
        },
    )
    .expect("reuse current clean checkpoint");

    assert_eq!(reader_calls.get(), 1);
    let artifacts = ArtifactRepo::new(&conn);
    assert!(artifacts
        .find_by_id("old-analysis")
        .expect("look up stale artifact")
        .is_none());
    assert!(artifacts
        .find_by_id("third-party")
        .expect("look up third-party artifact")
        .is_some());
    let timeline = TimelineRepo::new(&conn);
    assert!(timeline
        .find_by_id("old-analysis-event")
        .expect("look up stale timeline event")
        .is_none());
    assert!(timeline
        .find_by_id("macb-event")
        .expect("look up MACB event")
        .is_some());
}

#[test]
fn linux_candidates_are_read_in_normalized_path_order() {
    let conn = source_connection();
    insert_source_file(&conn, "web-z", "var/www/z.php");
    insert_source_file(&conn, "web-a", "var/www/a.php");
    insert_source_file(&conn, "web-m", "var/www/m.php");
    let read_order = std::cell::RefCell::new(Vec::new());

    run_analysis_extraction(
        &conn,
        "case-linux-order",
        DataSourcePlatform::Linux,
        &["LinuxWebServices"],
        |file_id| {
            read_order.borrow_mut().push(file_id.0.clone());
            Ok::<Box<dyn Read>, std::io::Error>(Box::new(Cursor::new(
                b"<?php echo 'hello';".to_vec(),
            )))
        },
    )
    .expect("scan web candidates");

    assert_eq!(
        read_order.into_inner(),
        vec![
            "web-a".to_string(),
            "web-m".to_string(),
            "web-z".to_string()
        ]
    );
}

#[test]
fn deterministic_linux_unsupported_candidate_is_replayed_without_opening_evidence() {
    let conn = source_connection();
    insert_source_file(&conn, "ssh-moduli", "etc/ssh/moduli");
    let reader_calls = Cell::new(0usize);

    let first = run_analysis_extraction(
        &conn,
        "case-linux-diagnostic",
        DataSourcePlatform::Linux,
        &["LinuxSystemConfig"],
        |_| {
            reader_calls.set(reader_calls.get() + 1);
            Ok::<Box<dyn Read>, std::io::Error>(Box::new(Cursor::new(
                b"fixture SSH moduli".to_vec(),
            )))
        },
    )
    .expect("scan unsupported SSH candidate");
    let second = run_analysis_extraction(
        &conn,
        "case-linux-diagnostic",
        DataSourcePlatform::Linux,
        &["LinuxSystemConfig"],
        |_| {
            reader_calls.set(reader_calls.get() + 1);
            Ok::<Box<dyn Read>, std::io::Error>(Box::new(Cursor::new(
                b"fixture SSH moduli".to_vec(),
            )))
        },
    )
    .expect("replay unsupported SSH diagnostic");

    assert_eq!(first.scanned_count, 1);
    assert_eq!(second.scanned_count, 1);
    assert_eq!(first.checkpoint_hit_count, 0);
    assert_eq!(second.checkpoint_hit_count, 1);
    assert_eq!(reader_calls.get(), 0);
    assert_eq!(second.warnings, first.warnings);
    let diagnostic_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM source_meta
             WHERE key LIKE 'analysis_candidate_scan:diagnostic:%'",
            [],
            |row| row.get(0),
        )
        .expect("count diagnostic checkpoints");
    assert_eq!(diagnostic_count, 1);
}

#[test]
fn evidence_read_failure_remains_retryable_and_is_not_checkpointed() {
    let conn = source_connection();
    insert_source_file(&conn, "web-unreadable", "var/www/html/index.php");
    let reader_calls = Cell::new(0usize);

    for _ in 0..2 {
        let run = run_analysis_extraction(
            &conn,
            "case-linux-read-retry",
            DataSourcePlatform::Linux,
            &["LinuxWebServices"],
            |_| {
                reader_calls.set(reader_calls.get() + 1);
                Err::<Box<dyn Read>, _>(std::io::Error::other("temporary read failure"))
            },
        )
        .expect("return a retryable read warning");
        assert_eq!(run.checkpoint_hit_count, 0);
    }

    assert_eq!(reader_calls.get(), 2);
    let checkpoint_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM source_meta
             WHERE key LIKE 'analysis_candidate_scan:%'",
            [],
            |row| row.get(0),
        )
        .expect("count scan checkpoints");
    assert_eq!(checkpoint_count, 0);
}

#[test]
fn registry_preload_warnings_are_attributed_once_to_registry_section() {
    let conn = source_connection();
    insert_source_file(&conn, "registry-system", "Windows/System32/config/SYSTEM");
    let observed_limit = Cell::new(0usize);
    let cancel_token = AtomicBool::new(false);
    let run = run_analysis_extraction_with_cancel(
        &conn,
        "case-registry-failed",
        DataSourcePlatform::Windows,
        &["Registry"],
        &cancel_token,
        |_, read_limit| {
            observed_limit.set(read_limit);
            Err::<Box<dyn Read>, _>(std::io::Error::other("preload failure"))
        },
    )
    .expect("return failed Registry extraction status");

    let preload_warning = "Windows/System32/config/SYSTEM read failed: preload failure";
    assert_eq!(observed_limit.get(), MAX_REGISTRY_ANALYSIS_BYTES);
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
fn extraction_rejects_pre_cancelled_work_before_evidence_access() {
    let conn = source_connection();
    insert_source_file(&conn, "mail-cancelled", "mailbox/message.eml");
    let cancel_token = AtomicBool::new(true);
    let reader_calls = AtomicUsize::new(0);

    let error = run_analysis_extraction_with_cancel(
        &conn,
        "case-cancelled",
        DataSourcePlatform::Windows,
        &["Email"],
        &cancel_token,
        |_, _| {
            reader_calls.fetch_add(1, Ordering::Relaxed);
            Ok::<Box<dyn Read>, std::io::Error>(Box::new(Cursor::new(Vec::<u8>::new())))
        },
    )
    .expect_err("pre-cancelled extraction must stop");

    assert!(matches!(error, AnalysisServiceError::Cancelled));
    assert_eq!(reader_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn extraction_cancellation_between_candidates_prevents_partial_persistence() {
    let conn = source_connection();
    insert_source_file(&conn, "mail-first", "mailbox/first.eml");
    insert_source_file(&conn, "mail-second", "mailbox/second.eml");
    let cancel_token = AtomicBool::new(false);
    let reader_calls = AtomicUsize::new(0);

    let error = run_analysis_extraction_with_cancel(
        &conn,
        "case-cancel-after-first",
        DataSourcePlatform::Windows,
        &["Email"],
        &cancel_token,
        |_, _| {
            reader_calls.fetch_add(1, Ordering::Relaxed);
            cancel_token.store(true, Ordering::Relaxed);
            Ok::<Box<dyn Read>, std::io::Error>(Box::new(Cursor::new(
                b"From: analyst@example.test\r\n\r\ncancel fixture".to_vec(),
            )))
        },
    )
    .expect_err("cancellation during candidate processing must stop the run");

    assert!(matches!(error, AnalysisServiceError::Cancelled));
    assert_eq!(reader_calls.load(Ordering::Relaxed), 1);
    let artifact_count = ArtifactRepo::new(&conn)
        .count()
        .expect("count persisted artifacts");
    assert_eq!(artifact_count, 0);
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
