use domain::DataSourceId;
use persistence_sqlite::{
    open_in_memory,
    repositories::processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim,
        ProcessingPhaseCompletion, ProcessingPhaseState, ProcessingPhaseTransition,
    },
    runner,
};
use rusqlite::{params, Connection};

const DERIVED_SOURCE_ID: &str = "derived-vm-100";
const ORDINARY_SOURCE_ID: &str = "source-osd-0";
const OWNER_A: &str = "test-process-a";
const OWNER_B: &str = "test-process-b";

fn fingerprint(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn setup_case_db() -> Connection {
    let conn = open_in_memory().expect("open case database");
    runner::run_all(&conn).expect("run case migrations");
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-1', 'PVE Case')",
        [],
    )
    .expect("insert case");
    for (source_id, kind) in [(DERIVED_SOURCE_ID, "ceph_rbd"), (ORDINARY_SOURCE_ID, "e01")] {
        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path)
             VALUES (?1, 'case-1', ?1, ?2, '')",
            params![source_id, kind],
        )
        .expect("insert data source");
    }
    conn
}

fn acquire(
    repo: &DataSourceProcessingPhaseRepo<'_>,
    source_id: &DataSourceId,
    phase: ProcessingPhase,
    version: u32,
    input: &str,
    owner: &str,
) -> persistence_sqlite::repositories::processing_phase_repo::DataSourceProcessingPhaseRecord {
    match repo
        .claim(source_id, phase, version, input, owner)
        .expect("claim processing phase")
    {
        ProcessingPhaseClaim::Acquired(record) => record,
        other => panic!("expected acquired phase, got {other:?}"),
    }
}

#[test]
fn migration_installs_constrained_phase_ledger_with_cascade_delete() {
    let conn = setup_case_db();
    assert_eq!(runner::latest_version(), "0042_file_entry_encrypted");

    let columns = conn
        .prepare("SELECT name FROM pragma_table_info('data_source_processing_phases')")
        .expect("prepare column query")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect columns");
    assert_eq!(
        columns,
        [
            "data_source_id",
            "phase",
            "state",
            "version",
            "input_fingerprint",
            "owner_id",
            "attempt_id",
            "stats_json",
            "last_error",
            "started_at",
            "completed_at",
            "heartbeat_at",
            "lease_expires_at",
            "updated_at",
        ]
    );

    let valid_fingerprint = fingerprint('a');
    for (phase, state, version, input_fingerprint, stats_json) in [
        ("unknown", "pending", 1, valid_fingerprint.as_str(), "{}"),
        ("catalog", "unknown", 1, valid_fingerprint.as_str(), "{}"),
        ("catalog", "pending", 0, valid_fingerprint.as_str(), "{}"),
        ("catalog", "pending", 1, "not-a-fingerprint", "{}"),
        ("catalog", "pending", 1, valid_fingerprint.as_str(), "[]"),
    ] {
        assert!(conn
            .execute(
                "INSERT INTO data_source_processing_phases (
                    data_source_id, phase, state, version, input_fingerprint, stats_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    DERIVED_SOURCE_ID,
                    phase,
                    state,
                    version,
                    input_fingerprint,
                    stats_json
                ],
            )
            .is_err());
    }

    conn.execute(
        "INSERT INTO data_source_processing_phases (
            data_source_id, phase, version, input_fingerprint
         ) VALUES (?1, 'catalog', 1, ?2)",
        params![DERIVED_SOURCE_ID, valid_fingerprint],
    )
    .expect("insert valid phase");
    let index_exists: bool = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type = 'index'
                  AND name = 'idx_data_source_processing_phases_state'
             )",
            [],
            |row| row.get(0),
        )
        .expect("query phase index");
    assert!(index_exists);

    conn.execute(
        "DELETE FROM data_sources WHERE id = ?1",
        [DERIVED_SOURCE_ID],
    )
    .expect("delete derived source");
    let remaining: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM data_source_processing_phases",
            [],
            |row| row.get(0),
        )
        .expect("count processing phases");
    assert_eq!(remaining, 0);
}

#[test]
fn repository_claims_finishes_and_lists_in_catalog_order() {
    let conn = setup_case_db();
    let repo = DataSourceProcessingPhaseRepo::new(&conn);
    let source_id = DataSourceId(DERIVED_SOURCE_ID.to_string());
    let input = fingerprint('b');

    for phase in [
        ProcessingPhase::Search,
        ProcessingPhase::Platform,
        ProcessingPhase::Catalog,
    ] {
        let pending = repo
            .upsert(&source_id, phase, 1, &input)
            .expect("upsert phase");
        assert_eq!(pending.state, ProcessingPhaseState::Pending);
    }

    let running = acquire(
        &repo,
        &source_id,
        ProcessingPhase::Catalog,
        1,
        &input,
        OWNER_A,
    );
    assert_eq!(running.state, ProcessingPhaseState::Running);
    assert_eq!(running.owner_id.as_deref(), Some(OWNER_A));
    assert!(running.attempt_id.is_some());
    assert!(running.lease_expires_at.is_some());

    let ready = repo
        .finish(
            &source_id,
            ProcessingPhase::Catalog,
            ProcessingPhaseCompletion::new(
                1,
                &input,
                OWNER_A,
                running.attempt_id.as_deref().expect("attempt ID"),
                ProcessingPhaseTransition::ready(r#"{"files":42}"#),
            ),
        )
        .expect("complete catalog");
    assert_eq!(ready.state, ProcessingPhaseState::Ready);
    assert!(ready.completed_at.is_some());
    assert!(ready.lease_expires_at.is_none());

    let listed = repo.list_for_data_source(&source_id).expect("list phases");
    assert_eq!(
        listed.iter().map(|record| record.phase).collect::<Vec<_>>(),
        [
            ProcessingPhase::Catalog,
            ProcessingPhase::Platform,
            ProcessingPhase::Search,
        ]
    );
    assert_eq!(
        repo.claim(&source_id, ProcessingPhase::Catalog, 1, &input, OWNER_A)
            .expect("claim ready phase"),
        ProcessingPhaseClaim::Ready(ready)
    );
}

#[test]
fn claim_is_single_owner_and_reclaims_only_after_lease_expiry() {
    let conn = setup_case_db();
    let repo = DataSourceProcessingPhaseRepo::new(&conn);
    let source_id = DataSourceId(DERIVED_SOURCE_ID.to_string());
    let input = fingerprint('c');
    let first = acquire(
        &repo,
        &source_id,
        ProcessingPhase::Graph,
        1,
        &input,
        OWNER_A,
    );

    let busy = repo
        .claim(&source_id, ProcessingPhase::Graph, 1, &input, OWNER_A)
        .expect("repeat same-process claim");
    assert!(matches!(busy, ProcessingPhaseClaim::Busy(_)));

    let other_owner_busy = repo
        .claim(&source_id, ProcessingPhase::Graph, 1, &input, OWNER_B)
        .expect("different owner cannot steal an active lease");
    assert!(matches!(other_owner_busy, ProcessingPhaseClaim::Busy(_)));

    conn.execute(
        "UPDATE data_source_processing_phases
         SET lease_expires_at = datetime('now', '-1 second')
         WHERE data_source_id = ?1 AND phase = 'graph'",
        [&source_id.0],
    )
    .expect("expire graph lease");

    let replacement = acquire(
        &repo,
        &source_id,
        ProcessingPhase::Graph,
        1,
        &input,
        OWNER_B,
    );
    assert_ne!(replacement.attempt_id, first.attempt_id);
    assert!(repo
        .finish(
            &source_id,
            ProcessingPhase::Graph,
            ProcessingPhaseCompletion::new(
                1,
                &input,
                OWNER_A,
                first.attempt_id.as_deref().expect("first attempt"),
                ProcessingPhaseTransition::ready("{}"),
            ),
        )
        .is_err());

    let heartbeat = repo
        .heartbeat(
            &source_id,
            ProcessingPhase::Graph,
            1,
            &input,
            OWNER_B,
            replacement
                .attempt_id
                .as_deref()
                .expect("replacement attempt"),
        )
        .expect("heartbeat replacement");
    assert_eq!(heartbeat.state, ProcessingPhaseState::Running);
}

#[test]
fn recovery_marks_interrupted_phases_failed_and_retryable() {
    let conn = setup_case_db();
    let repo = DataSourceProcessingPhaseRepo::new(&conn);
    let source_id = DataSourceId(DERIVED_SOURCE_ID.to_string());
    let input = fingerprint('6');
    let running = acquire(
        &repo,
        &source_id,
        ProcessingPhase::Search,
        1,
        &input,
        OWNER_A,
    );
    conn.execute(
        "UPDATE data_source_processing_phases
         SET lease_expires_at = datetime('now', '-1 second')
         WHERE data_source_id = ?1 AND phase = 'search'",
        [&source_id.0],
    )
    .expect("expire interrupted phase lease");

    let recovered = repo
        .recover_interrupted("Interrupted: application exited unexpectedly")
        .expect("recover interrupted phase");
    assert_eq!(recovered, 1);

    let failed = repo
        .find(&source_id, ProcessingPhase::Search)
        .expect("query recovered phase")
        .expect("recovered phase exists");
    assert_eq!(failed.state, ProcessingPhaseState::Failed);
    assert_eq!(
        failed.last_error.as_deref(),
        Some("Interrupted: application exited unexpectedly")
    );
    assert!(failed.lease_expires_at.is_none());

    assert!(repo
        .finish(
            &source_id,
            ProcessingPhase::Search,
            ProcessingPhaseCompletion::new(
                1,
                &input,
                OWNER_A,
                running.attempt_id.as_deref().expect("interrupted attempt"),
                ProcessingPhaseTransition::ready("{}"),
            ),
        )
        .is_err());
    let retry = acquire(
        &repo,
        &source_id,
        ProcessingPhase::Search,
        1,
        &input,
        OWNER_B,
    );
    assert_ne!(retry.attempt_id, running.attempt_id);
}

#[test]
fn recovery_preserves_unexpired_running_phase() {
    let conn = setup_case_db();
    let repo = DataSourceProcessingPhaseRepo::new(&conn);
    let source_id = DataSourceId(DERIVED_SOURCE_ID.to_string());
    let input = fingerprint('7');
    acquire(
        &repo,
        &source_id,
        ProcessingPhase::Search,
        1,
        &input,
        OWNER_A,
    );

    let recovered = repo
        .recover_interrupted("Interrupted: application exited unexpectedly")
        .expect("recover interrupted phase");

    assert_eq!(recovered, 0);
    let running = repo
        .find(&source_id, ProcessingPhase::Search)
        .expect("query running phase")
        .expect("running phase exists");
    assert_eq!(running.state, ProcessingPhaseState::Running);
    assert!(running.lease_expires_at.is_some());
}

#[test]
fn failed_phase_retries_and_identity_change_resets_ready_work() {
    let conn = setup_case_db();
    let repo = DataSourceProcessingPhaseRepo::new(&conn);
    let source_id = DataSourceId(DERIVED_SOURCE_ID.to_string());
    let original = fingerprint('d');
    let changed = fingerprint('e');
    let first = acquire(
        &repo,
        &source_id,
        ProcessingPhase::Artifacts,
        1,
        &original,
        OWNER_A,
    );
    let failed = repo
        .finish(
            &source_id,
            ProcessingPhase::Artifacts,
            ProcessingPhaseCompletion::new(
                1,
                &original,
                OWNER_A,
                first.attempt_id.as_deref().expect("attempt"),
                ProcessingPhaseTransition::failed(r#"{"parsed":3}"#, "parser stopped"),
            ),
        )
        .expect("fail phase");
    assert_eq!(failed.state, ProcessingPhaseState::Failed);

    let retry = acquire(
        &repo,
        &source_id,
        ProcessingPhase::Artifacts,
        1,
        &original,
        OWNER_A,
    );
    let ready = repo
        .finish(
            &source_id,
            ProcessingPhase::Artifacts,
            ProcessingPhaseCompletion::new(
                1,
                &original,
                OWNER_A,
                retry.attempt_id.as_deref().expect("retry attempt"),
                ProcessingPhaseTransition::ready(r#"{"parsed":4}"#),
            ),
        )
        .expect("finish retry");
    assert_eq!(ready.state, ProcessingPhaseState::Ready);

    let reset = repo
        .upsert(&source_id, ProcessingPhase::Artifacts, 2, &changed)
        .expect("reset changed identity");
    assert_eq!(reset.state, ProcessingPhaseState::Pending);
    assert!(reset.owner_id.is_none());
    assert!(reset.attempt_id.is_none());
}

#[test]
fn active_phase_identity_cannot_be_replaced_before_lease_expiry() {
    let conn = setup_case_db();
    let repo = DataSourceProcessingPhaseRepo::new(&conn);
    let source_id = DataSourceId(DERIVED_SOURCE_ID.to_string());
    let original = fingerprint('1');
    let changed = fingerprint('2');
    acquire(
        &repo,
        &source_id,
        ProcessingPhase::Graph,
        1,
        &original,
        OWNER_A,
    );

    let error = repo
        .upsert(&source_id, ProcessingPhase::Graph, 2, &changed)
        .expect_err("active phase identity must remain fenced");

    assert!(error
        .to_string()
        .contains("identity cannot change while its lease is active"));
    let current = repo
        .find(&source_id, ProcessingPhase::Graph)
        .expect("query current phase")
        .expect("current phase exists");
    assert_eq!(current.state, ProcessingPhaseState::Running);
    assert_eq!(current.input_fingerprint, original);
}

#[test]
fn repository_rejects_invalid_sources_payloads_and_stale_attempts() {
    let conn = setup_case_db();
    let repo = DataSourceProcessingPhaseRepo::new(&conn);
    let source_id = DataSourceId(DERIVED_SOURCE_ID.to_string());
    let ordinary_id = DataSourceId(ORDINARY_SOURCE_ID.to_string());
    let input = fingerprint('f');

    assert!(repo
        .claim(&ordinary_id, ProcessingPhase::Artifacts, 1, &input, OWNER_A)
        .is_err());
    assert!(repo
        .claim(&source_id, ProcessingPhase::Artifacts, 0, &input, OWNER_A)
        .is_err());
    assert!(repo
        .claim(
            &source_id,
            ProcessingPhase::Artifacts,
            1,
            &input.to_uppercase(),
            OWNER_A
        )
        .is_err());

    let running = acquire(
        &repo,
        &source_id,
        ProcessingPhase::Artifacts,
        1,
        &input,
        OWNER_A,
    );
    let attempt = running.attempt_id.as_deref().expect("attempt");
    assert!(repo
        .finish(
            &source_id,
            ProcessingPhase::Artifacts,
            ProcessingPhaseCompletion::new(
                1,
                &input,
                OWNER_A,
                attempt,
                ProcessingPhaseTransition {
                    state: ProcessingPhaseState::Failed,
                    stats_json: "{}",
                    last_error: None,
                },
            ),
        )
        .is_err());
    assert!(repo
        .finish(
            &source_id,
            ProcessingPhase::Artifacts,
            ProcessingPhaseCompletion::new(
                1,
                &input,
                OWNER_A,
                "wrong-attempt",
                ProcessingPhaseTransition::ready("{}"),
            ),
        )
        .is_err());
    assert!(repo
        .heartbeat(
            &source_id,
            ProcessingPhase::Artifacts,
            1,
            &input,
            OWNER_B,
            attempt,
        )
        .is_err());
}
