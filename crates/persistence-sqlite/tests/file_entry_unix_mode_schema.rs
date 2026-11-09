use persistence_sqlite::{open_in_memory, runner};
use rusqlite::Connection;

#[test]
fn application_schema_carries_nullable_unix_mode() {
    let application = open_in_memory().expect("open application database");
    runner::run_all(&application).expect("run application migrations");
    seed_application_source(&application);
    assert_unix_mode_contract(&application, "app-file", "app-source");
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

fn seed_application_source(conn: &Connection) {
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-1', 'case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("insert application case");
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('app-source', 'case-1', 'source', 'e01', '', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("insert data source");
}
