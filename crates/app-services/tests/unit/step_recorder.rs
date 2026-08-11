use super::*;
use crate::notebook_service;
use domain::{CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    notebook_repo::StepFilters,
};
use persistence_sqlite::runner;

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
    let source_path =
        crate::source_db::source_db_path(&case.case_root, &DataSourceId(source_id.to_string()));
    std::fs::create_dir_all(source_path.parent().expect("source directory"))
        .expect("create source directory");
    drop(persistence_sqlite::open_or_create_source(&source_path).expect("create source db"));
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
    let source_path = register_ready_source(&case, "case-hash-change", "ds-1");
    let hash_before = compute_case_state_hash(&case.conn, &case.case_root, "case-hash-change");

    let source_conn = persistence_sqlite::open_or_create_source(&source_path).unwrap();
    source_conn
        .execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type, size)
             VALUES ('fe-1', 'ds-1', '/test.txt', 'test.txt', 'file', 100)",
            [],
        )
        .unwrap();
    source_conn
        .execute(
            "INSERT INTO artifacts (id, data_source_id, artifact_type, title)
             VALUES ('art-1', 'ds-1', 'TestFamily', 'TestArtifact')",
            [],
        )
        .unwrap();
    drop(source_conn);

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

    let all =
        notebook_service::list_steps(&case.conn, "case-persist", &StepFilters::default()).unwrap();
    assert_eq!(all.len(), 2);

    let search_filter = StepFilters {
        step_kind: Some("search".to_string()),
        ..Default::default()
    };
    let filtered =
        notebook_service::list_steps(&case.conn, "case-persist", &search_filter).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].step_kind, "search");
    assert_eq!(filtered[0].duration_ms, 42);
}
