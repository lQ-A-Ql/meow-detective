use domain::{DataSourcePlatform, FileEntry};
use persistence_sqlite::repositories::{
    source_meta_repo::{SourceMetaRepo, TIMELINE_CURSOR_REVISION_KEY},
    timeline_repo::TimelineRepo,
};
use rayon::prelude::*;
use rusqlite::{params, Connection};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use super::projection_meta::{
    ensure_projection_meta_table, is_projection_done, mark_projection_done, projection_identity,
    projection_source_tables_present, table_has_column, timeline_graph_tables_present,
};
use super::TimelineServiceError;

const FILE_ACTIVITY_PROJECTION_KEY: &str = "file_activity_v2";
const TIMELINE_GRAPH_PROJECTION_KEY: &str = "timeline_graph_v3";
const SOURCE_BATCH_SIZE: u32 = 10_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineProjectionStats {
    pub inserted_count: u64,
    pub elapsed_ms: u128,
    pub events_elapsed_ms: u128,
    pub graph_elapsed_ms: u128,
    pub already_projected: bool,
    pub graph_complete: bool,
    pub warnings: Vec<String>,
}

pub fn project_and_store_file_activity(
    conn: &Connection,
    files: &[FileEntry],
) -> Result<u64, TimelineServiceError> {
    let events = files
        .par_iter()
        .flat_map_iter(timeline::project_file_activity)
        .collect::<Vec<_>>();
    let count = events.len() as u64;
    if !events.is_empty() {
        TimelineRepo::new(conn).insert_batch(&events)?;
    }
    Ok(count)
}

pub fn materialize_file_activity(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    let identity = projection_identity(conn, platform)?;
    materialize_file_activity_with_identity(conn, platform, cancel_token, &identity)
}

pub fn materialize_file_activity_with_identity(
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
    let file_events_done = is_projection_done(conn, FILE_ACTIVITY_PROJECTION_KEY, input_identity)?;
    let graph_supported = timeline_graph_tables_present(conn)?;
    let graph_done = !graph_supported
        || is_projection_done(conn, TIMELINE_GRAPH_PROJECTION_KEY, input_identity)?;
    if file_events_done && graph_done {
        return Ok(already_projected_stats());
    }

    with_timeline_page_cache(conn, || {
        let started = Instant::now();
        let events_started = Instant::now();
        let inserted_count = if file_events_done {
            0
        } else {
            let inserted = replace_file_activity_events(conn, platform, cancel_token)?;
            mark_projection_done(conn, FILE_ACTIVITY_PROJECTION_KEY, inserted, input_identity)?;
            inserted
        };
        let events_elapsed_ms = events_started.elapsed().as_millis();
        let graph_started = Instant::now();
        let warnings = if graph_done {
            Vec::new()
        } else {
            populate_graph_non_fatal(conn, cancel_token, input_identity)?
        };
        let graph_elapsed_ms = graph_started.elapsed().as_millis();
        let graph_complete = !graph_supported
            || is_projection_done(conn, TIMELINE_GRAPH_PROJECTION_KEY, input_identity)?;
        Ok(TimelineProjectionStats {
            inserted_count,
            elapsed_ms: started.elapsed().as_millis(),
            events_elapsed_ms,
            graph_elapsed_ms,
            already_projected: false,
            graph_complete,
            warnings,
        })
    })
}

/// Timeline materialization is dominated by random B-tree descends on
/// multi-gigabyte sources; a large page cache cut the write-side cost by
/// 5-8x in real-image measurements. Run the phase with a ~1 GiB cache and
/// restore the previous setting afterwards (the value is per-connection).
fn with_timeline_page_cache<T>(
    conn: &Connection,
    f: impl FnOnce() -> Result<T, TimelineServiceError>,
) -> Result<T, TimelineServiceError> {
    let previous: i64 = conn
        .query_row("PRAGMA cache_size", [], |row| row.get(0))
        .map_err(|error| TimelineServiceError::Other(format!("read cache_size: {error}")))?;
    conn.execute_batch("PRAGMA cache_size = -1000000")
        .map_err(|error| TimelineServiceError::Other(format!("raise cache_size: {error}")))?;
    let result = f();
    let _ = conn.execute_batch(&format!("PRAGMA cache_size = {previous}"));
    result
}

fn replace_file_activity_events(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel_token: &AtomicBool,
) -> Result<u64, TimelineServiceError> {
    let transaction = conn.unchecked_transaction().map_err(|error| {
        TimelineServiceError::Other(format!("begin file activity replacement: {error}"))
    })?;
    clear_previous_file_projection(&transaction)?;
    let inserted = insert_file_activity_batched(&transaction, platform, cancel_token)?;
    ensure_not_cancelled(cancel_token)?;
    SourceMetaRepo::new(&transaction).bump_revision(TIMELINE_CURSOR_REVISION_KEY)?;
    transaction.commit().map_err(|error| {
        TimelineServiceError::Other(format!("commit file activity replacement: {error}"))
    })?;
    Ok(inserted)
}

fn insert_file_activity_batched(
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
         WHERE (created_at IS NOT NULL
                OR modified_at IS NOT NULL
                OR accessed_at IS NOT NULL
                OR (deleted = 1 AND changed_at IS NOT NULL))
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
        "WITH activity(
            id_prefix, event_type, timestamp_field, title_prefix,
            description_suffix, parser_id, confidence, timestamp_semantics,
            requires_deleted
         ) AS (
            VALUES
              ('file-created:', 'FILE_CREATED', 'createdAt', 'File created: ',
               ' created', 'timeline.file_created', 1.0,
               'filesystem creation or birth timestamp', 0),
              ('file-modified:', 'FILE_MODIFIED', 'modifiedAt', 'File modified: ',
               ' modified', 'timeline.file_modified', 1.0,
               'filesystem content modification timestamp', 0),
              ('file-accessed:', 'FILE_ACCESSED', 'accessedAt', 'File accessed: ',
               ' accessed', 'timeline.file_accessed', 1.0,
               'filesystem access timestamp; does not prove execution', 0),
              ('file-deleted:', 'FILE_DELETED', 'changedAt', 'Deleted file record: ',
               ' is marked deleted', 'timeline.file_deleted', 0.65,
               'metadata change timestamp on a deleted record; deletion time is approximate', 1)
         )
         INSERT OR IGNORE INTO timeline_events
         (id, case_id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs)
         SELECT
            activity.id_prefix || fe.id,
            ds.case_id,
            fe.id,
            activity.event_type,
            CASE activity.timestamp_field
              WHEN 'createdAt' THEN fe.created_at
              WHEN 'modifiedAt' THEN fe.modified_at
              WHEN 'accessedAt' THEN fe.accessed_at
              WHEN 'changedAt' THEN fe.changed_at
            END,
            activity.title_prefix || fe.name,
            fe.path || activity.description_suffix,
            activity.parser_id,
            '2',
            activity.confidence,
            activity.event_type,
            printf(
              '{{\"platform\":\"{}\",\"timestampField\":\"%s\",\"timestampSemantics\":\"%s\"}}',
              activity.timestamp_field,
              activity.timestamp_semantics
            )
         FROM file_entries fe
         JOIN data_sources ds ON ds.id = fe.data_source_id
         CROSS JOIN activity
         WHERE CASE activity.timestamp_field
              WHEN 'createdAt' THEN fe.created_at
              WHEN 'modifiedAt' THEN fe.modified_at
              WHEN 'accessedAt' THEN fe.accessed_at
              WHEN 'changedAt' THEN fe.changed_at
            END IS NOT NULL
           AND LOWER(fe.entry_type) = 'file'
           AND fe.id > ?1
           AND fe.id <= ?2
           AND CASE activity.timestamp_field
              WHEN 'createdAt' THEN fe.created_at
              WHEN 'modifiedAt' THEN fe.modified_at
              WHEN 'accessedAt' THEN fe.accessed_at
              WHEN 'changedAt' THEN fe.changed_at
            END NOT IN (
               '1970-01-01T00:00:00+00:00',
               '1970-01-01T00:00:00Z',
               '1970-01-01 00:00:00'
           )
           AND (activity.requires_deleted = 0 OR fe.deleted = 1)
           AND ({policy})",
        platform.as_storage_str(),
    );
    conn.execute(&sql, params![after_id, through_id])
        .map(|count| count as u64)
        .map_err(|error| {
            TimelineServiceError::Other(format!("insert file activity timeline events: {error}"))
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

fn clear_previous_file_projection(conn: &Connection) -> Result<(), TimelineServiceError> {
    if timeline_graph_tables_present(conn)? {
        conn.execute_batch(
            "DELETE FROM graph_edges
             WHERE source_id IN (
                 SELECT id FROM timeline_events
                 WHERE parser_id IN (
                    'timeline.macb', 'timeline.file_created', 'timeline.file_modified',
                    'timeline.file_accessed', 'timeline.file_deleted'
                 )
             );
             DELETE FROM graph_nodes
             WHERE id IN (
                 SELECT id FROM timeline_events
                 WHERE parser_id IN (
                    'timeline.macb', 'timeline.file_created', 'timeline.file_modified',
                    'timeline.file_accessed', 'timeline.file_deleted'
                 )
             );",
        )?;
    }
    conn.execute(
        "DELETE FROM timeline_events
         WHERE parser_id IN (
            'timeline.macb', 'timeline.file_created', 'timeline.file_modified',
            'timeline.file_accessed', 'timeline.file_deleted'
         )",
        [],
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

pub fn materialize_file_activity_unknown(
    conn: &Connection,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    let cancel_token = AtomicBool::new(false);
    materialize_file_activity(conn, DataSourcePlatform::Unknown, &cancel_token)
}

pub fn materialize_file_activity_unknown_with_cancel(
    conn: &Connection,
    cancel_token: &AtomicBool,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    materialize_file_activity(conn, DataSourcePlatform::Unknown, cancel_token)
}

pub fn materialize_file_activity_unknown_with_cancel_and_identity(
    conn: &Connection,
    cancel_token: &AtomicBool,
    input_identity: &str,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    materialize_file_activity_with_identity(
        conn,
        DataSourcePlatform::Unknown,
        cancel_token,
        input_identity,
    )
}

#[cfg(test)]
#[path = "../../tests/unit/timeline_service/projection.rs"]
mod tests;
