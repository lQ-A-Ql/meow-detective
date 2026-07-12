use super::*;
use crate::notebook_service;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::repositories::{case_repo::CaseRepo, notebook_repo::StepFilters};
use persistence_sqlite::runner;

fn setup(case_id: &str) -> Connection {
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
    conn
}

#[test]
fn case_state_hash_is_deterministic() {
    let conn = setup("case-hash");
    let hash1 = compute_case_state_hash(&conn, "case-hash");
    let hash2 = compute_case_state_hash(&conn, "case-hash");
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64); // SHA-256 hex
}

#[test]
fn case_state_hash_changes_when_counts_change() {
    let conn = setup("case-hash-change");
    let hash_before = compute_case_state_hash(&conn, "case-hash-change");

    // Insert data source first (required FK), then file entry to change counts
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path)
         VALUES ('ds-1', 'case-hash-change', 'test-ds', 'LogicalDirectory', '/tmp')",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO file_entries (id, data_source_id, path, name, entry_type, size)
         VALUES ('fe-1', 'ds-1', '/test.txt', 'test.txt', 'file', 100)",
        [],
    )
    .unwrap();

    // Insert an artifact to change counts further
    conn.execute(
        "INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, title)
         VALUES ('art-1', 'case-hash-change', 'ds-1', 'TestFamily', 'TestArtifact')",
        [],
    )
    .unwrap();

    let hash_after = compute_case_state_hash(&conn, "case-hash-change");
    assert_ne!(hash_before, hash_after);
    assert_eq!(hash_before.len(), 64);
    assert_eq!(hash_after.len(), 64);
}

#[test]
fn record_step_returns_dto_with_hash() {
    let conn = setup("case-step");
    let dto = record_step(
        &conn,
        "case-step",
        "search",
        r#"{"query":"test"}"#,
        150,
        true,
        None,
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
    let conn = setup("case-fail");
    let dto = record_step(
        &conn,
        "case-fail",
        "import",
        r#"{"source":"disk.img"}"#,
        3000,
        false,
        Some("E_IMPORT_FAILED"),
    )
    .unwrap();

    assert!(!dto.success);
    assert_eq!(dto.error_code.as_deref(), Some("E_IMPORT_FAILED"));
    assert!(dto.case_state_hash.is_some());
}

#[test]
fn recorded_steps_are_persisted_and_listable() {
    let conn = setup("case-persist");
    record_step(
        &conn,
        "case-persist",
        "search",
        r#"{"query":"needle"}"#,
        42,
        true,
        None,
    )
    .unwrap();
    record_step(
        &conn,
        "case-persist",
        "import",
        r#"{"source":"e01"}"#,
        5000,
        false,
        Some("E_CANCELLED"),
    )
    .unwrap();

    let all = notebook_service::list_steps(&conn, "case-persist", &StepFilters::default()).unwrap();
    assert_eq!(all.len(), 2);

    let search_filter = StepFilters {
        step_kind: Some("search".to_string()),
        ..Default::default()
    };
    let filtered = notebook_service::list_steps(&conn, "case-persist", &search_filter).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].step_kind, "search");
    assert_eq!(filtered[0].duration_ms, 42);
}
