use rusqlite::{Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

use super::source_registry::SOURCE_MIGRATIONS;

pub(super) fn registered_sql() -> &'static str {
    SOURCE_MIGRATIONS
        .iter()
        .find_map(|(name, sql)| (*name == "source_031_mount_directory_index").then_some(*sql))
        .expect("source mount directory migration must be registered")
}

pub(super) fn ensure(conn: &Connection, sql: &str) -> DbResult<()> {
    let has_mount_columns: bool = conn.query_row(
        "SELECT COUNT(*) = 6
         FROM pragma_table_info('file_entries')
         WHERE name IN (
             'parent_id', 'data_source_id', 'partition_index',
             'entry_type', 'name', 'id'
         )",
        [],
        |row| row.get(0),
    )?;
    if !has_mount_columns {
        return Err(DbError::Migration(
            "source_031 requires file_entries(parent_id, data_source_id, partition_index, entry_type, name, id)"
                .to_string(),
        ));
    }
    let current_sql = conn
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_source_file_entries_mount_children'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if current_sql.as_deref().is_some_and(index_is_current) {
        return Ok(());
    }
    repair(conn, sql)
}

fn index_is_current(sql: &str) -> bool {
    let normalized = sql
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    normalized.contains(
        "on file_entries( parent_id, data_source_id, partition_index, entry_type, name collate nocase, id )",
    ) && !normalized.contains("where deleted = 0")
}

fn repair(conn: &Connection, sql: &str) -> DbResult<()> {
    conn.execute_batch(
        "SAVEPOINT source_031_mount_directory_repair;
         DROP INDEX IF EXISTS idx_source_file_entries_mount_children;",
    )?;
    if let Err(error) = conn.execute_batch(sql) {
        let _ = conn.execute_batch(
            "ROLLBACK TO source_031_mount_directory_repair;
             RELEASE source_031_mount_directory_repair;",
        );
        return Err(error.into());
    }
    conn.execute_batch("RELEASE source_031_mount_directory_repair;")?;
    Ok(())
}
