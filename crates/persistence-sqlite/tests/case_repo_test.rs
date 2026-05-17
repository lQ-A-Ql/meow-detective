use chrono::Utc;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::{open_in_memory, repositories::case_repo::CaseRepo, runner};

struct TestCtx {
    conn: rusqlite::Connection,
}

impl TestCtx {
    fn repo(&self) -> CaseRepo<'_> {
        CaseRepo::new(&self.conn)
    }
}

fn setup() -> TestCtx {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    TestCtx { conn }
}

fn make_case(id: &str, name: &str) -> CaseMeta {
    CaseMeta {
        id: CaseId(id.to_string()),
        name: name.to_string(),
        number: Some(format!("CASE-{}", id)),
        examiner: Some("tester".to_string()),
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn create_and_find() {
    let ctx = setup();
    let case = make_case("case-001", "Test Case 1");
    ctx.repo().create(&case).unwrap();
    let found = ctx.repo().find_by_id(&case.id).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "Test Case 1");
    assert_eq!(found.examiner.unwrap(), "tester");
}

#[test]
fn update_fields() {
    let ctx = setup();
    let mut case = make_case("case-002", "Original Name");
    ctx.repo().create(&case).unwrap();

    case.name = "Updated Name".to_string();
    case.examiner = Some("new examiner".to_string());
    ctx.repo().update(&case).unwrap();

    let found = ctx.repo().find_by_id(&case.id).unwrap().unwrap();
    assert_eq!(found.name, "Updated Name");
    assert_eq!(found.examiner.unwrap(), "new examiner");
}

#[test]
fn list_all_cases() {
    let ctx = setup();
    ctx.repo().create(&make_case("case-003", "Alpha")).unwrap();
    ctx.repo().create(&make_case("case-004", "Beta")).unwrap();

    let cases = ctx.repo().list_all().unwrap();
    assert_eq!(cases.len(), 2);
    let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Alpha"));
    assert!(names.contains(&"Beta"));
}

#[test]
fn find_nonexistent_returns_none() {
    let ctx = setup();
    let result = ctx
        .repo()
        .find_by_id(&CaseId("does-not-exist".to_string()))
        .unwrap();
    assert!(result.is_none());
}

#[test]
fn delete_case() {
    let ctx = setup();
    let case = make_case("case-005", "Delete Me");
    ctx.repo().create(&case).unwrap();
    ctx.repo().delete(&case.id).unwrap();
    let result = ctx.repo().find_by_id(&case.id).unwrap();
    assert!(result.is_none());
}
