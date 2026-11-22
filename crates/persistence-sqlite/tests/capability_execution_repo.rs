use persistence_sqlite::repositories::capability_execution_repo::{
    CapabilityClaimOutcome, CapabilityClaimRequest, CapabilityCompletion,
    CapabilityExecutionAttempt, CapabilityExecutionRepo, CapabilityQueueOutcome,
};
use persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRepo;
use persistence_sqlite::{open_in_memory, runner};

fn setup() -> rusqlite::Connection {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute("INSERT INTO cases (id, name) VALUES ('case', 'Review')", [])
        .unwrap();
    for ordinal in 0..6 {
        conn.execute("INSERT INTO data_sources (id, case_id, name, kind, source_path) VALUES (?1, 'case', ?1, 'e01', '')", [format!("source-{ordinal}")]).unwrap();
    }
    conn
}

fn queue(conn: &rusqlite::Connection, source: &str) -> i64 {
    match CapabilityExecutionRepo::new(conn)
        .queue_retry("case", source, "artifacts")
        .unwrap()
    {
        CapabilityQueueOutcome::Queued(row) => row.revision,
        _ => panic!("request must be queued"),
    }
}

fn claim(conn: &rusqlite::Connection, revision: i64) -> CapabilityExecutionAttempt {
    match CapabilityExecutionRepo::new(conn)
        .claim(CapabilityClaimRequest {
            case_id: "case",
            source_id: "source-0",
            capability: "artifacts",
            revision,
            input_fingerprint: &"a".repeat(64),
            owner: "process-a",
        })
        .unwrap()
    {
        CapabilityClaimOutcome::Acquired(attempt) => attempt,
        _ => panic!("queued attempt must be acquired"),
    }
}

fn ready() -> CapabilityCompletion<'static> {
    CapabilityCompletion {
        state: "ready",
        detail: None,
        error_code: None,
    }
}

#[test]
fn ordinary_source_execution_uses_the_shared_phase_attempt() {
    let conn = setup();
    let revision = queue(&conn, "source-0");
    let attempt = claim(&conn, revision);
    assert!(attempt.phase.owner_id.is_some());
    assert!(attempt.phase.attempt_id.is_some());
    let result = CapabilityExecutionRepo::new(&conn)
        .complete(&attempt, ready())
        .unwrap()
        .unwrap();
    assert_eq!(result.state, "ready");
    assert_eq!(result.revision, revision + 1);
}

#[test]
fn cancelled_worker_cannot_publish_over_a_new_queued_request() {
    let conn = setup();
    let revision = queue(&conn, "source-0");
    let old = claim(&conn, revision);
    let repo = CapabilityExecutionRepo::new(&conn);
    assert!(repo.cancel("case", "source-0", "artifacts").unwrap());
    let next_revision = queue(&conn, "source-0");
    assert!(next_revision > revision);
    assert!(repo.complete(&old, ready()).unwrap().is_none());
    assert_eq!(
        SourceCapabilityRepo::new(&conn)
            .get("case", "source-0", "artifacts")
            .unwrap()
            .unwrap()
            .state,
        "queued"
    );
    let next = claim(&conn, next_revision);
    assert_ne!(old.phase.attempt_id, next.phase.attempt_id);
    assert!(repo.complete(&next, ready()).unwrap().is_some());
}

#[test]
fn full_retry_queue_does_not_create_or_change_the_rejected_row() {
    let conn = setup();
    for ordinal in 0..4 {
        queue(&conn, &format!("source-{ordinal}"));
    }
    assert!(matches!(
        CapabilityExecutionRepo::new(&conn)
            .queue_retry("case", "source-4", "artifacts")
            .unwrap(),
        CapabilityQueueOutcome::Full
    ));
    assert!(SourceCapabilityRepo::new(&conn)
        .get("case", "source-4", "artifacts")
        .unwrap()
        .is_none());
}

#[test]
fn duplicate_retry_does_not_change_the_running_attempt() {
    let conn = setup();
    let revision = queue(&conn, "source-0");
    let attempt = claim(&conn, revision);
    assert!(matches!(
        CapabilityExecutionRepo::new(&conn)
            .queue_retry("case", "source-0", "artifacts")
            .unwrap(),
        CapabilityQueueOutcome::AlreadyActive
    ));
    assert!(CapabilityExecutionRepo::new(&conn)
        .complete(&attempt, ready())
        .unwrap()
        .is_some());
}

#[test]
fn expired_lease_cannot_replace_an_existing_capability_owner() {
    let conn = setup();
    let revision = queue(&conn, "source-0");
    let attempt = claim(&conn, revision);
    conn.execute(
        "UPDATE data_source_processing_phases SET lease_expires_at = datetime('now', '-1 second')",
        [],
    )
    .unwrap();
    SourceCapabilityRepo::new(&conn)
        .requeue_running("case")
        .unwrap();
    assert!(matches!(
        CapabilityExecutionRepo::new(&conn)
            .claim(CapabilityClaimRequest {
                case_id: "case",
                source_id: "source-0",
                capability: "artifacts",
                revision,
                input_fingerprint: &"a".repeat(64),
                owner: "process-b",
            })
            .unwrap(),
        CapabilityClaimOutcome::Busy
    ));
    let current = persistence_sqlite::repositories::processing_phase_repo::DataSourceProcessingPhaseRepo::new(&conn)
        .find(&domain::DataSourceId("source-0".into()), attempt.phase.phase)
        .unwrap()
        .unwrap();
    assert_eq!(current.attempt_id, attempt.phase.attempt_id);
    assert_eq!(current.owner_id, attempt.phase.owner_id);
}

#[test]
fn wrong_case_and_stale_queue_revision_cannot_claim_work() {
    let conn = setup();
    assert!(CapabilityExecutionRepo::new(&conn)
        .queue_retry("another-case", "source-0", "artifacts")
        .is_err());
    let revision = queue(&conn, "source-0");
    assert!(matches!(
        CapabilityExecutionRepo::new(&conn)
            .claim(CapabilityClaimRequest {
                case_id: "case",
                source_id: "source-0",
                capability: "artifacts",
                revision: revision - 1,
                input_fingerprint: &"a".repeat(64),
                owner: "process-a",
            })
            .unwrap(),
        CapabilityClaimOutcome::Superseded
    ));
}
