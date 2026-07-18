use domain::FileEntry;
use persistence_sqlite::repositories::timeline_repo::TimelineRepo;
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use super::TimelineServiceError;

const MACB_PROJECTION_KEY: &str = "macb";
const TIMELINE_GRAPH_PROJECTION_KEY: &str = "macb_graph";
const MACB_SOURCE_BATCH_SIZE: u32 = 10_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineProjectionStats {
    pub inserted_count: u64,
    pub elapsed_ms: u128,
    pub already_projected: bool,
    pub graph_complete: bool,
    pub warnings: Vec<String>,
}

pub fn project_and_store_macb(
    conn: &Connection,
    files: &[FileEntry],
) -> Result<u64, TimelineServiceError> {
    let events: Vec<domain::TimelineEvent> = files
        .par_iter()
        .flat_map_iter(timeline::project_file_macb)
        .collect();
    let count = events.len() as u64;
    if !events.is_empty() {
        TimelineRepo::new(conn).insert_batch(&events)?;
    }
    Ok(count)
}

pub fn ensure_macb_timeline_projected(
    conn: &Connection,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    let cancel_token = AtomicBool::new(false);
    ensure_macb_timeline_projected_with_cancel(conn, &cancel_token)
}

pub fn ensure_macb_timeline_projected_with_cancel(
    conn: &Connection,
    cancel_token: &AtomicBool,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    let input_identity = implicit_projection_identity(conn)?;
    ensure_macb_timeline_projected_with_cancel_and_identity(conn, cancel_token, &input_identity)
}

pub fn ensure_macb_timeline_projected_with_cancel_and_identity(
    conn: &Connection,
    cancel_token: &AtomicBool,
    input_identity: &str,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    ensure_not_cancelled(cancel_token)?;
    if input_identity.trim().is_empty() {
        return Err(TimelineServiceError::InvalidInput(
            "timeline projection input identity must not be empty".to_string(),
        ));
    }
    if !timeline_projection_source_tables_present(conn)? {
        return Ok(already_projected_stats());
    }
    ensure_projection_meta_table(conn)?;
    let macb_already_projected = is_projection_done(conn, MACB_PROJECTION_KEY, input_identity)?;
    let graph_supported = timeline_graph_tables_present(conn)?;
    let graph_already_projected = !graph_supported
        || is_projection_done(conn, TIMELINE_GRAPH_PROJECTION_KEY, input_identity)?;
    if macb_already_projected && graph_already_projected {
        return Ok(already_projected_stats());
    }

    let started = Instant::now();
    let inserted = if macb_already_projected {
        0
    } else {
        let inserted = replace_macb_timeline_sql(conn, cancel_token)?;
        mark_projection_done(conn, MACB_PROJECTION_KEY, inserted, input_identity)?;
        inserted
    };
    let warnings = if graph_already_projected {
        Vec::new()
    } else {
        populate_graph_non_fatal(conn, cancel_token, input_identity)?
    };
    let graph_complete = is_projection_done(conn, TIMELINE_GRAPH_PROJECTION_KEY, input_identity)?;

    Ok(TimelineProjectionStats {
        inserted_count: inserted,
        elapsed_ms: started.elapsed().as_millis(),
        already_projected: false,
        graph_complete,
        warnings,
    })
}

fn already_projected_stats() -> TimelineProjectionStats {
    TimelineProjectionStats {
        already_projected: true,
        graph_complete: true,
        ..TimelineProjectionStats::default()
    }
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

fn timeline_projection_source_tables_present(
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

fn timeline_graph_tables_present(conn: &Connection) -> Result<bool, TimelineServiceError> {
    let count: u64 = conn.query_row(
        "SELECT COUNT(*)
         FROM sqlite_master
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
    let status: Option<(String, String)> = conn
        .query_row(
            "SELECT status, input_identity
             FROM timeline_projection_meta
             WHERE projection_key = ?1",
            params![key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(status.as_ref().is_some_and(|(status, stored_identity)| {
        status == "done" && stored_identity == input_identity
    }))
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

fn replace_macb_timeline_sql(
    conn: &Connection,
    cancel_token: &AtomicBool,
) -> Result<u64, TimelineServiceError> {
    let transaction = conn.unchecked_transaction().map_err(|error| {
        TimelineServiceError::Other(format!("begin MACB timeline replacement: {error}"))
    })?;
    clear_previous_macb_projection(&transaction)?;
    let inserted = project_macb_timeline_sql(&transaction, cancel_token)?;
    ensure_not_cancelled(cancel_token)?;
    transaction.commit().map_err(|error| {
        TimelineServiceError::Other(format!("commit MACB timeline replacement: {error}"))
    })?;
    Ok(inserted)
}

fn clear_previous_macb_projection(conn: &Connection) -> Result<(), TimelineServiceError> {
    if timeline_graph_tables_present(conn)? {
        conn.execute_batch(
            "DELETE FROM graph_edges
             WHERE source_id IN (
                 SELECT id FROM timeline_events WHERE parser_id = 'timeline.macb'
             );
             DELETE FROM graph_nodes
             WHERE id IN (
                 SELECT id FROM timeline_events WHERE parser_id = 'timeline.macb'
             );",
        )?;
    }
    conn.execute(
        "DELETE FROM timeline_events WHERE parser_id = 'timeline.macb'",
        [],
    )?;
    Ok(())
}

fn project_macb_timeline_sql(
    conn: &Connection,
    cancel_token: &AtomicBool,
) -> Result<u64, TimelineServiceError> {
    let kinds = [
        ("created_at", "FILE_CREATED", "File created: ", " created"),
        (
            "modified_at",
            "FILE_MODIFIED",
            "File modified: ",
            " modified",
        ),
        (
            "accessed_at",
            "FILE_ACCESSED",
            "File accessed: ",
            " accessed",
        ),
        (
            "changed_at",
            "FILE_METADATA_CHANGED",
            "File metadata changed: ",
            " metadata changed",
        ),
    ];
    let mut inserted = 0;
    for (column, event_type, title_prefix, description_suffix) in kinds {
        ensure_not_cancelled(cancel_token)?;
        inserted += insert_macb_kind_batched(
            conn,
            column,
            event_type,
            title_prefix,
            description_suffix,
            cancel_token,
        )?;
    }
    Ok(inserted)
}

fn insert_macb_kind_batched(
    conn: &Connection,
    timestamp_column: &str,
    event_type: &str,
    title_prefix: &str,
    description_suffix: &str,
    cancel_token: &AtomicBool,
) -> Result<u64, TimelineServiceError> {
    let mut cursor = String::new();
    let mut inserted = 0;
    loop {
        ensure_not_cancelled(cancel_token)?;
        let Some(next_cursor) =
            next_macb_source_cursor(conn, timestamp_column, &cursor, MACB_SOURCE_BATCH_SIZE)?
        else {
            break;
        };
        inserted += insert_macb_source_range(
            conn,
            timestamp_column,
            event_type,
            title_prefix,
            description_suffix,
            &cursor,
            &next_cursor,
        )?;
        cursor = next_cursor;
    }
    Ok(inserted)
}

fn next_macb_source_cursor(
    conn: &Connection,
    timestamp_column: &str,
    after_id: &str,
    batch_size: u32,
) -> Result<Option<String>, TimelineServiceError> {
    let sql = format!(
        "SELECT id
         FROM file_entries
         WHERE {timestamp_column} IS NOT NULL
           AND LOWER(entry_type) = 'file'
           AND id > ?1
         ORDER BY id ASC
         LIMIT ?2"
    );
    let mut statement = conn.prepare(&sql)?;
    let mut rows = statement.query(params![after_id, batch_size])?;
    let mut last_id = None;
    while let Some(row) = rows.next()? {
        last_id = Some(row.get(0)?);
    }
    Ok(last_id)
}

#[allow(clippy::too_many_arguments)]
fn insert_macb_source_range(
    conn: &Connection,
    timestamp_column: &str,
    event_type: &str,
    title_prefix: &str,
    description_suffix: &str,
    after_id: &str,
    through_id: &str,
) -> Result<u64, TimelineServiceError> {
    let sql = format!(
        "INSERT OR IGNORE INTO timeline_events
         (id, case_id, source_object_id, event_type, ts, title, description, parser_id, source_attribution, attrs)
         SELECT
            'macb:' || fe.id || ':{event_type}',
            ds.case_id,
            fe.id,
            '{event_type}',
            fe.{timestamp_column},
            ?1 || fe.name,
            fe.path || ?2,
            'timeline.macb',
            '{event_type}',
            '{{}}'
         FROM file_entries fe
         JOIN data_sources ds ON ds.id = fe.data_source_id
         WHERE fe.{timestamp_column} IS NOT NULL
           AND LOWER(fe.entry_type) = 'file'
           AND fe.id > ?3
           AND fe.id <= ?4
           AND NOT EXISTS (
               SELECT 1 FROM timeline_events existing
               WHERE existing.source_object_id = fe.id
                 AND existing.event_type = '{event_type}'
                 AND existing.ts = fe.{timestamp_column}
           )"
    );
    conn.execute(
        &sql,
        params![title_prefix, description_suffix, after_id, through_id],
    )
    .map(|count| count as u64)
    .map_err(|error| {
        TimelineServiceError::Other(format!("Insert {event_type} timeline projection: {error}"))
    })
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), TimelineServiceError> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(TimelineServiceError::Cancelled)
    } else {
        Ok(())
    }
}

fn implicit_projection_identity(conn: &Connection) -> Result<String, TimelineServiceError> {
    let mut hasher = Sha256::new();
    hasher.update(b"timeline-projection-input-v1");
    hash_projection_rows(
        conn,
        "SELECT id, created_at, modified_at, accessed_at, changed_at
         FROM file_entries
         ORDER BY id ASC",
        &mut hasher,
    )?;
    hash_projection_rows(
        conn,
        "SELECT id, source_object_id, ts, parser_id, parser_version
         FROM timeline_events
         WHERE COALESCE(parser_id, '') <> 'timeline.macb'
         ORDER BY id ASC",
        &mut hasher,
    )?;
    Ok(hex::encode(hasher.finalize()))
}

fn hash_projection_rows(
    conn: &Connection,
    sql: &str,
    hasher: &mut Sha256,
) -> Result<(), TimelineServiceError> {
    let mut statement = conn.prepare(sql)?;
    let column_count = statement.column_count();
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        for column in 0..column_count {
            let value = row.get::<_, Option<String>>(column)?.unwrap_or_default();
            hasher.update((value.len() as u64).to_le_bytes());
            hasher.update(value.as_bytes());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/timeline_service/projection.rs"]
mod tests;
