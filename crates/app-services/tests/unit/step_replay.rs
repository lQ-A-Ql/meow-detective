use super::*;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::repositories::{case_repo::CaseRepo, notebook_repo::NotebookRepo};
use persistence_sqlite::runner;

fn setup(case_id: &str) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    CaseRepo::new(&conn)
        .create(&CaseMeta {
            id: CaseId(case_id.to_string()),
            name: "Step Replay Test".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .unwrap();
    conn
}

fn insert_step(conn: &Connection, id: &str, case_id: &str, step_kind: &str, params_json: &str) {
    let repo = NotebookRepo::new(conn);
    repo.record_step(
        &persistence_sqlite::repositories::notebook_repo::InvestigationStep {
            id: id.to_string(),
            case_id: case_id.to_string(),
            step_kind: step_kind.to_string(),
            params_json: params_json.to_string(),
            timestamp: "2026-06-14T12:00:00Z".to_string(),
            duration_ms: Some(100),
            case_state_hash: None,
            success: Some(true),
            error_code: None,
        },
    )
    .unwrap();
}

#[test]
fn replay_search_step_parses_query_and_replays() {
    let conn = setup("case-replay-search");
    let tmp = tempfile::TempDir::new().unwrap();
    let index_dir = tmp.path().join("index");

    // Pre-create and populate search index for the replay to find
    let index = search::SearchIndex::create(&index_dir).unwrap();
    let text = search::ExtractedText {
        file_id: "file-1".to_string(),
        content: "forensic needle in a haystack".to_string(),
        encoding: "utf-8".to_string(),
        extractable: true,
        byte_count: 32,
    };
    index.index_documents(&[text], &[]).unwrap();
    // Close index so replay can open it fresh
    drop(index);

    insert_step(
        &conn,
        "step-search-1",
        "case-replay-search",
        "search",
        r#"{"query":"needle","caseId":"case-replay-search"}"#,
    );
    insert_step(
        &conn,
        "step-search-2",
        "case-replay-search",
        "search",
        r#"{"query":"nonexistent","caseId":"case-replay-search"}"#,
    );

    let result = replay_steps(
        &conn,
        &index_dir,
        "case-replay-search",
        "step-search-1",
        "step-search-2",
    )
    .unwrap();

    assert_eq!(result.matched_steps.len(), 2);
    assert!(result.differed_steps.is_empty());
    assert!(result.failed_steps.is_empty());

    // First step should find the needle
    assert!(result.matched_steps[0].detail.contains("1 hits"));
    // Second should find 0
    assert!(result.matched_steps[1].detail.contains("0 hits"));
}

#[test]
fn non_search_steps_are_not_yet_replayable() {
    let conn = setup("case-replay-other");
    let tmp = tempfile::TempDir::new().unwrap();
    let index_dir = tmp.path().join("index");
    std::fs::create_dir_all(&index_dir).unwrap();

    insert_step(
        &conn,
        "step-import-1",
        "case-replay-other",
        "import",
        r#"{"source":"disk.img"}"#,
    );
    insert_step(
        &conn,
        "step-artifact-1",
        "case-replay-other",
        "artifact_extract",
        r#"{"family":"LNK"}"#,
    );

    let result = replay_steps(
        &conn,
        &index_dir,
        "case-replay-other",
        "step-import-1",
        "step-artifact-1",
    )
    .unwrap();

    assert!(result.matched_steps.is_empty());
    assert_eq!(result.differed_steps.len(), 2);
    assert!(result.failed_steps.is_empty());

    for differ in &result.differed_steps {
        assert_eq!(differ.actual, "not yet replayable");
    }
}

#[test]
fn invalid_range_returns_caveat() {
    let conn = setup("case-replay-invalid");
    let tmp = tempfile::TempDir::new().unwrap();
    let index_dir = tmp.path().join("index");
    std::fs::create_dir_all(&index_dir).unwrap();

    let result = replay_steps(
        &conn,
        &index_dir,
        "case-replay-invalid",
        "nonexistent-1",
        "nonexistent-2",
    )
    .unwrap();

    assert!(result.matched_steps.is_empty());
    assert!(result.differed_steps.is_empty());
    assert!(result.failed_steps.is_empty());
    assert_eq!(result.caveats.len(), 1);
    assert!(result.caveats[0].contains("not found"));
}
