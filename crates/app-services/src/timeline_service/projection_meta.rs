//! Projection bookkeeping for the timeline file-activity materialization:
//! the input identity fingerprint, the `timeline_projection_meta` status
//! table, and the table-presence probes those depend on. Split out of
//! `projection.rs` to keep both modules under the size budget.

use domain::DataSourcePlatform;
use rusqlite::{params, Connection, OptionalExtension};

use super::TimelineServiceError;

pub(super) fn table_has_column(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, TimelineServiceError> {
    let count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
        [column],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub(super) fn projection_identity(
    conn: &Connection,
    platform: DataSourcePlatform,
) -> Result<String, TimelineServiceError> {
    let (count, max_id, max_created, max_modified, max_accessed, max_changed, deleted_count): (
        u64,
        String,
        String,
        String,
        String,
        String,
        u64,
    ) = conn.query_row(
        "SELECT COUNT(*), COALESCE(MAX(id), ''), COALESCE(MAX(created_at), ''),
                COALESCE(MAX(modified_at), ''), COALESCE(MAX(accessed_at), ''),
                COALESCE(MAX(changed_at), ''), COALESCE(SUM(deleted), 0)
         FROM file_entries",
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        },
    )?;
    Ok(format!(
        "file-activity-v2:{}:{count}:{max_id}:{max_created}:{max_modified}:{max_accessed}:{max_changed}:{deleted_count}",
        platform.as_storage_str()
    ))
}

pub(super) fn projection_source_tables_present(
    conn: &Connection,
) -> Result<bool, TimelineServiceError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name IN ('file_entries', 'data_sources')",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 2)
}

pub(super) fn timeline_graph_tables_present(
    conn: &Connection,
) -> Result<bool, TimelineServiceError> {
    let count: u64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name IN ('graph_nodes', 'graph_edges')",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 2)
}

pub(super) fn ensure_projection_meta_table(conn: &Connection) -> Result<(), TimelineServiceError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS timeline_projection_meta (
            projection_key TEXT PRIMARY KEY NOT NULL,
            status TEXT NOT NULL,
            inserted_count INTEGER NOT NULL DEFAULT 0,
            input_identity TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;
    Ok(())
}

pub(super) fn is_projection_done(
    conn: &Connection,
    key: &str,
    input_identity: &str,
) -> Result<bool, TimelineServiceError> {
    let status = conn
        .query_row(
            "SELECT status, input_identity FROM timeline_projection_meta
             WHERE projection_key = ?1",
            [key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    Ok(status.is_some_and(|(status, identity)| status == "done" && identity == input_identity))
}

pub(super) fn mark_projection_done(
    conn: &Connection,
    key: &str,
    inserted_count: u64,
    input_identity: &str,
) -> Result<(), TimelineServiceError> {
    conn.execute(
        "INSERT INTO timeline_projection_meta
         (projection_key, status, inserted_count, input_identity, updated_at)
         VALUES (?1, 'done', ?2, ?3, datetime('now'))
         ON CONFLICT(projection_key) DO UPDATE SET
            status = excluded.status,
            inserted_count = excluded.inserted_count,
            input_identity = excluded.input_identity,
            updated_at = excluded.updated_at",
        params![key, inserted_count as i64, input_identity],
    )?;
    Ok(())
}
