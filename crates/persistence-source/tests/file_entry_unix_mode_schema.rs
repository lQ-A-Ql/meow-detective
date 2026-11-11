use persistence_source::repositories::staging_repo::StagingRepo;
use rusqlite::Connection;
use tempfile::TempDir;

#[test]
fn staging_schema_carries_nullable_unix_mode() {
    let temporary = TempDir::new().expect("create staging root");
    let connection = StagingRepo::open_partition_staging_conn(temporary.path(), "unix-mode", 0)
        .expect("open staging database");
    assert_unix_mode_contract(&connection, "staging-file", "unix-mode");
}

fn assert_unix_mode_contract(conn: &Connection, file_id: &str, data_source_id: &str) {
    let column: (String, i64, Option<String>) = conn
        .query_row(
            "SELECT type, \"notnull\", dflt_value
             FROM pragma_table_info('file_entries')
             WHERE name = 'unix_mode'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("unix_mode column");
    assert_eq!(column, ("INTEGER".to_string(), 0, None));

    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type)
         VALUES (?1, ?2, 'plain.txt', 'plain.txt', 'file')",
        (file_id, data_source_id),
    )
    .expect("insert file without unix mode");
    let default_value: Option<i64> = conn
        .query_row(
            "SELECT unix_mode FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read default state");
    assert_eq!(default_value, None);

    conn.execute(
        "UPDATE file_entries SET unix_mode = 33188 WHERE id = ?1",
        [file_id],
    )
    .expect("persist unix mode");
    let stored: Option<i64> = conn
        .query_row(
            "SELECT unix_mode FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read persisted unix mode");
    assert_eq!(stored, Some(0o100644i64));
}
