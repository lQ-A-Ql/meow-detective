use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Barrier},
};

use domain::structure::{
    ActivityState, CancellationScope, CapabilityKind, ConflictReason, EvidenceRequirement,
    MetadataFailure, ResourceKind, TargetFailure,
};
use persistence_sqlite::{
    open_existing, open_in_memory, open_or_create,
    repositories::structure_activity_repo::{ActivityRecord, StructureActivityRepo},
    runner, DbError,
};
use rusqlite::{params, Connection};

fn setup() -> Connection {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    register_cases(&conn);
    conn
}

fn register_cases(conn: &Connection) {
    conn.execute_batch("INSERT INTO cases(id,name) VALUES('case-a','A'),('case-b','B');")
        .unwrap();
}

fn record() -> ActivityRecord {
    ActivityRecord {
        case_id: "case-a".into(),
        run_id: "run-a".into(),
        work_key: "stable-work".into(),
        attempt_id: "attempt-a".into(),
        owner_id: "owner-a".into(),
        definition_sha256: "a".repeat(64),
        input_capability_ids: BTreeSet::from(["capability-a@1".into(), "capability-b@2".into()]),
        target_id: "target-a".into(),
        state: ActivityState::Pending,
        output_receipt: None,
        failure: None,
    }
}

fn advance(repo: &StructureActivityRepo<'_>, record: &ActivityRecord, next: ActivityState) -> bool {
    change(repo, record, next, None, None).unwrap()
}

fn change(
    repo: &StructureActivityRepo<'_>,
    record: &ActivityRecord,
    next: ActivityState,
    receipt: Option<&str>,
    failure: Option<&TargetFailure>,
) -> persistence_sqlite::DbResult<bool> {
    repo.transition(
        &record.case_id,
        &record.run_id,
        &record.work_key,
        &record.attempt_id,
        &record.owner_id,
        record.state,
        next,
        receipt,
        failure,
    )
}

fn running(conn: &Connection) -> ActivityRecord {
    let repo = StructureActivityRepo::new(conn);
    let mut record = record();
    assert!(repo.insert_pending(&record).unwrap());
    for state in [
        ActivityState::Ready,
        ActivityState::Admitted,
        ActivityState::Running,
    ] {
        assert!(advance(&repo, &record, state));
        record.state = state;
    }
    record
}

fn disk_case() -> (tempfile::TempDir, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-workspaces/structure-activity-ledger");
    std::fs::create_dir_all(&root).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("case-")
        .tempdir_in(root)
        .unwrap();
    let path = directory.path().join("app.db");
    let conn = open_or_create(&path).unwrap();
    runner::run_all(&conn).unwrap();
    register_cases(&conn);
    (directory, path)
}

#[test]
fn migration_installs_only_control_fields_and_is_idempotent() {
    let conn = setup();
    assert_eq!(runner::run_all(&conn).unwrap(), 0);
    let columns = conn
        .prepare("SELECT name FROM pragma_table_info('structure_activities')")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        columns,
        [
            "case_id",
            "run_id",
            "work_key",
            "attempt_id",
            "owner_id",
            "definition_sha256",
            "input_capability_ids_json",
            "target_id",
            "state",
            "output_receipt",
            "failure_json",
        ]
    );
    let foreign_key: (String, String, String) = conn
        .query_row(
            "SELECT \"table\", \"from\", on_delete FROM pragma_foreign_key_list('structure_activities')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        foreign_key,
        ("cases".into(), "case_id".into(), "CASCADE".into())
    );
}

#[test]
fn pending_insert_and_lookup_preserve_the_complete_control_identity() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    let original = record();
    assert!(repo.insert_pending(&original).unwrap());
    assert_eq!(
        repo.get("case-a", "run-a", "stable-work").unwrap(),
        Some(original)
    );
    assert!(repo
        .get("case-a", "missing-run", "stable-work")
        .unwrap()
        .is_none());
    assert!(repo
        .get("case-a", "run-a", "missing-work")
        .unwrap()
        .is_none());
}

#[test]
fn identical_definitions_are_idempotent_without_reassigning_attempt_or_owner() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    let original = record();
    assert!(repo.insert_pending(&original).unwrap());
    assert!(!repo.insert_pending(&original).unwrap());
    let mut contender = original.clone();
    contender.attempt_id = "attempt-b".into();
    contender.owner_id = "owner-b".into();
    assert!(!repo.insert_pending(&contender).unwrap());
    assert_eq!(
        repo.get("case-a", "run-a", "stable-work").unwrap(),
        Some(original)
    );
}

#[test]
fn same_run_conflicting_definitions_never_overwrite_the_registered_work() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    let original = record();
    repo.insert_pending(&original).unwrap();
    let mut alternatives = vec![original.clone(); 3];
    alternatives[0].definition_sha256 = "b".repeat(64);
    alternatives[1].input_capability_ids = BTreeSet::from(["other-capability@1".into()]);
    alternatives[2].target_id = "target-b".into();
    for alternative in alternatives {
        assert!(matches!(
            repo.insert_pending(&alternative),
            Err(DbError::System(_))
        ));
        assert_eq!(
            repo.get("case-a", "run-a", "stable-work").unwrap(),
            Some(original.clone())
        );
    }
}

#[test]
fn cases_have_independent_activity_keys_and_transition_authority() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    let first = record();
    repo.insert_pending(&first).unwrap();
    assert!(repo
        .get("case-b", "run-a", "stable-work")
        .unwrap()
        .is_none());
    let wrong_case = ActivityRecord {
        case_id: "case-b".into(),
        ..first.clone()
    };
    assert!(!advance(&repo, &wrong_case, ActivityState::Ready));
    let mut second = first.clone();
    second.case_id = "case-b".into();
    second.attempt_id = "attempt-b".into();
    second.owner_id = "owner-b".into();
    assert!(repo.insert_pending(&second).unwrap());
    assert!(advance(&repo, &first, ActivityState::Ready));
    assert_eq!(
        repo.get("case-b", "run-a", "stable-work").unwrap(),
        Some(second)
    );
}

#[test]
fn unregistered_case_cannot_create_an_activity() {
    let conn = setup();
    let mut unknown = record();
    unknown.case_id = "missing-case".into();
    let error = StructureActivityRepo::new(&conn)
        .insert_pending(&unknown)
        .unwrap_err();
    assert!(matches!(
        error,
        DbError::Sqlite(rusqlite::Error::SqliteFailure(code, _))
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    ));
}

#[test]
fn transitions_compare_case_run_work_attempt_owner_and_expected_state() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    let original = record();
    repo.insert_pending(&original).unwrap();
    let identities = [
        ("case-b", "run-a", "stable-work", "attempt-a", "owner-a"),
        ("case-a", "other-run", "stable-work", "attempt-a", "owner-a"),
        ("case-a", "run-a", "other-work", "attempt-a", "owner-a"),
        ("case-a", "run-a", "stable-work", "other-attempt", "owner-a"),
        ("case-a", "run-a", "stable-work", "attempt-a", "other-owner"),
    ];
    for (case, run, key, attempt, owner) in identities {
        let wrong = ActivityRecord {
            case_id: case.into(),
            run_id: run.into(),
            work_key: key.into(),
            attempt_id: attempt.into(),
            owner_id: owner.into(),
            ..original.clone()
        };
        assert!(!advance(&repo, &wrong, ActivityState::Ready));
    }
    let wrong_state = ActivityRecord {
        state: ActivityState::Ready,
        ..original.clone()
    };
    assert!(!advance(&repo, &wrong_state, ActivityState::Admitted));
    assert!(advance(&repo, &original, ActivityState::Ready));
    assert!(!advance(&repo, &original, ActivityState::Ready));
}

#[test]
fn illegal_state_transitions_are_rejected_before_the_database_update() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    let original = record();
    repo.insert_pending(&original).unwrap();
    for state in [
        ActivityState::Pending,
        ActivityState::Admitted,
        ActivityState::Running,
    ] {
        assert!(change(&repo, &original, state, None, None).is_err());
    }
    assert_eq!(
        repo.get("case-a", "run-a", "stable-work").unwrap(),
        Some(original)
    );
}

#[test]
fn success_requires_a_receipt_and_preserves_terminal_state() {
    let conn = setup();
    let current = running(&conn);
    let repo = StructureActivityRepo::new(&conn);
    for (receipt, failure) in [
        (None, None),
        (Some(""), None),
        (Some("receipt-a"), Some(TargetFailure::IoUnavailable)),
    ] {
        assert!(change(
            &repo,
            &current,
            ActivityState::Succeeded,
            receipt,
            failure.as_ref()
        )
        .is_err());
    }
    assert!(change(
        &repo,
        &current,
        ActivityState::Succeeded,
        Some("receipt-a"),
        None
    )
    .unwrap());
    assert!(!change(
        &repo,
        &current,
        ActivityState::Succeeded,
        Some("receipt-b"),
        None
    )
    .unwrap());
    let completed = ActivityRecord {
        state: ActivityState::Succeeded,
        output_receipt: Some("receipt-a".into()),
        ..current.clone()
    };
    assert!(change(&repo, &completed, ActivityState::Ready, None, None).is_err());
    assert!(!repo.insert_pending(&record()).unwrap());
    let stored = repo
        .get(&current.case_id, &current.run_id, &current.work_key)
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, ActivityState::Succeeded);
    assert_eq!(stored.output_receipt.as_deref(), Some("receipt-a"));
    assert_eq!(stored.failure, None);
}

#[test]
fn typed_terminal_reasons_are_persisted_with_their_precise_state() {
    let outcomes = [
        (ActivityState::Failed, TargetFailure::IoUnavailable),
        (ActivityState::Failed, TargetFailure::DeadlineExceeded),
        (
            ActivityState::Failed,
            TargetFailure::CorruptMetadata {
                reason: MetadataFailure::InvalidChecksum,
            },
        ),
        (
            ActivityState::Failed,
            TargetFailure::UnsupportedFeature {
                capability: CapabilityKind::RuntimeState,
            },
        ),
        (
            ActivityState::Failed,
            TargetFailure::Conflicted {
                reason: ConflictReason::InputChanged,
            },
        ),
        (
            ActivityState::Failed,
            TargetFailure::ResourceLimit {
                resource: ResourceKind::Bytes,
                required: u64::MAX,
                limit: 1024,
            },
        ),
        (
            ActivityState::Blocked,
            TargetFailure::RequiredDependencyFailed,
        ),
        (
            ActivityState::Blocked,
            TargetFailure::MissingCapabilities {
                capabilities: BTreeSet::from([CapabilityKind::Namespace]),
            },
        ),
        (
            ActivityState::Blocked,
            TargetFailure::MissingEvidence {
                requirement: EvidenceRequirement::Member,
            },
        ),
        (
            ActivityState::Cancelled,
            TargetFailure::Cancelled {
                scope: CancellationScope::Run,
            },
        ),
    ];
    for (state, reason) in outcomes {
        let conn = setup();
        let current = running(&conn);
        let repo = StructureActivityRepo::new(&conn);
        assert!(change(&repo, &current, state, None, Some(&reason)).unwrap());
        let stored = repo.get("case-a", "run-a", "stable-work").unwrap().unwrap();
        assert_eq!(stored.state, state);
        assert_eq!(stored.failure, Some(reason));
        assert_eq!(stored.output_receipt, None);
    }
}

#[test]
fn failure_blocking_and_cancellation_reject_inaccurate_or_missing_reasons() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    repo.insert_pending(&record()).unwrap();
    let incorrect = [
        (ActivityState::Failed, None),
        (ActivityState::Blocked, None),
        (ActivityState::Cancelled, None),
        (
            ActivityState::Failed,
            Some(TargetFailure::Cancelled {
                scope: CancellationScope::Case,
            }),
        ),
        (
            ActivityState::Failed,
            Some(TargetFailure::RequiredDependencyFailed),
        ),
        (ActivityState::Blocked, Some(TargetFailure::IoUnavailable)),
        (
            ActivityState::Cancelled,
            Some(TargetFailure::DeadlineExceeded),
        ),
        (
            ActivityState::Blocked,
            Some(TargetFailure::MissingCapabilities {
                capabilities: BTreeSet::new(),
            }),
        ),
        (
            ActivityState::Failed,
            Some(TargetFailure::ResourceLimit {
                resource: ResourceKind::Cpu,
                required: 1,
                limit: 2,
            }),
        ),
    ];
    for (state, reason) in incorrect {
        assert!(change(&repo, &record(), state, None, reason.as_ref()).is_err());
    }
    assert_eq!(
        repo.get("case-a", "run-a", "stable-work").unwrap(),
        Some(record())
    );
}

#[test]
fn retry_uses_a_new_run_and_preserves_the_stable_work_key_and_prior_failure() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    let original = record();
    repo.insert_pending(&original).unwrap();
    assert!(change(
        &repo,
        &original,
        ActivityState::Failed,
        None,
        Some(&TargetFailure::IoUnavailable),
    )
    .unwrap());
    let retry = ActivityRecord {
        run_id: "run-b".into(),
        attempt_id: "attempt-b".into(),
        owner_id: "owner-b".into(),
        ..original.clone()
    };
    assert!(repo.insert_pending(&retry).unwrap());
    assert_eq!(
        repo.get("case-a", "run-b", "stable-work").unwrap(),
        Some(retry)
    );
    let first = repo.get("case-a", "run-a", "stable-work").unwrap().unwrap();
    assert_eq!(first.state, ActivityState::Failed);
    assert_eq!(first.failure, Some(TargetFailure::IoUnavailable));
    assert_eq!(first.attempt_id, original.attempt_id);
}

#[test]
fn caller_transaction_controls_insert_and_transition_commit_or_rollback() {
    let conn = setup();
    {
        let tx = conn.unchecked_transaction().unwrap();
        let repo = StructureActivityRepo::new(&tx);
        assert!(repo.insert_pending(&record()).unwrap());
        assert!(advance(&repo, &record(), ActivityState::Ready));
        tx.rollback().unwrap();
    }
    let repo = StructureActivityRepo::new(&conn);
    assert!(repo
        .get("case-a", "run-a", "stable-work")
        .unwrap()
        .is_none());
    {
        let tx = conn.unchecked_transaction().unwrap();
        let repo = StructureActivityRepo::new(&tx);
        assert!(repo.insert_pending(&record()).unwrap());
        assert!(advance(&repo, &record(), ActivityState::Ready));
        tx.commit().unwrap();
    }
    assert_eq!(
        repo.get("case-a", "run-a", "stable-work")
            .unwrap()
            .unwrap()
            .state,
        ActivityState::Ready
    );
}

#[test]
fn case_retirement_cascades_atomically_without_touching_other_cases() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    repo.insert_pending(&record()).unwrap();
    let other = ActivityRecord {
        case_id: "case-b".into(),
        ..record()
    };
    repo.insert_pending(&other).unwrap();
    {
        let tx = conn.unchecked_transaction().unwrap();
        tx.execute("DELETE FROM cases WHERE id='case-a'", [])
            .unwrap();
        assert!(StructureActivityRepo::new(&tx)
            .get("case-a", "run-a", "stable-work")
            .unwrap()
            .is_none());
        tx.rollback().unwrap();
    }
    assert_eq!(
        repo.get("case-a", "run-a", "stable-work").unwrap(),
        Some(record())
    );
    conn.execute("DELETE FROM cases WHERE id='case-a'", [])
        .unwrap();
    assert!(repo
        .get("case-a", "run-a", "stable-work")
        .unwrap()
        .is_none());
    assert_eq!(
        repo.get("case-b", "run-a", "stable-work").unwrap(),
        Some(other)
    );
    assert!(!advance(&repo, &record(), ActivityState::Ready));
}

#[test]
fn a_recreated_case_cannot_be_updated_with_the_retired_attempt() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    repo.insert_pending(&record()).unwrap();
    conn.execute("DELETE FROM cases WHERE id='case-a'", [])
        .unwrap();
    conn.execute(
        "INSERT INTO cases(id,name) VALUES('case-a','Recreated')",
        [],
    )
    .unwrap();
    let replacement = ActivityRecord {
        attempt_id: "replacement-attempt".into(),
        owner_id: "replacement-owner".into(),
        ..record()
    };
    repo.insert_pending(&replacement).unwrap();
    assert!(!advance(&repo, &record(), ActivityState::Ready));
    assert!(advance(&repo, &replacement, ActivityState::Ready));
}

#[test]
fn persisted_running_activity_can_be_recovered_with_its_original_attempt() {
    let (_directory, path) = disk_case();
    {
        let conn = open_existing(&path).unwrap();
        running(&conn);
    }
    {
        let conn = open_existing(&path).unwrap();
        let repo = StructureActivityRepo::new(&conn);
        let recovered = repo.get("case-a", "run-a", "stable-work").unwrap().unwrap();
        assert_eq!(recovered.state, ActivityState::Running);
        assert_eq!(recovered.attempt_id, "attempt-a");
        assert!(change(
            &repo,
            &recovered,
            ActivityState::Succeeded,
            Some("published-receipt"),
            None,
        )
        .unwrap());
    }
    let conn = open_existing(&path).unwrap();
    let repo = StructureActivityRepo::new(&conn);
    let recovered = repo.get("case-a", "run-a", "stable-work").unwrap().unwrap();
    assert_eq!(recovered.state, ActivityState::Succeeded);
    assert_eq!(
        recovered.output_receipt.as_deref(),
        Some("published-receipt")
    );
    assert!(!repo.insert_pending(&record()).unwrap());
}

#[test]
fn concurrent_insert_owners_have_one_winner_without_overwriting_the_winner() {
    let (_directory, path) = disk_case();
    let connections = [open_existing(&path).unwrap(), open_existing(&path).unwrap()];
    let barrier = Arc::new(Barrier::new(2));
    let outcomes = std::thread::scope(|scope| {
        let handles = connections
            .into_iter()
            .enumerate()
            .map(|(index, conn)| {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    let candidate = ActivityRecord {
                        owner_id: format!("owner-{index}"),
                        attempt_id: format!("attempt-{index}"),
                        ..record()
                    };
                    barrier.wait();
                    let inserted = StructureActivityRepo::new(&conn)
                        .insert_pending(&candidate)
                        .unwrap();
                    (inserted, candidate)
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|(inserted, _)| *inserted).count(), 1);
    let winner = &outcomes.iter().find(|(inserted, _)| *inserted).unwrap().1;
    let conn = open_existing(&path).unwrap();
    let stored = StructureActivityRepo::new(&conn)
        .get("case-a", "run-a", "stable-work")
        .unwrap();
    assert_eq!(stored.as_ref(), Some(winner));
}

#[test]
fn two_real_connections_cannot_both_win_the_same_state_transition() {
    let (_directory, path) = disk_case();
    let conn = open_existing(&path).unwrap();
    StructureActivityRepo::new(&conn)
        .insert_pending(&record())
        .unwrap();
    let connections = [conn, open_existing(&path).unwrap()];
    let barrier = Arc::new(Barrier::new(2));
    let outcomes = std::thread::scope(|scope| {
        let handles = connections
            .into_iter()
            .map(|conn| {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    barrier.wait();
                    advance(
                        &StructureActivityRepo::new(&conn),
                        &record(),
                        ActivityState::Ready,
                    )
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.into_iter().filter(|won| *won).count(), 1);
}

#[test]
fn serialized_records_validate_identity_outcomes_and_unknown_fields() {
    let original = record();
    let encoded = serde_json::to_string(&original).unwrap();
    assert_eq!(
        serde_json::from_str::<ActivityRecord>(&encoded).unwrap(),
        original
    );
    for field in [
        "caseId",
        "runId",
        "workKey",
        "attemptId",
        "ownerId",
        "targetId",
        "definitionSha256",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value[field] = serde_json::json!("");
        assert!(serde_json::from_value::<ActivityRecord>(value).is_err());
    }
    for value in [
        serde_json::json!({"state": "succeeded"}),
        serde_json::json!({"state": "failed"}),
        serde_json::json!({"inputCapabilityIds": []}),
        serde_json::json!({"inputCapabilityIds": [""]}),
        serde_json::json!({"unknownField": true}),
    ] {
        let mut serialized = serde_json::to_value(&original).unwrap();
        serialized
            .as_object_mut()
            .unwrap()
            .extend(value.as_object().unwrap().clone());
        assert!(serde_json::from_value::<ActivityRecord>(serialized).is_err());
    }
}

#[test]
fn database_rejects_definition_rebinding_illegal_states_and_untyped_terminal_results() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    repo.insert_pending(&record()).unwrap();
    for statement in [
        "UPDATE structure_activities SET owner_id='other-owner'",
        "UPDATE structure_activities SET attempt_id='other-attempt'",
        "UPDATE structure_activities SET definition_sha256=printf('%064d',0)",
        "UPDATE structure_activities SET input_capability_ids_json='[\"different\"]'",
        "UPDATE structure_activities SET target_id='different-target'",
        "UPDATE structure_activities SET state='running'",
        "UPDATE structure_activities SET state='succeeded',output_receipt='receipt'",
        "UPDATE structure_activities SET state='cancelled',failure_json='{}'",
        "UPDATE structure_activities SET state='blocked',failure_json='{\"kind\":\"io_unavailable\"}'",
    ] {
        assert!(conn.execute(statement, []).is_err());
    }
    assert_eq!(
        repo.get("case-a", "run-a", "stable-work").unwrap(),
        Some(record())
    );
}

#[test]
fn database_rejects_invalid_input_reference_arrays() {
    let conn = setup();
    for inputs in ["[]", "[1]", "[\"\"]", "[\"a\",\"a\"]", "{}", "invalid-json"] {
        assert!(conn.execute(
            "INSERT INTO structure_activities (
                case_id,run_id,work_key,attempt_id,owner_id,definition_sha256,
                input_capability_ids_json,target_id,state
             ) VALUES('case-a','run-a','stable-work','attempt-a','owner-a',?1,?2,'target-a','pending')",
            params!["a".repeat(64), inputs],
        ).is_err());
    }
}

#[test]
fn every_terminal_state_refuses_reexecution_and_mutation() {
    for (state, receipt, reason) in [
        (ActivityState::Succeeded, Some("receipt"), None),
        (
            ActivityState::Failed,
            None,
            Some(TargetFailure::IoUnavailable),
        ),
        (
            ActivityState::Blocked,
            None,
            Some(TargetFailure::RequiredDependencyFailed),
        ),
        (
            ActivityState::Cancelled,
            None,
            Some(TargetFailure::Cancelled {
                scope: CancellationScope::Source,
            }),
        ),
    ] {
        let conn = setup();
        let current = running(&conn);
        let repo = StructureActivityRepo::new(&conn);
        assert!(change(&repo, &current, state, receipt, reason.as_ref()).unwrap());
        let completed = repo.get("case-a", "run-a", "stable-work").unwrap().unwrap();
        assert!(!repo.insert_pending(&record()).unwrap());
        assert!(!change(&repo, &current, state, receipt, reason.as_ref()).unwrap());
        assert!(change(&repo, &completed, ActivityState::Ready, None, None).is_err());
        assert!(conn.execute(
            "UPDATE structure_activities SET state='ready',output_receipt=NULL,failure_json=NULL",
            [],
        ).is_err());
        assert_eq!(
            repo.get("case-a", "run-a", "stable-work").unwrap(),
            Some(completed)
        );
    }
}

#[test]
fn input_reference_sets_are_canonical_and_pending_insertion_rejects_results() {
    let conn = setup();
    let repo = StructureActivityRepo::new(&conn);
    let original = record();
    assert!(repo.insert_pending(&original).unwrap());
    let mut value = serde_json::to_value(&original).unwrap();
    value["inputCapabilityIds"] =
        serde_json::json!(["capability-b@2", "capability-a@1", "capability-b@2"]);
    let reordered: ActivityRecord = serde_json::from_value(value).unwrap();
    assert_eq!(reordered, original);
    assert!(!repo.insert_pending(&reordered).unwrap());
    let ready = ActivityRecord {
        state: ActivityState::Ready,
        ..original
    };
    ready.validate().unwrap();
    assert!(repo.insert_pending(&ready).is_err());
}

#[test]
fn cancellation_retires_the_owner_attempt_before_a_new_run_retries() {
    let (_directory, path) = disk_case();
    let current = {
        let conn = open_existing(&path).unwrap();
        running(&conn)
    };
    let conn = open_existing(&path).unwrap();
    let repo = StructureActivityRepo::new(&conn);
    let reason = TargetFailure::Cancelled {
        scope: CancellationScope::Run,
    };
    assert!(change(
        &repo,
        &current,
        ActivityState::Cancelled,
        None,
        Some(&reason)
    )
    .unwrap());
    assert!(!change(
        &repo,
        &current,
        ActivityState::Succeeded,
        Some("late-receipt"),
        None,
    )
    .unwrap());
    let retry = ActivityRecord {
        run_id: "retry-run".into(),
        attempt_id: "retry-attempt".into(),
        owner_id: "retry-owner".into(),
        ..record()
    };
    assert!(repo.insert_pending(&retry).unwrap());
    let cancelled = repo.get("case-a", "run-a", "stable-work").unwrap().unwrap();
    assert_eq!(cancelled.state, ActivityState::Cancelled);
    assert_eq!(cancelled.failure, Some(reason));
}
