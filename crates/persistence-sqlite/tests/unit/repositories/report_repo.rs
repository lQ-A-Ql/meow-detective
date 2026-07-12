use super::*;

fn setup_db() -> Connection {
    let conn = crate::connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE cases (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            number TEXT,
            examiner TEXT,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE reports (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id),
            template_id TEXT NOT NULL,
            file_name TEXT NOT NULL,
            created_by TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'running',
            progress INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    conn
}

fn insert_case(conn: &Connection, case_id: &str) {
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?2, datetime('now'), datetime('now'))",
        params![case_id, "Test Case"],
    ).unwrap();
}

fn make_report(id: &str, case_id: &str, status: &str) -> ReportRecord {
    ReportRecord {
        id: id.to_string(),
        case_id: case_id.to_string(),
        template_id: "tpl-1".to_string(),
        file_name: "report.html".to_string(),
        created_by: "tester".to_string(),
        status: status.to_string(),
        progress: Some(0),
        created_at: "2025-01-01T00:00:00Z".to_string(),
    }
}

#[test]
fn insert_then_list_by_case_returns_record() {
    let conn = setup_db();
    insert_case(&conn, "case-1");
    let repo = ReportRepo::new(&conn);
    repo.insert(&make_report("r1", "case-1", "running"))
        .unwrap();

    let results = repo.list_by_case("case-1").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "r1");
    assert_eq!(results[0].status, "running");
}

#[test]
fn update_status_changes_field() {
    let conn = setup_db();
    insert_case(&conn, "case-1");
    let repo = ReportRepo::new(&conn);
    repo.insert(&make_report("r1", "case-1", "running"))
        .unwrap();

    repo.update_status("r1", "completed", Some(100)).unwrap();

    let results = repo.list_by_case("case-1").unwrap();
    assert_eq!(results[0].status, "completed");
    assert_eq!(results[0].progress, Some(100));
}

#[test]
fn list_by_case_wrong_id_returns_empty() {
    let conn = setup_db();
    insert_case(&conn, "case-1");
    let repo = ReportRepo::new(&conn);
    repo.insert(&make_report("r1", "case-1", "running"))
        .unwrap();

    let results = repo.list_by_case("case-999").unwrap();
    assert!(results.is_empty());
}
