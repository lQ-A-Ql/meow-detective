use super::*;

fn sqlite_sidecar_path(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    value.into()
}

fn assert_sqlite_read_only(error: &rusqlite::Error) {
    assert_eq!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ReadOnly),
        "expected SQLite read-only failure, got {error}"
    );
}

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

#[test]
fn open_existing_source_read_only_enforces_query_only_and_rejects_writes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("source.db");
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode=DELETE;
                 CREATE TABLE evidence_marker(id INTEGER PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO evidence_marker(value) VALUES ('preserved');",
            )
            .unwrap();
    }

    let connection = open_existing_source_read_only(&path).unwrap();
    let query_only: i64 = connection
        .query_row("PRAGMA query_only", [], |row| row.get(0))
        .unwrap();
    assert_eq!(query_only, 1);

    let dml_error = connection
        .execute(
            "UPDATE evidence_marker SET value = 'changed' WHERE id = 1",
            [],
        )
        .unwrap_err();
    assert_sqlite_read_only(&dml_error);
    let ddl_error = connection
        .execute_batch("CREATE TABLE forbidden_write(id INTEGER)")
        .unwrap_err();
    assert_sqlite_read_only(&ddl_error);

    let preserved: String = connection
        .query_row(
            "SELECT value FROM evidence_marker WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(preserved, "preserved");
}

#[test]
fn open_existing_source_read_only_does_not_migrate_or_create_sidecars() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("legacy-source.db");
    let wal_path = sqlite_sidecar_path(&path, "-wal");
    let shm_path = sqlite_sidecar_path(&path, "-shm");
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode=DELETE;
                 CREATE TABLE schema_migrations (
                     id INTEGER PRIMARY KEY,
                     name TEXT NOT NULL UNIQUE,
                     applied_at TEXT NOT NULL DEFAULT (datetime('now'))
                 );
                 INSERT INTO schema_migrations(name) VALUES ('source_001');
                 CREATE TABLE evidence_marker(value TEXT NOT NULL);
                 INSERT INTO evidence_marker(value) VALUES ('preserved');",
            )
            .unwrap();
    }
    assert!(!wal_path.exists());
    assert!(!shm_path.exists());
    let before = std::fs::read(&path).unwrap();

    {
        let connection = open_existing_source_read_only(&path).unwrap();
        assert_eq!(
            crate::migrations::runner::current_version(&connection).unwrap(),
            Some("source_001".to_string())
        );
        let migrated_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'ceph_bluestore_omap_scans'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migrated_table_count, 0);
    }

    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!wal_path.exists());
    assert!(!shm_path.exists());
    let verifier =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert_eq!(
        crate::migrations::runner::current_version(&verifier).unwrap(),
        Some("source_001".to_string())
    );
}
