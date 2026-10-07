use persistence_sqlite::{
    repositories::notebook_repo::{InvestigationStep, NotebookRepo},
    runner,
};
use rusqlite::Connection;
use serde_json::json;

fn database() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    for case in ["case-a", "case-b"] {
        conn.execute("INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?1, '2026-10-07', '2026-10-07')", [case]).unwrap();
    }
    conn
}

#[test]
fn phase_attempts_are_isolated_by_case_step_kind_import_set_and_phase() {
    let conn = database();
    let repo = NotebookRepo::new(&conn);
    assert_eq!(
        repo.count_import_phase_steps("case-a", "linux_evidence_set_import", "set-a", "initialize")
            .unwrap(),
        0
    );
    for (index, (case, kind, set, phase)) in [
        ("case-a", "linux_evidence_set_import", "set-a", "initialize"),
        ("case-a", "linux_evidence_set_import", "set-a", "initialize"),
        ("case-b", "linux_evidence_set_import", "set-a", "initialize"),
        ("case-a", "other_step", "set-a", "initialize"),
        ("case-a", "linux_evidence_set_import", "set-b", "initialize"),
        ("case-a", "linux_evidence_set_import", "set-a", "finalize"),
    ]
    .into_iter()
    .enumerate()
    {
        repo.record_step(&InvestigationStep {
            id: format!("step-{index}"),
            case_id: case.into(),
            step_kind: kind.into(),
            params_json: json!({"importSetId":set,"phase":phase}).to_string(),
            timestamp: "2026-10-07T00:00:00Z".into(),
            duration_ms: Some(0),
            case_state_hash: None,
            success: Some(index != 0),
            error_code: None,
        })
        .unwrap();
    }
    assert_eq!(
        repo.count_import_phase_steps("case-a", "linux_evidence_set_import", "set-a", "initialize")
            .unwrap(),
        2
    );
    assert_eq!(
        repo.count_import_phase_steps("case-a", "linux_evidence_set_import", "set-b", "initialize")
            .unwrap(),
        1
    );
    assert_eq!(
        repo.count_import_phase_steps("case-a", "linux_evidence_set_import", "set-a", "finalize")
            .unwrap(),
        1
    );
    assert_eq!(
        repo.count_import_phase_steps(
            "case-a",
            "linux_evidence_set_import",
            "set-a' OR 1=1 --",
            "initialize"
        )
        .unwrap(),
        0
    );
}

#[test]
fn failed_phase_query_is_not_reported_as_zero_attempts() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(NotebookRepo::new(&conn)
        .count_import_phase_steps("case", "kind", "set", "phase")
        .is_err());
}
