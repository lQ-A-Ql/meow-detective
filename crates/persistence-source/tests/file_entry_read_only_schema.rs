use persistence_source::repositories::staging_repo::StagingRepo;
use rusqlite::{Connection, ErrorCode};
use tempfile::TempDir;

#[test]
fn fresh_and_upgraded_staging_schemas_default_read_only_to_false() {
    let temporary = TempDir::new().expect("create staging root");
    let fresh = StagingRepo::open_partition_staging_conn(temporary.path(), "fresh", 0)
        .expect("open fresh staging database");
    assert_read_only_contract(&fresh, "fresh-file", "fresh", true);

    let upgraded = Connection::open_in_memory().expect("open legacy staging database");
    upgraded
        .execute_batch(
            "CREATE TABLE file_entries (
                id TEXT PRIMARY KEY NOT NULL,
                parent_id TEXT,
                data_source_id TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                size INTEGER,
                ext TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT,
                modified_at TEXT,
                accessed_at TEXT,
                changed_at TEXT,
                hash_sha256 TEXT
            );
            INSERT INTO file_entries
                (id, data_source_id, path, name, entry_type)
            VALUES ('legacy-stage', 'upgraded', 'old.bin', 'old.bin', 'file');",
        )
        .expect("create legacy staging schema");
    StagingRepo::ensure_enum_staging_columns(&upgraded).expect("upgrade staging schema");
    let historical: i64 = upgraded
        .query_row(
            "SELECT read_only FROM file_entries WHERE id = 'legacy-stage'",
            [],
            |row| row.get(0),
        )
        .expect("read upgraded staging status");
    assert_eq!(historical, 0);
    assert_read_only_contract(&upgraded, "upgraded-file", "upgraded", false);
}

fn assert_read_only_contract(
    conn: &Connection,
    file_id: &str,
    data_source_id: &str,
    enforces_check: bool,
) {
    let column: (String, i64, Option<String>) = conn
        .query_row(
            "SELECT type, \"notnull\", dflt_value
             FROM pragma_table_info('file_entries')
             WHERE name = 'read_only'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read_only column");
    assert_eq!(column, ("INTEGER".to_string(), 1, Some("0".to_string())));

    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type)
         VALUES (?1, ?2, 'plain.txt', 'plain.txt', 'file')",
        (file_id, data_source_id),
    )
    .expect("insert file with default read-only state");
    let default_value: i64 = conn
        .query_row(
            "SELECT read_only FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read default state");
    assert_eq!(default_value, 0);

    conn.execute(
        "UPDATE file_entries SET read_only = 1 WHERE id = ?1",
        [file_id],
    )
    .expect("persist read-only state");
    if enforces_check {
        let error = conn
            .execute(
                "UPDATE file_entries SET read_only = 2 WHERE id = ?1",
                [file_id],
            )
            .expect_err("reject non-boolean read-only state");
        assert!(matches!(
            error.sqlite_error_code(),
            Some(ErrorCode::ConstraintViolation)
        ));
    }
}
