use rusqlite::Connection;

pub(super) fn validate_enum_merge_target(conn: &Connection) -> rusqlite::Result<()> {
    if !table_exists(conn, "source_meta")? {
        return Err(rusqlite::Error::InvalidColumnName(
            "source merge target is not a source database: missing source_meta".to_string(),
        ));
    }
    if !table_exists(conn, "schema_migrations")? {
        return Err(rusqlite::Error::InvalidColumnName(
            "source merge target is not a source database: missing schema_migrations".to_string(),
        ));
    }
    let latest_source_version = crate::migrations::runner::latest_source_version();
    let has_latest_source_migration: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM schema_migrations WHERE name = ?1
        )",
        [latest_source_version],
        |row| row.get(0),
    )?;
    if !has_latest_source_migration {
        return Err(rusqlite::Error::InvalidColumnName(format!(
            "source merge target is not a current source database: missing {latest_source_version}"
        )));
    }
    for column in [
        "id",
        "parent_id",
        "data_source_id",
        "path",
        "name",
        "entry_type",
        "partition_index",
    ] {
        let present: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('file_entries') WHERE name = ?1",
            [column],
            |row| row.get(0),
        )?;
        if !present {
            return Err(rusqlite::Error::InvalidColumnName(format!(
                "source merge target file_entries.{column}"
            )));
        }
    }
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_schema
            WHERE type = 'table' AND name = ?1
        )",
        [table],
        |row| row.get(0),
    )
}
