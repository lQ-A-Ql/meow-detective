use persistence_source::repositories::staging_repo::StagingRepo;
use rusqlite::{Connection, ErrorCode};
use tempfile::TempDir;

#[test]
fn current_staging_schema_defaults_archive_to_false() {
    let temporary = TempDir::new().expect("create staging root");
    let fresh = StagingRepo::open_partition_staging_conn(temporary.path(), "fresh", 0)
        .expect("open fresh staging database");
    assert_archive_contract(&fresh, "fresh-file", "fresh");
}

fn assert_archive_contract(conn: &Connection, file_id: &str, data_source_id: &str) {
    let column: (String, i64, Option<String>) = conn
        .query_row(
            "SELECT type, \"notnull\", dflt_value
             FROM pragma_table_info('file_entries')
             WHERE name = 'archive'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("archive column");
    assert_eq!(column, ("INTEGER".to_string(), 1, Some("0".to_string())));

    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type)
         VALUES (?1, ?2, 'plain.txt', 'plain.txt', 'file')",
        (file_id, data_source_id),
    )
    .expect("insert file with default archive state");
    let default_value: i64 = conn
        .query_row(
            "SELECT archive FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read default state");
    assert_eq!(default_value, 0);

    conn.execute(
        "UPDATE file_entries SET archive = 1 WHERE id = ?1",
        [file_id],
    )
    .expect("persist archive state");
    let error = conn
        .execute(
            "UPDATE file_entries SET archive = 2 WHERE id = ?1",
            [file_id],
        )
        .expect_err("reject non-boolean archive state");
    assert!(matches!(
        error.sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    ));
}
