use super::StagingRepo;
use persistence_sqlite::DbResult;
use rusqlite::Connection;

pub fn table_has_column(conn: &Connection, table: &str, column: &str) -> DbResult<bool> {
    conn.query_row(
        &format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = ?1"),
        [column],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

impl StagingRepo {
    pub fn ensure_enum_staging_columns(conn: &Connection) -> DbResult<()> {
        for column in ["hidden", "system", "read_only", "archive"] {
            if !table_has_column(conn, "file_entries", column)? {
                conn.execute(
                    &format!(
                        "ALTER TABLE file_entries ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0"
                    ),
                    [],
                )?;
            }
        }
        if !table_has_column(conn, "file_entries", "encrypted")? {
            conn.execute_batch("ALTER TABLE file_entries ADD COLUMN encrypted INTEGER CHECK (encrypted IS NULL OR encrypted IN (0,1))")?;
        }
        for column in ["partition_index", "unix_mode"] {
            if !table_has_column(conn, "file_entries", column)? {
                conn.execute(
                    &format!("ALTER TABLE file_entries ADD COLUMN {column} INTEGER"),
                    [],
                )?;
            }
        }
        Ok(())
    }

    pub fn ensure_analysis_staging_columns(conn: &Connection) -> DbResult<()> {
        for (table, column, sql_type) in [
            ("artifact_rows", "extractor_id", "TEXT"),
            ("artifact_rows", "extractor_version", "TEXT"),
            ("artifact_rows", "confidence", "REAL"),
            ("artifact_rows", "source_attribution", "TEXT"),
            ("timeline_rows", "parser_id", "TEXT"),
            ("timeline_rows", "parser_version", "TEXT"),
            ("timeline_rows", "confidence", "REAL"),
            ("timeline_rows", "source_attribution", "TEXT"),
        ] {
            if !table_has_column(conn, table, column)? {
                conn.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN {column} {sql_type}"),
                    [],
                )?;
            }
        }
        Ok(())
    }
}
