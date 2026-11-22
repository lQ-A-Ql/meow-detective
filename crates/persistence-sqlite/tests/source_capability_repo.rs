use persistence_sqlite::{
    open_in_memory,
    repositories::source_capability_repo::{SourceCapabilityRepo, SourceCapabilityRow},
    runner,
};

const CASE_ID: &str = "case-capability";
const OTHER_CASE_ID: &str = "case-other";
const SOURCE_ID: &str = "source-capability";
const OTHER_SOURCE_ID: &str = "source-other";

fn row(
    case_id: &str,
    source_id: &str,
    capability: &str,
    state: &str,
    derived: i64,
) -> SourceCapabilityRow {
    SourceCapabilityRow {
        case_id: case_id.to_string(),
        data_source_id: source_id.to_string(),
        capability: capability.to_string(),
        state: state.to_string(),
        revision: 0,
        progress_done: 0,
        progress_total: None,
        phase: None,
        detail: None,
        error_code: None,
        derived,
        updated_at: "2026-11-19T00:00:00Z".to_string(),
    }
}

fn setup_case_db() -> rusqlite::Connection {
    let conn = open_in_memory().expect("open case database");
    runner::run_all(&conn).expect("run case migrations");
    conn.execute(
        "INSERT INTO cases (id, name) VALUES (?1, 'Capability case'), (?2, 'Other case')",
        [CASE_ID, OTHER_CASE_ID],
    )
    .expect("insert cases");
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path)
         VALUES (?1, ?2, 'Capability source', 'e01', ''),
                (?3, ?4, 'Other source', 'e01', '')",
        [SOURCE_ID, CASE_ID, OTHER_SOURCE_ID, OTHER_CASE_ID],
    )
    .expect("insert sources");
    conn
}

#[test]
fn latest_migration_registers_source_capability_table() {
    let conn = setup_case_db();
    assert_eq!(runner::latest_version(), "0068_source_capability");
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='source_capability'")
        .expect("prepare table query")
        .query_map([], |row| row.get(0))
        .expect("query tables")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect tables");
    assert_eq!(tables, vec!["source_capability".to_string()]);
}

#[test]
fn upsert_inserts_and_updates_conflicting_key() {
    let conn = setup_case_db();
    let repo = SourceCapabilityRepo::new(&conn);

    repo.upsert(&row(CASE_ID, SOURCE_ID, "catalog", "pending", 1))
        .expect("insert catalog row");
    let mut updated = row(CASE_ID, SOURCE_ID, "catalog", "ready", 0);
    updated.revision = 2;
    updated.detail = Some("task confirmed".to_string());
    repo.upsert(&updated).expect("upsert catalog row");

    let stored = repo
        .get(CASE_ID, SOURCE_ID, "catalog")
        .expect("get row")
        .expect("row exists");
    assert_eq!(stored.state, "ready");
    assert_eq!(stored.revision, 2);
    assert_eq!(stored.detail.as_deref(), Some("task confirmed"));
    assert_eq!(stored.derived, 0);

    let list = repo.list_for_case(CASE_ID).expect("list case rows");
    assert_eq!(list.len(), 1);
    let other = repo
        .list_for_case(OTHER_CASE_ID)
        .expect("list other case rows");
    assert!(other.is_empty());
}

#[test]
fn insert_if_missing_never_overwrites_existing_row() {
    let conn = setup_case_db();
    let repo = SourceCapabilityRepo::new(&conn);

    repo.upsert(&row(CASE_ID, SOURCE_ID, "artifacts", "ready", 0))
        .expect("insert task-confirmed row");
    repo.insert_if_missing(&row(CASE_ID, SOURCE_ID, "artifacts", "pending", 1))
        .expect("attempt backfill insert");

    let stored = repo
        .get(CASE_ID, SOURCE_ID, "artifacts")
        .expect("get row")
        .expect("row exists");
    assert_eq!(stored.state, "ready");
    assert_eq!(stored.derived, 0);
}
