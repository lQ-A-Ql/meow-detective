use super::*;
use crate::connection::open_in_memory;
use crate::migrations::runner;
use crate::repositories::audit_repo::{AuditAction, AuditRepo};

fn setup_conn() -> rusqlite::Connection {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn
}

#[test]
fn audit_events_append_to_a_case_chain_and_verify() {
    let conn = setup_conn();
    let audit = AuditRepo::new(&conn);

    audit
        .log_simple(Some("case-1"), &AuditAction::CaseCreate, Some("case-1"))
        .unwrap();
    audit
        .log_simple(Some("case-1"), &AuditAction::CaseOpen, Some("case-1"))
        .unwrap();

    let verification = LedgerRepo::new(&conn).verify(Some("case-1")).unwrap();
    assert!(verification.valid, "verification failed: {verification:?}");
    assert_eq!(verification.entry_count, 2);
    assert!(verification.head_hash.is_some());
    assert_eq!(AuditRepo::new(&conn).count(Some("case-1")).unwrap(), 2);
}

#[test]
fn ledger_verification_detects_payload_tampering() {
    let conn = setup_conn();
    AuditRepo::new(&conn)
        .log_simple(Some("case-1"), &AuditAction::CaseCreate, Some("case-1"))
        .unwrap();

    conn.execute(
        "UPDATE forensic_ledger SET details = ?1 WHERE scope_key = ?2 AND sequence = 1",
        rusqlite::params![r#"{"tampered":true}"#, "case-1"],
    )
    .unwrap();

    let verification = LedgerRepo::new(&conn).verify(Some("case-1")).unwrap();
    assert!(!verification.valid);
    assert_eq!(verification.entry_count, 0);
    assert!(verification
        .first_error
        .as_deref()
        .is_some_and(|error| error.contains("entry hash mismatch")));
}

#[test]
fn ledger_chains_are_isolated_by_case_and_global_scope() {
    let conn = setup_conn();
    let audit = AuditRepo::new(&conn);
    audit
        .log_simple(Some("case-1"), &AuditAction::CaseCreate, Some("case-1"))
        .unwrap();
    audit
        .log_simple(Some("case-2"), &AuditAction::CaseCreate, Some("case-2"))
        .unwrap();
    audit
        .log_simple(None, &AuditAction::McpConnect, Some("server-1"))
        .unwrap();

    assert_eq!(
        LedgerRepo::new(&conn)
            .verify(Some("case-1"))
            .unwrap()
            .entry_count,
        1
    );
    assert_eq!(
        LedgerRepo::new(&conn)
            .verify(Some("case-2"))
            .unwrap()
            .entry_count,
        1
    );
    assert_eq!(LedgerRepo::new(&conn).verify(None).unwrap().entry_count, 1);
}

#[test]
fn audit_and_ledger_write_roll_back_together() {
    let conn = setup_conn();
    conn.execute_batch(
        "CREATE TRIGGER reject_ledger_write
         BEFORE INSERT ON forensic_ledger
         BEGIN SELECT RAISE(ABORT, 'ledger unavailable'); END;",
    )
    .unwrap();

    let result =
        AuditRepo::new(&conn).log_simple(Some("case-1"), &AuditAction::CaseCreate, Some("case-1"));
    assert!(result.is_err());
    assert_eq!(AuditRepo::new(&conn).count(Some("case-1")).unwrap(), 0);
    let ledger_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM forensic_ledger WHERE scope_key = 'case-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(ledger_count, 0);
}
