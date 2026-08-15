use domain::{CaseId, DataSourceId};
use persistence_sqlite::{
    open_in_memory,
    repositories::bitlocker_restore_intent_repo::{
        BitLockerRestoreIntentRepo, BitLockerRestoreStatus,
    },
    runner,
};

const CASE_ID: &str = "case-bitlocker";
const OTHER_CASE_ID: &str = "case-other";
const SOURCE_ID: &str = "source-bitlocker";
const OTHER_SOURCE_ID: &str = "source-other";
const FINGERPRINT: &str = "0123456789abcdef0123456789abcdef";

fn setup_case_db() -> rusqlite::Connection {
    let conn = open_in_memory().expect("open case database");
    runner::run_all(&conn).expect("run case migrations");
    conn.execute(
        "INSERT INTO cases (id, name) VALUES (?1, 'BitLocker case'), (?2, 'Other case')",
        [CASE_ID, OTHER_CASE_ID],
    )
    .expect("insert cases");
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path)
         VALUES (?1, ?2, 'Encrypted source', 'e01', ''),
                (?3, ?4, 'Other source', 'e01', '')",
        [SOURCE_ID, CASE_ID, OTHER_SOURCE_ID, OTHER_CASE_ID],
    )
    .expect("insert sources");
    conn
}

#[test]
fn restore_intent_is_case_scoped_without_storing_key_material() {
    let conn = setup_case_db();
    assert_eq!(runner::latest_version(), "0046_file_entry_unix_mode");
    let repo = BitLockerRestoreIntentRepo::new(&conn);
    let source_id = DataSourceId(SOURCE_ID.to_string());
    let other_source_id = DataSourceId(OTHER_SOURCE_ID.to_string());

    repo.upsert_enabled(&source_id, 2, FINGERPRINT)
        .expect("persist restore intent");
    repo.upsert_enabled(&other_source_id, 2, "fedcba9876543210fedcba9876543210")
        .expect("persist other case intent");

    let intents = repo
        .list_enabled_for_case(&CaseId(CASE_ID.to_string()))
        .expect("list case intents");
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].data_source_id, source_id);
    assert_eq!(intents[0].partition_index, 2);
    assert_eq!(intents[0].metadata_fingerprint, FINGERPRINT);
    assert_eq!(
        intents[0].last_restore_status,
        BitLockerRestoreStatus::Pending
    );
    assert!(intents[0].last_error_code.is_none());

    let columns = conn
        .prepare("SELECT name FROM pragma_table_info('bitlocker_restore_intents')")
        .expect("prepare column query")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect columns");
    assert_eq!(
        columns,
        [
            "data_source_id",
            "partition_index",
            "metadata_fingerprint",
            "enabled",
            "last_restore_status",
            "last_error_code",
            "updated_at",
        ]
    );
}

#[test]
fn status_updates_are_sanitized_and_forget_removes_the_intent() {
    let conn = setup_case_db();
    let repo = BitLockerRestoreIntentRepo::new(&conn);
    let source_id = DataSourceId(SOURCE_ID.to_string());
    repo.upsert_enabled(&source_id, 7, FINGERPRINT)
        .expect("persist restore intent");

    repo.mark_status(
        &source_id,
        7,
        BitLockerRestoreStatus::Failed,
        Some("BITLOCKER_KEY_STORE_FAILED"),
    )
    .expect("record retryable failure");
    let intent = repo
        .list_enabled_for_case(&CaseId(CASE_ID.to_string()))
        .expect("list intent")
        .pop()
        .expect("intent exists");
    assert_eq!(intent.last_restore_status, BitLockerRestoreStatus::Failed);
    assert_eq!(
        intent.last_error_code.as_deref(),
        Some("BITLOCKER_KEY_STORE_FAILED")
    );
    assert!(repo
        .mark_status(
            &source_id,
            7,
            BitLockerRestoreStatus::Failed,
            Some("raw credential text"),
        )
        .is_err());

    assert!(repo.remove(&source_id, 7).expect("remove restore intent"));
    assert!(repo
        .list_enabled_for_case(&CaseId(CASE_ID.to_string()))
        .expect("list after forget")
        .is_empty());
}

#[test]
fn intent_requires_a_known_source_and_is_removed_with_it() {
    let conn = setup_case_db();
    let repo = BitLockerRestoreIntentRepo::new(&conn);
    let source_id = DataSourceId(SOURCE_ID.to_string());

    assert!(repo
        .upsert_enabled(&DataSourceId("missing".to_string()), 0, FINGERPRINT)
        .is_err());
    assert!(repo
        .upsert_enabled(&source_id, 0, "upperCASE0123456789abcdef01234567")
        .is_err());
    repo.upsert_enabled(&source_id, 0, FINGERPRINT)
        .expect("persist restore intent");
    conn.execute("DELETE FROM data_sources WHERE id = ?1", [SOURCE_ID])
        .expect("delete source");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM bitlocker_restore_intents",
            [],
            |row| row.get(0),
        )
        .expect("count intents");
    assert_eq!(count, 0);
}
