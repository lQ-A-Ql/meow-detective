use super::*;

fn setup_db() -> rusqlite::Connection {
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
        );",
    )
    .unwrap();
    conn
}

fn make_case(id: &str, name: &str) -> CaseMeta {
    CaseMeta {
        id: CaseId(id.to_string()),
        name: name.to_string(),
        number: Some("2025-001".to_string()),
        examiner: Some("Tester".to_string()),
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn create_then_find_by_id_returns_it() {
    let conn = setup_db();
    let repo = CaseRepo::new(&conn);
    let case = make_case("c1", "Test Case");
    repo.create(&case).unwrap();

    let found = repo.find_by_id(&CaseId("c1".to_string())).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "Test Case");
    assert_eq!(found.number, Some("2025-001".to_string()));
}

#[test]
fn find_by_id_nonexistent_returns_none() {
    let conn = setup_db();
    let repo = CaseRepo::new(&conn);

    let found = repo.find_by_id(&CaseId("nope".to_string())).unwrap();
    assert!(found.is_none());
}

#[test]
fn list_all_returns_all_cases() {
    let conn = setup_db();
    let repo = CaseRepo::new(&conn);
    repo.create(&make_case("c1", "Case A")).unwrap();
    repo.create(&make_case("c2", "Case B")).unwrap();

    let cases = repo.list_all().unwrap();
    assert_eq!(cases.len(), 2);
}

#[test]
fn delete_removes_the_case() {
    let conn = setup_db();
    let repo = CaseRepo::new(&conn);
    repo.create(&make_case("c1", "Case A")).unwrap();

    repo.delete(&CaseId("c1".to_string())).unwrap();

    let found = repo.find_by_id(&CaseId("c1".to_string())).unwrap();
    assert!(found.is_none());
}
