use std::{
    path::PathBuf,
    sync::{Arc, Barrier},
};

use persistence_sqlite::{
    open_existing, open_in_memory, open_or_create,
    repositories::structure_namespace_checkpoint_repo::{
        NamespaceCheckpointRecord, NamespacePageCommit, StructureNamespaceCheckpointRepo,
    },
    runner, DbError,
};
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};

fn setup() -> Connection {
    let connection = open_in_memory().unwrap();
    runner::run_all(&connection).unwrap();
    register_cases(&connection);
    connection
}

fn register_cases(connection: &Connection) {
    connection
        .execute_batch("INSERT INTO cases(id,name) VALUES('case-a','A'),('case-b','B');")
        .unwrap();
}

fn initial() -> NamespaceCheckpointRecord {
    NamespaceCheckpointRecord {
        case_id: "case-a".into(),
        run_id: "namespace-run".into(),
        target_id: "namespace-target".into(),
        snapshot_id: "immutable-snapshot-a".into(),
        query_sha256: digest(b"all children of fixed root"),
        ordering_id: "native-order-a".into(),
        attempt_id: "attempt-a".into(),
        owner_id: "owner-a".into(),
        next_page: 0,
        next_ordinal: 0,
        verified_objects: 0,
        next_cursor: None,
        eof: false,
        last_page_sha256: None,
        output_receipt: None,
    }
}

fn page(first: u64, count: u64, cursor: Option<&str>) -> NamespacePageCommit {
    NamespacePageCommit {
        first_ordinal: first,
        object_count: count,
        page_sha256: digest(&serde_json::to_vec(&(first, count, cursor)).unwrap()),
        next_cursor: cursor.map(str::to_owned),
        eof: cursor.is_none(),
        output_receipt: format!("sealed-page:{first}:{count}:{}", cursor.unwrap_or("eof")),
    }
}

fn get(repo: &StructureNamespaceCheckpointRepo<'_>) -> NamespaceCheckpointRecord {
    repo.get("case-a", "namespace-run", "namespace-target")
        .unwrap()
        .unwrap()
}

fn disk_case() -> (tempfile::TempDir, PathBuf) {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-workspaces/namespace-checkpoints");
    std::fs::create_dir_all(&directory).unwrap();
    let root = tempfile::tempdir_in(directory).unwrap();
    let database = root.path().join("app.db");
    let connection = open_or_create(&database).unwrap();
    runner::run_all(&connection).unwrap();
    register_cases(&connection);
    (root, database)
}

#[test]
fn migration_creates_only_last_page_control_fields_with_case_cascade() {
    let connection = setup();
    assert_eq!(runner::run_all(&connection).unwrap(), 0);
    let columns = connection
        .prepare("SELECT name FROM pragma_table_info('structure_namespace_checkpoints')")
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
            "target_id",
            "snapshot_id",
            "query_sha256",
            "ordering_id",
            "attempt_id",
            "owner_id",
            "next_page",
            "next_ordinal",
            "verified_objects",
            "next_cursor",
            "eof",
            "last_page_sha256",
            "output_receipt",
            "last_page_identity",
        ]
    );
    let foreign_key: (String, String, String) = connection
        .query_row(
            "SELECT \"table\", \"from\", on_delete
         FROM pragma_foreign_key_list('structure_namespace_checkpoints')",
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
fn initial_checkpoint_insert_and_idempotent_retry_preserve_initial_state() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    assert!(repo.insert_pending(&initial()).unwrap());
    assert!(!repo.insert_pending(&initial()).unwrap());
    assert_eq!(get(&repo), initial());
    assert!(repo
        .get("case-b", "namespace-run", "namespace-target")
        .unwrap()
        .is_none());
}

#[test]
fn checkpoint_creation_rejects_noninitial_counters_pages_results_and_eof() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    let mut alternatives = vec![initial(); 7];
    alternatives[0].next_page = 1;
    alternatives[1].next_ordinal = 1;
    alternatives[2].verified_objects = 1;
    alternatives[3].next_cursor = Some("unexpected-cursor".into());
    alternatives[4].eof = true;
    alternatives[5].last_page_sha256 = Some(digest(b"unexpected-page"));
    alternatives[6].output_receipt = Some("unexpected-receipt".into());
    for invalid in alternatives {
        assert!(repo.insert_pending(&invalid).is_err());
    }
    assert!(repo
        .get("case-a", "namespace-run", "namespace-target")
        .unwrap()
        .is_none());
}

#[test]
fn pages_advance_independent_page_and_object_ordinals_with_no_member_payloads() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    let first = page(0, 512, Some("after-first-page"));
    assert!(repo.commit_page(&initial(), &first).unwrap());
    let current = get(&repo);
    assert_eq!(current.next_page, 1);
    assert_eq!(current.next_ordinal, 512);
    assert_eq!(current.verified_objects, 512);
    assert_eq!(current.next_cursor.as_deref(), Some("after-first-page"));
    assert_eq!(current.last_page_sha256.as_ref(), Some(&first.page_sha256));
    assert_eq!(current.output_receipt.as_ref(), Some(&first.output_receipt));
    assert!(!current.eof);
    let second = page(512, 1, Some("after-second-page"));
    assert!(repo.commit_page(&current, &second).unwrap());
    let current = get(&repo);
    assert_eq!(current.next_page, 2);
    assert_eq!(current.next_ordinal, 513);
    assert_eq!(current.verified_objects, 513);
    assert_eq!(current.last_page_sha256.as_ref(), Some(&second.page_sha256));
    assert_eq!(
        current.output_receipt.as_ref(),
        Some(&second.output_receipt)
    );
}

#[test]
fn identical_page_retry_is_idempotent_and_never_counts_objects_twice() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    let start = initial();
    repo.insert_pending(&start).unwrap();
    let first = page(0, 7, Some("page-seven"));
    assert!(repo.commit_page(&start, &first).unwrap());
    let saved = get(&repo);
    assert!(!repo.commit_page(&start, &first).unwrap());
    assert_eq!(get(&repo), saved);
    assert!(!repo.insert_pending(&start).unwrap());
    assert_eq!(get(&repo), saved);
}

#[test]
fn replaying_the_same_page_number_with_different_parameters_is_rejected() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    let start = initial();
    repo.insert_pending(&start).unwrap();
    let first = page(0, 7, Some("after-seven"));
    repo.commit_page(&start, &first).unwrap();
    let mut alternatives = vec![first.clone(); 5];
    alternatives[0].object_count = 8;
    alternatives[1].page_sha256 = digest(b"different page bytes");
    alternatives[2].next_cursor = Some("different-cursor".into());
    alternatives[3].output_receipt = "different-output-receipt".into();
    alternatives[4].eof = true;
    alternatives[4].next_cursor = None;
    for altered in alternatives {
        assert!(matches!(
            repo.commit_page(&start, &altered),
            Err(DbError::System(_))
        ));
    }
    assert_eq!(get(&repo).verified_objects, 7);
}

#[test]
fn last_page_identity_rejects_different_first_and_count_with_the_same_end_ordinal() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    repo.commit_page(&initial(), &page(0, 5, Some("after-five")))
        .unwrap();
    let start = get(&repo);
    let committed = page(5, 7, Some("after-twelve"));
    repo.commit_page(&start, &committed).unwrap();
    let mut claimed_start = start;
    claimed_start.next_ordinal = 6;
    claimed_start.verified_objects = 6;
    claimed_start.validate().unwrap();
    let mut altered = committed;
    altered.first_ordinal = 6;
    altered.object_count = 6;
    altered.validate().unwrap();
    assert!(repo.commit_page(&claimed_start, &altered).is_err());
    assert_eq!(get(&repo).next_ordinal, 12);
}

#[test]
fn first_ordinal_must_match_the_expected_checkpoint_without_skips_or_overlap() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    repo.commit_page(&initial(), &page(0, 10, Some("after-ten")))
        .unwrap();
    let current = get(&repo);
    for first in [0, 9, 11] {
        assert!(repo.commit_page(&current, &page(first, 1, None)).is_err());
    }
    assert_eq!(get(&repo), current);
}

#[test]
fn empty_terminal_page_advances_page_number_without_changing_object_counts() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    let end = page(0, 0, None);
    assert!(repo.commit_page(&initial(), &end).unwrap());
    let saved = get(&repo);
    assert_eq!(saved.next_page, 1);
    assert_eq!(saved.next_ordinal, 0);
    assert_eq!(saved.verified_objects, 0);
    assert!(saved.eof);
    assert_eq!(saved.next_cursor, None);
    assert!(!repo.commit_page(&initial(), &end).unwrap());
    assert!(repo.commit_page(&saved, &end).is_err());
    assert_eq!(get(&repo), saved);
}

#[test]
fn nonempty_namespace_can_finish_with_an_empty_page_and_eof_cannot_restart() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    repo.commit_page(&initial(), &page(0, 7, Some("after-seven")))
        .unwrap();
    let before_eof = get(&repo);
    let terminal = page(7, 0, None);
    assert!(repo.commit_page(&before_eof, &terminal).unwrap());
    let completed = get(&repo);
    assert_eq!(completed.next_page, 2);
    assert_eq!(completed.next_ordinal, 7);
    assert!(completed.eof);
    assert!(!repo.commit_page(&before_eof, &terminal).unwrap());
    assert!(repo
        .commit_page(&completed, &page(7, 1, Some("restart")))
        .is_err());
    assert!(repo.commit_page(&completed, &page(7, 0, None)).is_err());
}

#[test]
fn nonterminal_pages_require_objects_and_nonempty_continuation_cursors() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    let mut cases = vec![page(0, 1, Some("cursor")); 6];
    cases[0].object_count = 0;
    cases[1].next_cursor = None;
    cases[2].next_cursor = Some(String::new());
    cases[3].next_cursor = Some(" ".into());
    cases[4].eof = true;
    cases[5].output_receipt.clear();
    for invalid in cases {
        assert!(repo.commit_page(&initial(), &invalid).is_err());
    }
    assert_eq!(get(&repo), initial());
}

#[test]
fn wrong_owner_or_attempt_cannot_commit_or_reassign_a_checkpoint() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    let start = initial();
    repo.insert_pending(&start).unwrap();
    let mut intruders = vec![start.clone(); 2];
    intruders[0].owner_id = "other-owner".into();
    intruders[1].attempt_id = "other-attempt".into();
    for intruder in intruders {
        assert!(!repo.insert_pending(&intruder).unwrap());
        assert!(!repo.commit_page(&intruder, &page(0, 1, None)).unwrap());
    }
    assert_eq!(get(&repo), start);
}

#[test]
fn snapshot_query_and_order_are_immutable_in_insertion_and_page_commits() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    let original = initial();
    repo.insert_pending(&original).unwrap();
    let mut alternatives = vec![original.clone(); 3];
    alternatives[0].snapshot_id = "another-snapshot".into();
    alternatives[1].query_sha256 = digest(b"another query");
    alternatives[2].ordering_id = "another-order".into();
    for changed in alternatives {
        assert!(repo.insert_pending(&changed).is_err());
        assert!(repo.commit_page(&changed, &page(0, 1, None)).is_err());
    }
    assert_eq!(get(&repo), original);
}

#[test]
fn same_target_names_in_other_cases_or_runs_have_independent_progress() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    let first = initial();
    let other_case = NamespaceCheckpointRecord {
        case_id: "case-b".into(),
        ..initial()
    };
    let other_run = NamespaceCheckpointRecord {
        run_id: "another-run".into(),
        ..initial()
    };
    for record in [&first, &other_case, &other_run] {
        assert!(repo.insert_pending(record).unwrap());
    }
    repo.commit_page(&first, &page(0, 7, None)).unwrap();
    assert_eq!(
        repo.get(
            &other_case.case_id,
            &other_case.run_id,
            &other_case.target_id
        )
        .unwrap(),
        Some(other_case)
    );
    assert_eq!(
        repo.get(&other_run.case_id, &other_run.run_id, &other_run.target_id)
            .unwrap(),
        Some(other_run)
    );
    assert_eq!(get(&repo).next_ordinal, 7);
}

#[test]
fn missing_checkpoint_returns_lost_cas_without_creating_a_row() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    assert!(!repo.commit_page(&initial(), &page(0, 1, None)).unwrap());
    assert!(repo
        .get("case-a", "namespace-run", "namespace-target")
        .unwrap()
        .is_none());
}

#[test]
fn exact_u64_counters_round_trip_in_sqlite_and_overflow_is_rejected() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    let first = page(0, u64::MAX - 1, Some("after-u64-minus-one"));
    repo.commit_page(&initial(), &first).unwrap();
    let current = get(&repo);
    assert_eq!(current.next_ordinal, u64::MAX - 1);
    assert!(repo
        .commit_page(&current, &page(u64::MAX - 1, 2, None))
        .is_err());
    assert_eq!(get(&repo), current);
    assert!(repo
        .commit_page(&current, &page(u64::MAX - 1, 1, None))
        .unwrap());
    let end = get(&repo);
    assert_eq!(end.next_ordinal, u64::MAX);
    assert_eq!(end.verified_objects, u64::MAX);
    let encoded = serde_json::to_string(&end).unwrap();
    assert!(encoded.contains("18446744073709551615"));
    assert_eq!(
        serde_json::from_str::<NamespaceCheckpointRecord>(&encoded).unwrap(),
        end
    );
    let (kind, size): (String, i64) = connection
        .query_row(
            "SELECT typeof(next_ordinal),length(next_ordinal) FROM structure_namespace_checkpoints",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((kind.as_str(), size), ("blob", 8));
}

#[test]
fn adjacent_u64_values_above_float_precision_are_not_merged() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    repo.commit_page(
        &initial(),
        &page(0, 9_007_199_254_740_992, Some("large-first")),
    )
    .unwrap();
    let current = get(&repo);
    repo.commit_page(&current, &page(current.next_ordinal, 1, None))
        .unwrap();
    assert_eq!(get(&repo).next_ordinal, 9_007_199_254_740_993);
}

#[test]
fn overflowing_page_number_is_rejected_without_mutating_the_current_row() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    let mathematical_boundary = NamespaceCheckpointRecord {
        next_page: u64::MAX,
        next_ordinal: u64::MAX,
        verified_objects: u64::MAX,
        next_cursor: Some("boundary-cursor".into()),
        last_page_sha256: Some(digest(b"boundary page")),
        output_receipt: Some("boundary-receipt".into()),
        ..initial()
    };
    mathematical_boundary.validate().unwrap();
    assert!(repo
        .commit_page(&mathematical_boundary, &page(u64::MAX, 0, None))
        .is_err());
    assert_eq!(get(&repo), initial());
}

#[test]
fn cancellation_before_commit_preserves_the_last_page_for_restart() {
    let (_root, path) = disk_case();
    {
        let connection = open_existing(&path).unwrap();
        let repo = StructureNamespaceCheckpointRepo::new(&connection);
        repo.insert_pending(&initial()).unwrap();
        repo.commit_page(&initial(), &page(0, 2, Some("after-two")))
            .unwrap();
        let tx = connection.unchecked_transaction().unwrap();
        let interrupted = StructureNamespaceCheckpointRepo::new(&tx);
        let expected = get(&interrupted);
        assert!(interrupted
            .commit_page(&expected, &page(2, 1, None))
            .unwrap());
        tx.rollback().unwrap();
    }
    let connection = open_existing(&path).unwrap();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    let resumed = get(&repo);
    assert_eq!(resumed.next_page, 1);
    assert_eq!(resumed.next_ordinal, 2);
    assert_eq!(resumed.next_cursor.as_deref(), Some("after-two"));
    assert!(!resumed.eof);
    assert!(repo.commit_page(&resumed, &page(2, 1, None)).unwrap());
    assert_eq!(get(&repo).verified_objects, 3);
}

#[test]
fn cancellation_after_commit_allows_identical_last_page_retry_after_reopen() {
    let (_root, path) = disk_case();
    let first = page(0, 2, Some("after-two"));
    {
        let connection = open_existing(&path).unwrap();
        let repo = StructureNamespaceCheckpointRepo::new(&connection);
        repo.insert_pending(&initial()).unwrap();
        repo.commit_page(&initial(), &first).unwrap();
    }
    let connection = open_existing(&path).unwrap();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    assert!(!repo.commit_page(&initial(), &first).unwrap());
    let resumed = get(&repo);
    assert_eq!(resumed.verified_objects, 2);
    assert_eq!(resumed.next_cursor.as_deref(), Some("after-two"));
}

#[test]
fn outer_transaction_owns_checkpoint_creation_and_page_commit() {
    let connection = setup();
    {
        let tx = connection.unchecked_transaction().unwrap();
        let repo = StructureNamespaceCheckpointRepo::new(&tx);
        repo.insert_pending(&initial()).unwrap();
        repo.commit_page(&initial(), &page(0, 2, None)).unwrap();
        tx.rollback().unwrap();
    }
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    assert!(repo
        .get("case-a", "namespace-run", "namespace-target")
        .unwrap()
        .is_none());
    {
        let tx = connection.unchecked_transaction().unwrap();
        let repo = StructureNamespaceCheckpointRepo::new(&tx);
        repo.insert_pending(&initial()).unwrap();
        repo.commit_page(&initial(), &page(0, 2, None)).unwrap();
        tx.commit().unwrap();
    }
    assert!(get(&repo).eof);
}

#[test]
fn two_real_connections_have_one_initial_owner_and_preserve_the_winner() {
    let (_root, path) = disk_case();
    let connections = [open_existing(&path).unwrap(), open_existing(&path).unwrap()];
    let gate = Arc::new(Barrier::new(2));
    let results = std::thread::scope(|workers| {
        let handles = connections
            .into_iter()
            .enumerate()
            .map(|(index, connection)| {
                let gate = Arc::clone(&gate);
                workers.spawn(move || {
                    let candidate = NamespaceCheckpointRecord {
                        owner_id: format!("owner-{index}"),
                        attempt_id: format!("attempt-{index}"),
                        ..initial()
                    };
                    gate.wait();
                    let won = StructureNamespaceCheckpointRepo::new(&connection)
                        .insert_pending(&candidate)
                        .unwrap();
                    (won, candidate)
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|(won, _)| *won).count(), 1);
    let winner = &results.iter().find(|(won, _)| *won).unwrap().1;
    let connection = open_existing(&path).unwrap();
    assert_eq!(
        &get(&StructureNamespaceCheckpointRepo::new(&connection)),
        winner
    );
}

#[test]
fn concurrent_identical_page_commits_increment_objects_once() {
    let (_root, path) = disk_case();
    let first = open_existing(&path).unwrap();
    StructureNamespaceCheckpointRepo::new(&first)
        .insert_pending(&initial())
        .unwrap();
    let connections = [first, open_existing(&path).unwrap()];
    let gate = Arc::new(Barrier::new(2));
    let results = std::thread::scope(|workers| {
        let handles = connections
            .into_iter()
            .map(|connection| {
                let gate = Arc::clone(&gate);
                workers.spawn(move || {
                    gate.wait();
                    StructureNamespaceCheckpointRepo::new(&connection)
                        .commit_page(&initial(), &page(0, 7, None))
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.into_iter().filter(|won| *won).count(), 1);
    let connection = open_existing(&path).unwrap();
    let current = get(&StructureNamespaceCheckpointRepo::new(&connection));
    assert_eq!(current.verified_objects, 7);
    assert_eq!(current.next_page, 1);
    assert!(current.eof);
}

#[test]
fn concurrent_different_page_payloads_have_one_committed_winner() {
    let (_root, path) = disk_case();
    let connection = open_existing(&path).unwrap();
    StructureNamespaceCheckpointRepo::new(&connection)
        .insert_pending(&initial())
        .unwrap();
    let connections = [connection, open_existing(&path).unwrap()];
    let gate = Arc::new(Barrier::new(2));
    let results = std::thread::scope(|workers| {
        let handles = connections
            .into_iter()
            .enumerate()
            .map(|(index, connection)| {
                let gate = Arc::clone(&gate);
                workers.spawn(move || {
                    let candidate = page(0, index as u64 + 1, None);
                    gate.wait();
                    let committed = StructureNamespaceCheckpointRepo::new(&connection)
                        .commit_page(&initial(), &candidate);
                    (committed, candidate)
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results
            .iter()
            .filter(|(outcome, _)| matches!(outcome, Ok(true)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|(outcome, _)| outcome.is_err())
            .count(),
        1
    );
    let winner = &results
        .iter()
        .find(|(outcome, _)| matches!(outcome, Ok(true)))
        .unwrap()
        .1;
    let connection = open_existing(&path).unwrap();
    let current = get(&StructureNamespaceCheckpointRepo::new(&connection));
    assert_eq!(current.verified_objects, winner.object_count);
    assert_eq!(current.last_page_sha256.as_ref(), Some(&winner.page_sha256));
    assert_eq!(
        current.output_receipt.as_ref(),
        Some(&winner.output_receipt)
    );
}

#[test]
fn case_retirement_cascades_only_its_checkpoints_and_can_be_rolled_back() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    let other = NamespaceCheckpointRecord {
        case_id: "case-b".into(),
        ..initial()
    };
    repo.insert_pending(&other).unwrap();
    {
        let tx = connection.unchecked_transaction().unwrap();
        tx.execute("DELETE FROM cases WHERE id='case-a'", [])
            .unwrap();
        assert!(StructureNamespaceCheckpointRepo::new(&tx)
            .get("case-a", "namespace-run", "namespace-target")
            .unwrap()
            .is_none());
        tx.rollback().unwrap();
    }
    assert_eq!(get(&repo), initial());
    connection
        .execute("DELETE FROM cases WHERE id='case-a'", [])
        .unwrap();
    assert!(repo
        .get("case-a", "namespace-run", "namespace-target")
        .unwrap()
        .is_none());
    assert_eq!(
        repo.get("case-b", "namespace-run", "namespace-target")
            .unwrap(),
        Some(other)
    );
}

#[test]
fn database_constraints_reject_rebinding_counter_regression_and_eof_restart() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    repo.insert_pending(&initial()).unwrap();
    for statement in [
        "UPDATE structure_namespace_checkpoints SET snapshot_id='changed'",
        "UPDATE structure_namespace_checkpoints SET ordering_id='changed'",
        "UPDATE structure_namespace_checkpoints SET query_sha256=printf('%064d',0)",
        "UPDATE structure_namespace_checkpoints SET owner_id='changed'",
        "UPDATE structure_namespace_checkpoints SET attempt_id='changed'",
        "UPDATE structure_namespace_checkpoints SET next_page=1",
    ] {
        assert!(connection.execute(statement, []).is_err());
    }
    repo.commit_page(&initial(), &page(0, 7, None)).unwrap();
    assert!(connection
        .execute(
            "UPDATE structure_namespace_checkpoints SET eof=0,next_cursor='restart',
            next_page=X'0000000000000002'",
            [],
        )
        .is_err());
    assert!(get(&repo).eof);
}

#[test]
fn checkpoint_records_and_page_requests_validate_on_deserialization() {
    for object in [
        serde_json::to_value(initial()).unwrap(),
        serde_json::to_value(page(0, 1, None)).unwrap(),
    ] {
        if object.get("snapshotId").is_some() {
            let decoded: NamespaceCheckpointRecord =
                serde_json::from_value(object.clone()).unwrap();
            assert_eq!(decoded, initial());
            let mut unknown = object;
            unknown["unknownField"] = json!(true);
            assert!(serde_json::from_value::<NamespaceCheckpointRecord>(unknown).is_err());
        } else {
            let decoded: NamespacePageCommit = serde_json::from_value(object.clone()).unwrap();
            assert_eq!(decoded, page(0, 1, None));
            let mut unknown = object;
            unknown["unknownField"] = json!(true);
            assert!(serde_json::from_value::<NamespacePageCommit>(unknown).is_err());
        }
    }
    for (field, value) in [
        ("querySha256", json!("A".repeat(64))),
        ("nextOrdinal", json!(1)),
        ("ownerId", json!("")),
        ("eof", json!(true)),
    ] {
        let mut value_record = serde_json::to_value(initial()).unwrap();
        value_record[field] = value;
        assert!(serde_json::from_value::<NamespaceCheckpointRecord>(value_record).is_err());
    }
}

#[test]
fn unregistered_case_checkpoint_cannot_be_created() {
    let connection = setup();
    let repo = StructureNamespaceCheckpointRepo::new(&connection);
    let missing = NamespaceCheckpointRecord {
        case_id: "missing-case".into(),
        ..initial()
    };
    assert!(matches!(
        repo.insert_pending(&missing),
        Err(DbError::Sqlite(rusqlite::Error::SqliteFailure(error, _)))
            if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    ));
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
