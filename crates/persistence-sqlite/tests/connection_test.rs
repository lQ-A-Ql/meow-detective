use persistence_sqlite::{open_in_memory, runner};
use tempfile::TempDir;

#[test]
fn create_new_db() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
    assert!(db_path.exists());

    let count = runner::run_all(&conn).unwrap();
    assert_eq!(count as usize, runner::MIGRATION_COUNT);
}

#[test]
fn open_existing_db() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    {
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        runner::run_all(&conn).unwrap();
    }
    {
        let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
        let count = runner::run_all(&conn).unwrap();
        assert_eq!(count, 0, "Re-opening should not re-apply migrations");
    }
}

#[test]
fn run_all_migrations() {
    let conn = open_in_memory().unwrap();
    let count = runner::run_all(&conn).unwrap();
    assert_eq!(count as usize, runner::MIGRATION_COUNT);

    for table in [
        "cases",
        "data_sources",
        "jobs",
        "reports",
        "tags",
        "graph_nodes",
        "graph_edges",
    ] {
        let has_table: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_table, "Table '{table}' should exist after migration");
    }
}

#[test]
fn application_schema_omits_source_detail_tables() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();

    for table in ["file_entries", "artifacts", "timeline_events"] {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!exists, "application database must not own {table}");
    }
}

#[test]
fn idempotent_rerun() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    let count = runner::run_all(&conn).unwrap();
    assert_eq!(count, 0, "Second run should not apply any migrations");
}

#[test]
fn latest_marker_does_not_hide_missing_earlier_migrations() {
    let conn = open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (name) VALUES (?1)",
        [runner::latest_version()],
    )
    .unwrap();

    let applied = runner::run_all(&conn).unwrap();

    assert_eq!(applied as usize, runner::MIGRATION_COUNT - 1);
    let cases_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'cases'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(cases_exists);
}

#[test]
fn version_query() {
    let conn = open_in_memory().unwrap();
    let version = runner::current_version(&conn).unwrap();
    assert!(version.is_none());

    runner::run_all(&conn).unwrap();
    let version = runner::current_version(&conn).unwrap();
    assert_eq!(version, Some(runner::latest_version().to_string()));
}

#[test]
fn failed_migration_is_not_marked_applied() {
    let conn = open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        INSERT INTO schema_migrations (name) VALUES ('0001_cases');
        CREATE TABLE cases (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            number TEXT,
            examiner TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE data_sources (
            id TEXT PRIMARY KEY NOT NULL
        );",
    )
    .unwrap();

    let err = runner::run_all(&conn).unwrap_err();
    assert!(format!("{err}").contains("0002_data_sources"));

    let applied: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE name = '0002_data_sources'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(applied, 0);
}
