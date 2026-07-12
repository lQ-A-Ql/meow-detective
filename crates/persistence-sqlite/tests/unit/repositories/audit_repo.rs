use super::*;
use crate::connection::open_in_memory;

fn setup_conn() -> Connection {
    let conn = open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE audit_log (
            id TEXT PRIMARY KEY,
            case_id TEXT,
            user_id TEXT,
            action TEXT,
            resource_type TEXT,
            resource_id TEXT,
            details TEXT,
            ip_address TEXT,
            created_at TEXT DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    conn
}

#[test]
fn log_and_query_roundtrip() {
    let conn = setup_conn();
    let repo = AuditRepo::new(&conn);

    repo.log(
        Some("case-1"),
        "user-1",
        &AuditAction::CaseCreate,
        Some("case-1"),
        r#"{"name":"Test Case"}"#,
    )
    .unwrap();

    let entries = repo.query(None, None, 10, 0).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action, "case.create");
    assert_eq!(entries[0].case_id.as_deref(), Some("case-1"));
}

#[test]
fn count_filters_by_case() {
    let conn = setup_conn();
    let repo = AuditRepo::new(&conn);

    repo.log(
        Some("case-1"),
        "system",
        &AuditAction::CaseCreate,
        Some("case-1"),
        "{}",
    )
    .unwrap();
    repo.log(
        Some("case-1"),
        "system",
        &AuditAction::CaseOpen,
        Some("case-1"),
        "{}",
    )
    .unwrap();

    assert_eq!(repo.count(None).unwrap(), 2);
    assert_eq!(repo.count(Some("case-1")).unwrap(), 2);
    assert_eq!(repo.count(Some("case-2")).unwrap(), 0);
}

#[test]
fn query_by_action_filters_entries() {
    let conn = setup_conn();
    let repo = AuditRepo::new(&conn);

    repo.log_simple(None, &AuditAction::CaseCreate, Some("case-1"))
        .unwrap();
    repo.log_simple(None, &AuditAction::CaseOpen, Some("case-1"))
        .unwrap();
    repo.log_simple(None, &AuditAction::McpConnect, Some("srv-1"))
        .unwrap();

    let entries = repo.query(None, Some("case.create"), 10, 0).unwrap();
    assert_eq!(entries.len(), 1);

    let mcp_entries = repo.query(None, Some("mcp.connect"), 10, 0).unwrap();
    assert_eq!(mcp_entries.len(), 1);
    assert_eq!(mcp_entries[0].resource_type, "mcp");
}
