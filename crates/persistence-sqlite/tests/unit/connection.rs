use super::*;

#[test]
fn open_in_memory_can_query() {
    let conn = open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE test_tbl (id INTEGER PRIMARY KEY, val TEXT)")
        .unwrap();
    conn.execute("INSERT INTO test_tbl (val) VALUES ('hello')", [])
        .unwrap();
    let val: String = conn
        .query_row("SELECT val FROM test_tbl WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(val, "hello");
}

#[test]
fn open_in_memory_foreign_keys_enabled() {
    let conn = open_in_memory().unwrap();
    let fk: i32 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    assert_eq!(fk, 1);
}

#[test]
fn open_staging_creates_meta_table() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test_staging.db");
    let conn = open_staging(&path).unwrap();

    // staging_meta table should exist
    let tbl: String = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='staging_meta'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tbl, "staging_meta");

    // file_entries table should exist
    let tbl: String = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='file_entries'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tbl, "file_entries");
}

#[test]
fn open_staging_is_idempotent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test_staging.db");
    let conn1 = open_staging(&path).unwrap();
    conn1
        .execute(
            "INSERT INTO staging_meta (key, value) VALUES ('k', 'v')",
            [],
        )
        .unwrap();
    drop(conn1);

    // Opening again should not fail or lose data
    let conn2 = open_staging(&path).unwrap();
    let val: String = conn2
        .query_row(
            "SELECT value FROM staging_meta WHERE key = 'k'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(val, "v");
}

#[test]
fn open_or_create_source_runs_source_schema() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("sources").join("ds-1").join("source.db");
    let conn = open_or_create_source(&path).unwrap();

    for table in [
        "source_meta",
        "data_sources",
        "data_source_partitions",
        "file_entries",
        "artifacts",
        "timeline_events",
    ] {
        let found: String = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, table);
    }
}

#[test]
fn open_or_create_creates_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("subdir").join("test.db");
    assert!(!path.exists());

    let conn = open_or_create(&path).unwrap();
    conn.execute_batch("CREATE TABLE t (id INTEGER)").unwrap();
    assert!(path.exists());
}

#[test]
fn open_or_create_wal_mode() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("wal_test.db");
    let conn = open_or_create(&path).unwrap();
    let journal: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal, "wal");
}

#[test]
fn open_existing_fails_if_missing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("nonexistent.db");
    let result = open_existing(&path);
    assert!(result.is_err());
}

#[test]
fn open_existing_works_on_existing_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("existing.db");

    // Create the file first
    let conn_create = open_or_create(&path).unwrap();
    conn_create
        .execute_batch("CREATE TABLE t (id INTEGER)")
        .unwrap();
    drop(conn_create);

    // Now open_existing should work
    let conn = open_existing(&path).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
