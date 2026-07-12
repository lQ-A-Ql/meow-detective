use chrono::Utc;
use domain::{EdgeType, FileEntry, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::repositories::{graph_repo::GraphRepo, timeline_repo::TimelineRepo};
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::time::Instant;

use super::TimelineServiceError;

const MACB_PROJECTION_KEY: &str = "macb";
const TIMELINE_GRAPH_BATCH: u32 = 5000;
const GRAPH_WRITE_CHUNK: usize = 2000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineProjectionStats {
    pub inserted_count: u64,
    pub elapsed_ms: u128,
    pub already_projected: bool,
    pub warnings: Vec<String>,
}

struct TimelineGraphRow {
    id: String,
    source_object_id: String,
    event_type: String,
    title: String,
    confidence: Option<f64>,
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
    if !timeline_projection_source_tables_present(conn)? {
        return Ok(already_projected_stats());
    }
    ensure_projection_meta_table(conn)?;
    if is_projection_done(conn, MACB_PROJECTION_KEY)? {
        return Ok(already_projected_stats());
    }

    let started = Instant::now();
    let inserted = project_macb_timeline_sql(conn)?;
    mark_projection_done(conn, MACB_PROJECTION_KEY, inserted)?;
    let warnings = populate_graph_non_fatal(conn, inserted);

    Ok(TimelineProjectionStats {
        inserted_count: inserted,
        elapsed_ms: started.elapsed().as_millis(),
        already_projected: false,
        warnings,
    })
}

fn already_projected_stats() -> TimelineProjectionStats {
    TimelineProjectionStats {
        already_projected: true,
        ..TimelineProjectionStats::default()
    }
}

fn populate_graph_non_fatal(conn: &Connection, inserted: u64) -> Vec<String> {
    if inserted == 0 {
        return Vec::new();
    }
    match populate_timeline_event_graph(conn) {
        Ok(warnings) => warnings,
        Err(error) => {
            let message = format!("Timeline graph population failed: {error}");
            tracing::warn!("{message}");
            vec![message]
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

fn ensure_projection_meta_table(conn: &Connection) -> Result<(), TimelineServiceError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS timeline_projection_meta (
            projection_key TEXT PRIMARY KEY NOT NULL,
            status TEXT NOT NULL,
            inserted_count INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;
    Ok(())
}

fn is_projection_done(conn: &Connection, key: &str) -> Result<bool, TimelineServiceError> {
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM timeline_projection_meta WHERE projection_key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?;
    Ok(status.as_deref() == Some("done"))
}

fn mark_projection_done(
    conn: &Connection,
    key: &str,
    inserted_count: u64,
) -> Result<(), TimelineServiceError> {
    conn.execute(
        "INSERT INTO timeline_projection_meta (projection_key, status, inserted_count, updated_at)
         VALUES (?1, 'done', ?2, datetime('now'))
         ON CONFLICT(projection_key) DO UPDATE SET
            status = excluded.status,
            inserted_count = excluded.inserted_count,
            updated_at = excluded.updated_at",
        params![key, inserted_count as i64],
    )?;
    Ok(())
}

fn project_macb_timeline_sql(conn: &Connection) -> Result<u64, TimelineServiceError> {
    let tx = conn.unchecked_transaction().map_err(|error| {
        TimelineServiceError::Other(format!("Begin MACB timeline projection: {error}"))
    })?;
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
        inserted +=
            insert_macb_kind_sql(&tx, column, event_type, title_prefix, description_suffix)?;
    }
    tx.commit().map_err(|error| {
        TimelineServiceError::Other(format!("Commit MACB timeline projection: {error}"))
    })?;
    Ok(inserted)
}

fn insert_macb_kind_sql(
    conn: &Connection,
    timestamp_column: &str,
    event_type: &str,
    title_prefix: &str,
    description_suffix: &str,
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
           AND NOT EXISTS (
               SELECT 1 FROM timeline_events existing
               WHERE existing.source_object_id = fe.id
                 AND existing.event_type = '{event_type}'
                 AND existing.ts = fe.{timestamp_column}
           )"
    );
    conn.execute(&sql, params![title_prefix, description_suffix])
        .map(|count| count as u64)
        .map_err(|error| {
            TimelineServiceError::Other(format!("Insert {event_type} timeline projection: {error}"))
        })
}

fn populate_timeline_event_graph(conn: &Connection) -> Result<Vec<String>, TimelineServiceError> {
    let case_id = resolve_timeline_case_id(conn)?;
    let graph_repo = GraphRepo::new(conn);
    let created_at = Utc::now().to_rfc3339();
    let mut skipped_empty_source = 0;
    let mut offset = 0;

    loop {
        let rows = load_graph_rows(conn, &case_id, offset)?;
        if rows.is_empty() {
            break;
        }
        let row_count = rows.len() as u64;
        let (nodes, edges, skipped) = build_graph_records(&rows, &case_id, &created_at);
        write_graph_records(&graph_repo, &nodes, &edges)?;
        skipped_empty_source += skipped;
        offset += row_count;
    }

    Ok(graph_warnings(skipped_empty_source))
}

fn resolve_timeline_case_id(conn: &Connection) -> Result<String, TimelineServiceError> {
    conn.query_row(
        "SELECT DISTINCT case_id FROM timeline_events LIMIT 1",
        [],
        |row| row.get(0),
    )
    .map_err(|error| {
        TimelineServiceError::Other(format!("resolve case_id for timeline graph: {error}"))
    })
}

fn load_graph_rows(
    conn: &Connection,
    case_id: &str,
    offset: u64,
) -> Result<Vec<TimelineGraphRow>, TimelineServiceError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, source_object_id, event_type, title, confidence
             FROM timeline_events
             WHERE case_id = ?1
             ORDER BY id ASC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| {
            TimelineServiceError::Other(format!("prepare timeline graph query: {error}"))
        })?;
    let rows = stmt
        .query_map(params![case_id, TIMELINE_GRAPH_BATCH, offset], |row| {
            Ok(TimelineGraphRow {
                id: row.get(0)?,
                source_object_id: row.get(1)?,
                event_type: row.get(2)?,
                title: row.get(3)?,
                confidence: row.get(4)?,
            })
        })
        .map_err(|error| {
            TimelineServiceError::Other(format!("query timeline events for graph: {error}"))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| TimelineServiceError::Other(format!("collect timeline rows: {error}")))?;
    Ok(rows)
}

fn build_graph_records(
    rows: &[TimelineGraphRow],
    case_id: &str,
    created_at: &str,
) -> (Vec<GraphNode>, Vec<GraphEdge>, u64) {
    let mut nodes = Vec::with_capacity(rows.len());
    let mut edges = Vec::with_capacity(rows.len());
    let mut skipped = 0;
    for row in rows {
        nodes.push(GraphNode {
            id: row.id.clone(),
            case_id: case_id.to_string(),
            node_type: NodeType::TimelineEvent,
            label: row.title.clone(),
            summary: row.event_type.clone(),
            tags: Vec::new(),
            created_at: created_at.to_string(),
        });
        if row.source_object_id.is_empty() {
            skipped += 1;
            continue;
        }
        edges.push(GraphEdge {
            id: format!("references:{}:{}", row.id, row.source_object_id),
            case_id: case_id.to_string(),
            source_id: row.id.clone(),
            target_id: row.source_object_id.clone(),
            edge_type: EdgeType::References,
            confidence: row.confidence,
            provenance: Some(format!("timeline.macb:{}", row.event_type)),
            created_at: created_at.to_string(),
        });
    }
    (nodes, edges, skipped)
}

fn write_graph_records(
    repo: &GraphRepo<'_>,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
) -> Result<(), TimelineServiceError> {
    for chunk in nodes.chunks(GRAPH_WRITE_CHUNK) {
        repo.insert_nodes_batch(chunk).map_err(|error| {
            TimelineServiceError::Other(format!("timeline graph node insert: {error}"))
        })?;
    }
    for chunk in edges.chunks(GRAPH_WRITE_CHUNK) {
        repo.insert_edges_batch(chunk).map_err(|error| {
            TimelineServiceError::Other(format!("timeline graph edge insert: {error}"))
        })?;
    }
    Ok(())
}

fn graph_warnings(skipped_empty_source: u64) -> Vec<String> {
    if skipped_empty_source == 0 {
        Vec::new()
    } else {
        vec![format!(
            "{skipped_empty_source} timeline event(s) skipped because source_object_id was empty; no References edges were created"
        )]
    }
}
