use persistence_sqlite::{open_in_memory, runner};
use tempfile::TempDir;

#[test]
fn create_new_db() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let conn = persistence_sqlite::open_or_create(&db_path).unwrap();
    assert!(db_path.exists());

    let count = runner::run_all(&conn).unwrap();
    assert!(count >= 8, "Expected at least 8 migrations, got {}", count);
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
    assert_eq!(count, 8, "Expected 8 migrations to run");

    let tables = [
        "cases",
        "data_sources",
        "file_entries",
        "artifacts",
        "timeline_events",
        "jobs",
        "reports",
        "tags",
    ];
    for table in &tables {
        let has_table: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_table, "Table '{}' should exist after migration", table);
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
fn version_query() {
    let conn = open_in_memory().unwrap();
    let version = runner::current_version(&conn).unwrap();
    assert!(version.is_none());

    runner::run_all(&conn).unwrap();
    let version = runner::current_version(&conn).unwrap();
    assert_eq!(version, Some("0008_tags".to_string()));
}

#[test]
fn tables_exist_after_migration() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='file_entries'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(count > 0, "file_entries table should exist");
}
