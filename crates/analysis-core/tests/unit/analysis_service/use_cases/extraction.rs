use super::*;
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::audit_repo::AuditRepo;

fn case_connection() -> Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open case database");
    persistence_sqlite::runner::run_all(&connection).expect("run case migrations");
    connection
}

#[test]
fn plugin_audit_trail_records_load_reject_and_extract_failure() {
    let connection = case_connection();
    let loads = vec![PluginLoadRecord {
        plugin_id: "meow.fixture.good".to_string(),
        plugin_version: "0.1.0".to_string(),
    }];
    let rejections = vec![PluginRejection {
        path: std::path::PathBuf::from("plugins/bad.dll"),
        reason: "ABI version 1 != host 2".to_string(),
    }];
    let failures = vec![PluginExtractFailure {
        plugin_id: "meow.fixture.good".to_string(),
        source_path: "[P0]/Evidence/FOO.MFX".to_string(),
        error: "payload is not valid JSON".to_string(),
    }];

    record_plugin_audit_trail(
        &connection,
        &CaseId("case-1".to_string()),
        &DataSourceId("source-1".to_string()),
        &loads,
        &rejections,
        &failures,
    );

    let repo = AuditRepo::new(&connection);
    let load_entries = repo
        .query(Some("case-1"), Some("plugin.load"), 10, 0)
        .expect("query plugin.load");
    assert_eq!(load_entries.len(), 1);
    assert_eq!(
        load_entries[0].resource_id.as_deref(),
        Some("meow.fixture.good")
    );
    assert_eq!(load_entries[0].resource_type, "plugin");
    assert!(load_entries[0].details.contains("0.1.0"));

    let reject_entries = repo
        .query(Some("case-1"), Some("plugin.reject"), 10, 0)
        .expect("query plugin.reject");
    assert_eq!(reject_entries.len(), 1);
    assert!(reject_entries[0].details.contains("ABI version"));

    let failure_entries = repo
        .query(Some("case-1"), Some("plugin.extract_failed"), 10, 0)
        .expect("query plugin.extract_failed");
    assert_eq!(failure_entries.len(), 1);
    assert_eq!(
        failure_entries[0].resource_id.as_deref(),
        Some("meow.fixture.good")
    );
    assert!(failure_entries[0].details.contains("[P0]/Evidence/FOO.MFX"));
}

#[test]
fn empty_plugin_audit_trail_writes_nothing() {
    let connection = case_connection();
    record_plugin_audit_trail(
        &connection,
        &CaseId("case-1".to_string()),
        &DataSourceId("source-1".to_string()),
        &[],
        &[],
        &[],
    );
    let count = AuditRepo::new(&connection)
        .count(Some("case-1"))
        .expect("count audit entries");
    assert_eq!(count, 0);
}

#[test]
fn cancellation_during_resource_admission_remains_a_cancelled_error() {
    let runtime = AnalysisSourceReadRuntime::default();
    let mut progress = |_: ExtractionProgressUpdate| {};
    let control = AnalysisExecutionControl {
        cancel_token: Arc::new(AtomicBool::new(true)),
        source_runtime: &runtime,
        progress: &mut progress,
        browser_password: None,
    };
    assert!(matches!(
        admit_extraction(&control),
        Err(AnalysisServiceError::Cancelled)
    ));
}

#[test]
fn progress_cancellation_observes_the_callers_live_token() {
    use chrono::Utc;
    use domain::{
        DataSource, DataSourceKind, DataSourceProvenance, EntryType, FileEntry, FileEntryId,
    };
    use persistence_sqlite::repositories::{
        case_repo::CaseRepo,
        datasource_repo::{DataSourceRepo, DataSourceStorage},
    };
    use std::sync::atomic::Ordering;

    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.codex-tmp/parser-lifecycle-tests");
    std::fs::create_dir_all(&workspace).unwrap();
    let root = tempfile::tempdir_in(&workspace).unwrap();
    let case_root = root.path().join("case");
    let evidence_root = root.path().join("evidence");
    std::fs::create_dir_all(&case_root).unwrap();
    std::fs::create_dir_all(&evidence_root).unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/email/simple.mbox");
    let evidence = evidence_root.join("inbox.mbox");
    std::fs::copy(fixture, &evidence).unwrap();
    let case_conn = persistence_sqlite::open_or_create(&case_root.join("app.db")).unwrap();
    persistence_sqlite::runner::run_all(&case_conn).unwrap();
    let case_id = CaseId("cancel-case".into());
    CaseRepo::new(&case_conn)
        .create(&domain::CaseMeta {
            id: case_id.clone(),
            name: "Cancellation".into(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
    let source_id = DataSourceId("cancel-source".into());
    let source = DataSource {
        id: source_id.clone(),
        name: "Mail".into(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: evidence_root,
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source_id.0, Some("windows"), None);
    storage.import_state = "ready".into();
    DataSourceRepo::new(&case_conn)
        .insert_with_storage(&case_id, &source, &storage)
        .unwrap();
    let connection = source_kernel::source_db::open_source_db(&case_root, &source_id).unwrap();
    DataSourceRepo::new(&connection)
        .upsert_source_local_metadata(&case_id, &source)
        .unwrap();
    let entry = FileEntry {
        id: FileEntryId("cancel-mail".into()),
        parent_id: None,
        data_source_id: source_id.clone(),
        path: "inbox.mbox".into(),
        name: "inbox.mbox".into(),
        entry_type: EntryType::File,
        size: Some(evidence.metadata().unwrap().len()),
        ext: Some("mbox".into()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    };
    persistence_source::repositories::file_repo::FileRepo::new(&connection)
        .insert_batch(&[entry])
        .unwrap();
    let store = persistence_source::detail::open_store_for_write(&connection)
        .unwrap()
        .unwrap();
    persistence_source::fjall::replace_source_generation(
        &store,
        &source_id.0,
        "cancel",
        &"a".repeat(64),
    )
    .unwrap();
    drop(store);
    drop(connection);
    let runtime = AnalysisSourceReadRuntime::default();
    let cancel = Arc::new(AtomicBool::new(false));
    let request_cancel = cancel.clone();
    let result = run_source_analysis_extraction_with_progress_and_cancel(
        &case_conn,
        &case_root,
        &case_id,
        &source_id,
        &["Email"],
        AnalysisExtractionProgressContext::new(&runtime, "cancel-run"),
        cancel,
        move |_| request_cancel.store(true, Ordering::Release),
    );
    assert!(matches!(result, Err(AnalysisServiceError::Cancelled)));
}
