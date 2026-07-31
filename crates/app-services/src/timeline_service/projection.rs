use domain::{DataSourcePlatform, FileEntry};
use persistence_sqlite::repositories::{
    source_meta_repo::{SourceMetaRepo, TIMELINE_CURSOR_REVISION_KEY},
    timeline_repo::TimelineRepo,
};
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use super::TimelineServiceError;

const FILE_MODIFIED_PROJECTION_KEY: &str = "file_modified_v1";
const TIMELINE_GRAPH_PROJECTION_KEY: &str = "timeline_graph_v2";
const SOURCE_BATCH_SIZE: u32 = 10_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineProjectionStats {
    pub inserted_count: u64,
    pub elapsed_ms: u128,
    pub already_projected: bool,
    pub graph_complete: bool,
    pub warnings: Vec<String>,
}

pub fn project_and_store_file_modified(
    conn: &Connection,
    files: &[FileEntry],
) -> Result<u64, TimelineServiceError> {
    let events = files
        .par_iter()
        .filter_map(timeline::project_file_modified)
        .collect::<Vec<_>>();
    let count = events.len() as u64;
    if !events.is_empty() {
        TimelineRepo::new(conn).insert_batch(&events)?;
    }
    Ok(count)
}

pub fn materialize_file_modified(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    let identity = projection_identity(conn, platform)?;
    materialize_file_modified_with_identity(conn, platform, cancel_token, &identity)
}

pub fn materialize_file_modified_with_identity(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
    input_identity: &str,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    ensure_not_cancelled(cancel_token)?;
    if input_identity.trim().is_empty() {
        return Err(TimelineServiceError::InvalidInput(
            "timeline projection input identity must not be empty".to_string(),
        ));
    }
    if !projection_source_tables_present(conn)? {
        return Ok(already_projected_stats());
    }
    ensure_projection_meta_table(conn)?;
    let file_events_done = is_projection_done(conn, FILE_MODIFIED_PROJECTION_KEY, input_identity)?;
    let graph_supported = timeline_graph_tables_present(conn)?;
    let graph_done = !graph_supported
        || is_projection_done(conn, TIMELINE_GRAPH_PROJECTION_KEY, input_identity)?;
    if file_events_done && graph_done {
        return Ok(already_projected_stats());
    }

    let started = Instant::now();
    let inserted_count = if file_events_done {
        0
    } else {
        let inserted = replace_file_modified_events(conn, platform, cancel_token)?;
        mark_projection_done(conn, FILE_MODIFIED_PROJECTION_KEY, inserted, input_identity)?;
        inserted
    };
    let warnings = if graph_done {
        Vec::new()
    } else {
        populate_graph_non_fatal(conn, cancel_token, input_identity)?
    };
    let graph_complete = !graph_supported
        || is_projection_done(conn, TIMELINE_GRAPH_PROJECTION_KEY, input_identity)?;
    Ok(TimelineProjectionStats {
        inserted_count,
        elapsed_ms: started.elapsed().as_millis(),
        already_projected: false,
        graph_complete,
        warnings,
    })
}

fn replace_file_modified_events(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
) -> Result<u64, TimelineServiceError> {
    let transaction = conn.unchecked_transaction().map_err(|error| {
        TimelineServiceError::Other(format!("begin file timeline replacement: {error}"))
    })?;
    clear_previous_file_projection(&transaction)?;
    let inserted = insert_file_modified_batched(&transaction, platform, cancel_token)?;
    ensure_not_cancelled(cancel_token)?;
    SourceMetaRepo::new(&transaction).bump_revision(TIMELINE_CURSOR_REVISION_KEY)?;
    transaction.commit().map_err(|error| {
        TimelineServiceError::Other(format!("commit file timeline replacement: {error}"))
    })?;
    Ok(inserted)
}

fn insert_file_modified_batched(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
) -> Result<u64, TimelineServiceError> {
    let mut cursor = String::new();
    let mut inserted = 0;
    while let Some(next_cursor) = next_source_cursor(conn, &cursor, SOURCE_BATCH_SIZE)? {
        ensure_not_cancelled(cancel_token)?;
        inserted += insert_source_range(conn, platform, &cursor, &next_cursor)?;
        cursor = next_cursor;
    }
    Ok(inserted)
}

fn next_source_cursor(
    conn: &Connection,
    after_id: &str,
    batch_size: u32,
) -> Result<Option<String>, TimelineServiceError> {
    let mut statement = conn.prepare(
        "SELECT id FROM file_entries
         WHERE modified_at IS NOT NULL
           AND LOWER(entry_type) = 'file'
           AND id > ?1
         ORDER BY id ASC
         LIMIT ?2",
    )?;
    let mut rows = statement.query(params![after_id, batch_size])?;
    let mut last_id = None;
    while let Some(row) = rows.next()? {
        last_id = Some(row.get(0)?);
    }
    Ok(last_id)
}

fn insert_source_range(
    conn: &Connection,
    platform: DataSourcePlatform,
    after_id: &str,
    through_id: &str,
) -> Result<u64, TimelineServiceError> {
    let policy = file_policy_sql(conn, platform)?;
    let sql = format!(
        "INSERT OR IGNORE INTO timeline_events
         (id, case_id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs)
         SELECT
            'file-modified:' || fe.id,
            ds.case_id,
            fe.id,
            'FILE_MODIFIED',
            fe.modified_at,
            'File modified: ' || fe.name,
            fe.path || ' modified',
            'timeline.file_modified',
            '1',
            1.0,
            'FILE_MODIFIED',
            '{{\"platform\":\"{}\",\"timestampField\":\"modifiedAt\"}}'
         FROM file_entries fe
         JOIN data_sources ds ON ds.id = fe.data_source_id
         WHERE fe.modified_at IS NOT NULL
           AND LOWER(fe.entry_type) = 'file'
           AND fe.id > ?1
           AND fe.id <= ?2
           AND fe.modified_at NOT IN (
               '1970-01-01T00:00:00+00:00',
               '1970-01-01T00:00:00Z',
               '1970-01-01 00:00:00'
           )
           AND ({policy})",
        platform.as_storage_str(),
    );
    conn.execute(&sql, params![after_id, through_id])
        .map(|count| count as u64)
        .map_err(|error| {
            TimelineServiceError::Other(format!("insert FILE_MODIFIED timeline events: {error}"))
        })
}

fn file_policy_sql(
    conn: &Connection,
    platform: DataSourcePlatform,
) -> Result<String, TimelineServiceError> {
    let has_read_only = table_has_column(conn, "file_entries", "read_only")?;
    let read_only_policy = if has_read_only {
        "fe.read_only = 0"
    } else {
        "1 = 1"
    };
    match platform {
        DataSourcePlatform::Linux => Ok(read_only_policy.to_string()),
        DataSourcePlatform::Windows => Ok("COALESCE(fe.system, 0) = 0
             AND fe.name NOT LIKE '$%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE 'windows/%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE '[p%]/windows/%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE 'program files/%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE '[p%]/program files/%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE 'program files (x86)/%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE '[p%]/program files (x86)/%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE 'programdata/%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE '[p%]/programdata/%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE 'system volume information/%'
             AND LOWER(REPLACE(fe.path, '\\', '/')) NOT LIKE '[p%]/system volume information/%'"
            .to_string()),
        DataSourcePlatform::Unknown => Ok(read_only_policy.to_string()),
    }
}

fn table_has_column(
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

fn clear_previous_file_projection(conn: &Connection) -> Result<(), TimelineServiceError> {
    if timeline_graph_tables_present(conn)? {
        conn.execute_batch(
            "DELETE FROM graph_edges
             WHERE source_id IN (
                 SELECT id FROM timeline_events
                 WHERE parser_id IN ('timeline.macb', 'timeline.file_modified')
             );
             DELETE FROM graph_nodes
             WHERE id IN (
                 SELECT id FROM timeline_events
                 WHERE parser_id IN ('timeline.macb', 'timeline.file_modified')
             );",
        )?;
    }
    conn.execute(
        "DELETE FROM timeline_events
         WHERE parser_id IN ('timeline.macb', 'timeline.file_modified')",
        [],
    )?;
    Ok(())
}

fn projection_identity(
    conn: &Connection,
    platform: DataSourcePlatform,
) -> Result<String, TimelineServiceError> {
    let (count, max_id, max_modified): (u64, String, String) = conn.query_row(
        "SELECT COUNT(*), COALESCE(MAX(id), ''), COALESCE(MAX(modified_at), '')
         FROM file_entries",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Ok(format!(
        "file-modified-v1:{}:{count}:{max_id}:{max_modified}",
        platform.as_storage_str()
    ))
}

fn projection_source_tables_present(conn: &Connection) -> Result<bool, TimelineServiceError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name IN ('file_entries', 'data_sources')",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 2)
}

fn timeline_graph_tables_present(conn: &Connection) -> Result<bool, TimelineServiceError> {
    let count: u64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name IN ('graph_nodes', 'graph_edges')",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 2)
}

fn ensure_projection_meta_table(conn: &Connection) -> Result<(), TimelineServiceError> {
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

fn is_projection_done(
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

fn mark_projection_done(
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

fn populate_graph_non_fatal(
    conn: &Connection,
    cancel_token: &AtomicBool,
    input_identity: &str,
) -> Result<Vec<String>, TimelineServiceError> {
    match super::projection_graph::populate_timeline_event_graph(conn, cancel_token) {
        Ok(warnings) => {
            if warnings.is_empty() {
                mark_projection_done(conn, TIMELINE_GRAPH_PROJECTION_KEY, 0, input_identity)?;
            }
            Ok(warnings)
        }
        Err(TimelineServiceError::Cancelled) => Err(TimelineServiceError::Cancelled),
        Err(error) => {
            let message = format!("Timeline graph population failed: {error}");
            tracing::warn!("{message}");
            Ok(vec![message])
        }
    }
}

fn already_projected_stats() -> TimelineProjectionStats {
    TimelineProjectionStats {
        already_projected: true,
        graph_complete: true,
        ..TimelineProjectionStats::default()
    }
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), TimelineServiceError> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(TimelineServiceError::Cancelled)
    } else {
        Ok(())
    }
}

pub fn materialize_file_modified_unknown(
    conn: &Connection,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    let cancel_token = AtomicBool::new(false);
    materialize_file_modified(conn, DataSourcePlatform::Unknown, &cancel_token)
}

pub fn materialize_file_modified_unknown_with_cancel(
    conn: &Connection,
    cancel_token: &AtomicBool,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    materialize_file_modified(conn, DataSourcePlatform::Unknown, cancel_token)
}

pub fn materialize_file_modified_unknown_with_cancel_and_identity(
    conn: &Connection,
    cancel_token: &AtomicBool,
    input_identity: &str,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    materialize_file_modified_with_identity(
        conn,
        DataSourcePlatform::Unknown,
        cancel_token,
        input_identity,
    )
}

#[cfg(test)]
#[path = "../../tests/unit/timeline_service/projection.rs"]
mod tests;
