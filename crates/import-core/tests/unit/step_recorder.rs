use super::*;
crate::native_source_test_support!();
use domain::{CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_source::repositories::{artifact_repo::ArtifactRepo, file_repo::FileRepo};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    notebook_repo::{NotebookRepo, StepFilters},
};
use persistence_sqlite::runner;
use test_support::mark_source_projection_complete;

struct TestCase {
    // Holds the tempdir alive for the case root's lifetime.
    _root: tempfile::TempDir,
    case_root: std::path::PathBuf,
    conn: Connection,
}

fn setup(case_id: &str) -> TestCase {
    let root = tempfile::TempDir::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    CaseRepo::new(&conn)
        .create(&CaseMeta {
            id: CaseId(case_id.to_string()),
            name: "Step Recorder Test".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .unwrap();
    TestCase {
        case_root: root.path().to_path_buf(),
        _root: root,
        conn,
    }
}

/// Register a ready source backed by a real on-disk source database so the
/// ready-source router can open it.
fn register_ready_source(case: &TestCase, case_id: &str, source_id: &str) -> std::path::PathBuf {
    let source = DataSource {
        id: DataSourceId(source_id.to_string()),
        name: source_id.to_string(),
        kind: DataSourceKind::E01,
        source_path: case.case_root.join(format!("{source_id}.E01")),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some("windows"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(&case.conn)
        .insert_with_storage(&CaseId(case_id.to_string()), &source, &storage)
        .expect("register source");
    let source_path = source_kernel::source_db::source_db_path(
        &case.case_root,
        &DataSourceId(source_id.to_string()),
    );
    std::fs::create_dir_all(source_path.parent().expect("source directory"))
        .expect("create source directory");
    let source_conn =
        persistence_sqlite::open_or_create_source(&source_path).expect("create source db");
    persistence_source::detail::configure_source_identity(&source_conn, source_id)
        .expect("configure source identity");
    persistence_source::detail::configure_source_fjall(&source_conn, &source_path)
        .expect("configure native source");
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&CaseId(case_id.to_string()), &source)
        .expect("persist source-local metadata");
    source_path
}

#[test]
fn case_state_hash_is_deterministic() {
    let case = setup("case-hash");
    let hash1 = compute_case_state_hash(&case.conn, &case.case_root, "case-hash");
    let hash2 = compute_case_state_hash(&case.conn, &case.case_root, "case-hash");
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64); // SHA-256 hex
}

#[test]
fn case_state_hash_changes_when_counts_change() {
    let case = setup("case-hash-change");
    let _source_path = register_ready_source(&case, "case-hash-change", "ds-1");
    let hash_before = compute_case_state_hash(&case.conn, &case.case_root, "case-hash-change");

    let source_conn = source_kernel::source_db::open_source_db(
        &case.case_root,
        &DataSourceId("ds-1".to_string()),
    )
    .unwrap();
    FileRepo::new(&source_conn)
        .insert_batch(&[domain::FileEntry {
            id: domain::FileEntryId("fe-1".into()),
            parent_id: None,
            data_source_id: domain::DataSourceId("ds-1".into()),
            path: "/test.txt".into(),
            name: "test.txt".into(),
            entry_type: domain::EntryType::File,
            size: Some(100),
            ext: Some("txt".into()),
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
        }])
        .unwrap();
    ArtifactRepo::new(&source_conn)
        .insert_batch(
            &[domain::Artifact {
                id: domain::ArtifactId("art-1".into()),
                family: "TestFamily".into(),
                title: "TestArtifact".into(),
                summary: String::new(),
                source_object_id: None,
                extractor_id: None,
                extractor_version: None,
                confidence: None,
                source_attribution: None,
                created_at: chrono::Utc::now(),
                attrs: std::collections::BTreeMap::new(),
            }],
            "case-hash-change",
            "ds-1",
        )
        .unwrap();
    let store = persistence_source::detail::open_store_for_write(&source_conn)
        .expect("open source store")
        .expect("configured source store");
    persistence_source::fjall::replace_source_generation(
        &store,
        "ds-1",
        "step-recorder-fixture",
        &"a".repeat(64),
    )
    .unwrap();
    drop(store);
    mark_source_projection_complete(&source_conn, "ds-1");
    drop(source_conn);

    let ready_sources = source_kernel::source_db::open_ready_source_connections_read_only(
        &case.conn,
        &case.case_root,
        &CaseId("case-hash-change".to_string()),
    )
    .expect("open ready source in state hash");
    assert_eq!(ready_sources.len(), 1);
    let inventory_source = DataSourceId("ds-1".to_string());
    assert_eq!(
        source_kernel::source_db::ready_source_catalog_count(&case.case_root, &inventory_source)
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        source_kernel::source_db::ready_source_artifact_count(&case.case_root, &inventory_source)
            .unwrap(),
        Some(1)
    );
    let hash_after = compute_case_state_hash(&case.conn, &case.case_root, "case-hash-change");
    assert_ne!(hash_before, hash_after);
    assert_eq!(hash_before.len(), 64);
    assert_eq!(hash_after.len(), 64);
}

#[test]
fn case_state_hash_ignores_non_ready_sources() {
    let case = setup("case-hash-pending");
    let source = DataSource {
        id: DataSourceId("ds-pending".to_string()),
        name: "ds-pending".to_string(),
        kind: DataSourceKind::E01,
        source_path: case.case_root.join("ds-pending.E01"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db("ds-pending", Some("windows"), None);
    storage.import_state = "pending".to_string();
    DataSourceRepo::new(&case.conn)
        .insert_with_storage(&CaseId("case-hash-pending".to_string()), &source, &storage)
        .unwrap();

    let hash1 = compute_case_state_hash(&case.conn, &case.case_root, "case-hash-pending");
    let hash2 = compute_case_state_hash(&case.conn, &case.case_root, "case-hash-pending");
    assert_eq!(hash1, hash2);
}

#[test]
fn record_step_returns_dto_with_hash() {
    let case = setup("case-step");
    let dto = record_step(
        &case.conn,
        &case.case_root,
        CaseStepInput {
            case_id: "case-step",
            step_kind: "search",
            params_json: r#"{"query":"test"}"#,
            duration_ms: 150,
            success: true,
            error_code: None,
        },
    )
    .unwrap();

    assert_eq!(dto.case_id, "case-step");
    assert_eq!(dto.step_kind, "search");
    assert_eq!(dto.params_json, r#"{"query":"test"}"#);
    assert_eq!(dto.duration_ms, 150);
    assert!(dto.success);
    assert!(dto.error_code.is_none());
    assert!(dto.case_state_hash.is_some());
    assert_eq!(dto.case_state_hash.as_deref().unwrap().len(), 64);
    assert!(!dto.id.is_empty());
    assert!(!dto.timestamp.is_empty());
}

#[test]
fn record_step_failure_captures_error_code() {
    let case = setup("case-fail");
    let dto = record_step(
        &case.conn,
        &case.case_root,
        CaseStepInput {
            case_id: "case-fail",
            step_kind: "import",
            params_json: r#"{"source":"disk.img"}"#,
            duration_ms: 3000,
            success: false,
            error_code: Some("E_IMPORT_FAILED"),
        },
    )
    .unwrap();

    assert!(!dto.success);
    assert_eq!(dto.error_code.as_deref(), Some("E_IMPORT_FAILED"));
    assert!(dto.case_state_hash.is_some());
}

#[test]
fn recorded_steps_are_persisted_and_listable() {
    let case = setup("case-persist");
    record_step(
        &case.conn,
        &case.case_root,
        CaseStepInput {
            case_id: "case-persist",
            step_kind: "search",
            params_json: r#"{"query":"needle"}"#,
            duration_ms: 42,
            success: true,
            error_code: None,
        },
    )
    .unwrap();
    record_step(
        &case.conn,
        &case.case_root,
        CaseStepInput {
            case_id: "case-persist",
            step_kind: "import",
            params_json: r#"{"source":"e01"}"#,
            duration_ms: 5000,
            success: false,
            error_code: Some("E_CANCELLED"),
        },
    )
    .unwrap();

    let all = NotebookRepo::new(&case.conn)
        .list_steps("case-persist", &StepFilters::default())
        .unwrap();
    assert_eq!(all.len(), 2);

    let search_filter = StepFilters {
        step_kind: Some("search".to_string()),
        ..Default::default()
    };
    let filtered = NotebookRepo::new(&case.conn)
        .list_steps("case-persist", &search_filter)
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].step_kind, "search");
    assert_eq!(filtered[0].duration_ms, Some(42));
}
