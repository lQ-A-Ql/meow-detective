use persistence_source::repositories::staging_repo::StagingRepo;
use rusqlite::{Connection, ErrorCode};
use tempfile::TempDir;

#[test]
fn current_staging_schema_preserves_unknown_and_boolean_encryption() {
    let temporary = TempDir::new().expect("create staging root");
    let fresh = StagingRepo::open_partition_staging_conn(temporary.path(), "fresh", 0)
        .expect("open fresh staging database");
    assert_encrypted_column_contract(&fresh, "fresh-file", "fresh");
}

fn assert_encrypted_column_contract(conn: &Connection, file_id: &str, data_source_id: &str) {
    let column: (String, i64, Option<String>) = conn
        .query_row(
            "SELECT type, \"notnull\", dflt_value
             FROM pragma_table_info('file_entries')
             WHERE name = 'encrypted'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("encrypted column");
    assert_eq!(column, ("INTEGER".to_string(), 0, None));

    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type)
         VALUES (?1, ?2, 'plain.txt', 'plain.txt', 'file')",
        (file_id, data_source_id),
    )
    .expect("insert ordinary file with default encryption state");
    let encrypted: Option<i64> = conn
        .query_row(
            "SELECT encrypted FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read default encryption state");
    assert_eq!(
        encrypted, None,
        "omitted classification must remain unknown"
    );

    conn.execute(
        "UPDATE file_entries SET encrypted = 0 WHERE id = ?1",
        [file_id],
    )
    .expect("persist explicit clear state");
    let clear: i64 = conn
        .query_row(
            "SELECT encrypted FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read clear state");
    assert_eq!(clear, 0);

    conn.execute(
        "UPDATE file_entries SET encrypted = 1 WHERE id = ?1",
        [file_id],
    )
    .expect("persist explicit encrypted state");

    let error = conn
        .execute(
            "UPDATE file_entries SET encrypted = 2 WHERE id = ?1",
            [file_id],
        )
        .expect_err("reject a non-boolean encryption state");
    assert!(matches!(
        error.sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    ));
}
