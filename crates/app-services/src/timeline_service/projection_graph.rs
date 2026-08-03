use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use rusqlite::{params, Connection, Rows, Statement};

use super::TimelineServiceError;

const TIMELINE_GRAPH_BATCH: u32 = 20_000;
pub(super) const FIRST_GRAPH_PAGE_SQL: &str = "SELECT id
     FROM timeline_events
     WHERE case_id = ?1
     ORDER BY id ASC
     LIMIT ?2";
pub(super) const NEXT_GRAPH_PAGE_SQL: &str = "SELECT id
     FROM timeline_events
     WHERE case_id = ?1 AND id > ?2
     ORDER BY id ASC
     LIMIT ?3";
const INSERT_GRAPH_NODES_FOR_RANGE_SQL: &str = "INSERT OR REPLACE INTO graph_nodes
     (id, case_id, node_type, label, summary, tags, created_at)
     SELECT id, case_id, 'timeline_event', title, event_type, '[]', ?4
     FROM timeline_events
     WHERE case_id = ?1 AND id >= ?2 AND id <= ?3";
const INSERT_GRAPH_EDGES_FOR_RANGE_SQL: &str = "INSERT OR REPLACE INTO graph_edges
     (id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at)
     SELECT 'references:' || id || ':' || source_object_id,
            case_id, id, source_object_id, 'references', confidence,
            'timeline:' || event_type, ?4
     FROM timeline_events
     WHERE case_id = ?1 AND id >= ?2 AND id <= ?3
       AND source_object_id <> ''";
const INSERT_GRAPH_EDGES_WITH_TARGET_FOR_RANGE_SQL: &str = "INSERT OR REPLACE INTO graph_edges
     (id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at)
     SELECT 'references:' || event.id || ':' || event.source_object_id,
            event.case_id, event.id, event.source_object_id, 'references', event.confidence,
            'timeline:' || event.event_type, ?4
     FROM timeline_events event
     WHERE event.case_id = ?1 AND event.id >= ?2 AND event.id <= ?3
       AND event.source_object_id <> ''
       AND EXISTS (SELECT 1 FROM graph_nodes target WHERE target.id = event.source_object_id)";

pub(super) struct TimelineGraphRow {
    pub(super) id: String,
}

pub(super) fn populate_timeline_event_graph(
    conn: &Connection,
    cancel_token: &AtomicBool,
) -> Result<Vec<String>, TimelineServiceError> {
    populate_timeline_event_graph_with_batch(conn, TIMELINE_GRAPH_BATCH, cancel_token)
}

pub(super) fn populate_timeline_event_graph_with_batch(
    conn: &Connection,
    batch_size: u32,
    cancel_token: &AtomicBool,
) -> Result<Vec<String>, TimelineServiceError> {
    if batch_size == 0 {
        return Err(TimelineServiceError::InvalidInput(
            "timeline graph batch size must be greater than zero".to_string(),
        ));
    }
    let Some(case_id) = resolve_timeline_case_id(conn)? else {
        return Ok(Vec::new());
    };
    let created_at = Utc::now().to_rfc3339();
    let require_existing_target = graph_edges_require_existing_target(conn)?;
    let mut skipped_empty_source = 0;
    let mut cursor: Option<String> = None;

    loop {
        ensure_not_cancelled(cancel_token)?;
        let rows = load_graph_page(conn, &case_id, cursor.as_deref(), batch_size)?;
        if rows.is_empty() {
            break;
        }
        let next_cursor = rows.last().map(|row| row.id.clone()).ok_or_else(|| {
            TimelineServiceError::Other("timeline graph page was empty".to_string())
        })?;
        skipped_empty_source += write_graph_batch(
            conn,
            &rows,
            &case_id,
            &created_at,
            require_existing_target,
            cancel_token,
        )?;
        cursor = Some(next_cursor);
    }

    Ok(graph_warnings(skipped_empty_source))
}

fn resolve_timeline_case_id(conn: &Connection) -> Result<Option<String>, TimelineServiceError> {
    let mut statement = conn
        .prepare("SELECT DISTINCT case_id FROM timeline_events LIMIT 2")
        .map_err(|error| {
            TimelineServiceError::Other(format!("prepare timeline case query: {error}"))
        })?;
    let case_ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    match case_ids.as_slice() {
        [] => Ok(None),
        [case_id] => Ok(Some(case_id.clone())),
        _ => Err(TimelineServiceError::Other(
            "timeline graph projection encountered multiple case IDs in one source database"
                .to_string(),
        )),
    }
}

fn load_graph_page(
    conn: &Connection,
    case_id: &str,
    after_id: Option<&str>,
    batch_size: u32,
) -> Result<Vec<TimelineGraphRow>, TimelineServiceError> {
    match after_id {
        Some(after_id) => {
            let mut statement = conn.prepare(NEXT_GRAPH_PAGE_SQL).map_err(|error| {
                TimelineServiceError::Other(format!("prepare next timeline graph page: {error}"))
            })?;
            load_graph_rows_after(&mut statement, case_id, after_id, batch_size)
        }
        None => {
            let mut statement = conn.prepare(FIRST_GRAPH_PAGE_SQL).map_err(|error| {
                TimelineServiceError::Other(format!("prepare first timeline graph page: {error}"))
            })?;
            load_first_graph_rows(&mut statement, case_id, batch_size)
        }
    }
}

pub(super) fn load_first_graph_rows(
    stmt: &mut Statement<'_>,
    case_id: &str,
    batch_size: u32,
) -> Result<Vec<TimelineGraphRow>, TimelineServiceError> {
    let rows = stmt.query(params![case_id, batch_size]).map_err(|error| {
        TimelineServiceError::Other(format!("query first timeline graph page: {error}"))
    })?;
    collect_graph_rows(rows)
}

pub(super) fn load_graph_rows_after(
    stmt: &mut Statement<'_>,
    case_id: &str,
    after_id: &str,
    batch_size: u32,
) -> Result<Vec<TimelineGraphRow>, TimelineServiceError> {
    let rows = stmt
        .query(params![case_id, after_id, batch_size])
        .map_err(|error| {
            TimelineServiceError::Other(format!(
                "query timeline graph page after {after_id}: {error}"
            ))
        })?;
    collect_graph_rows(rows)
}

fn collect_graph_rows(mut rows: Rows<'_>) -> Result<Vec<TimelineGraphRow>, TimelineServiceError> {
    let mut collected = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| TimelineServiceError::Other(format!("read timeline graph row: {error}")))?
    {
        collected.push(TimelineGraphRow { id: row.get(0)? });
    }
    Ok(collected)
}

fn write_graph_batch(
    conn: &Connection,
    rows: &[TimelineGraphRow],
    case_id: &str,
    created_at: &str,
    require_existing_target: bool,
    cancel_token: &AtomicBool,
) -> Result<u64, TimelineServiceError> {
    let Some(first_id) = rows.first().map(|row| row.id.as_str()) else {
        return Ok(0);
    };
    let last_id = rows.last().map(|row| row.id.as_str()).ok_or_else(|| {
        TimelineServiceError::Other("timeline graph batch lost its cursor".to_string())
    })?;
    let transaction = conn.unchecked_transaction().map_err(|error| {
        TimelineServiceError::Other(format!("begin timeline graph batch: {error}"))
    })?;
    transaction
        .execute(
            INSERT_GRAPH_NODES_FOR_RANGE_SQL,
            params![case_id, first_id, last_id, created_at],
        )
        .map_err(|error| {
            TimelineServiceError::Other(format!("timeline graph node batch insert: {error}"))
        })?;
    ensure_not_cancelled(cancel_token)?;
    let edge_sql = if require_existing_target {
        INSERT_GRAPH_EDGES_WITH_TARGET_FOR_RANGE_SQL
    } else {
        INSERT_GRAPH_EDGES_FOR_RANGE_SQL
    };
    let inserted_edges = transaction
        .execute(edge_sql, params![case_id, first_id, last_id, created_at])
        .map_err(|error| {
            TimelineServiceError::Other(format!("timeline graph edge batch insert: {error}"))
        })? as u64;
    let skipped = rows.len() as u64 - inserted_edges.min(rows.len() as u64);
    transaction.commit().map_err(|error| {
        TimelineServiceError::Other(format!("commit timeline graph batch: {error}"))
    })?;
    Ok(skipped)
}

fn graph_edges_require_existing_target(conn: &Connection) -> Result<bool, TimelineServiceError> {
    let mut statement = conn.prepare("PRAGMA foreign_key_list('graph_edges')")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let table: String = row.get(2)?;
        let from: String = row.get(3)?;
        if table.eq_ignore_ascii_case("graph_nodes") && from.eq_ignore_ascii_case("target_id") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), TimelineServiceError> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(TimelineServiceError::Cancelled)
    } else {
        Ok(())
    }
}

fn graph_warnings(skipped_empty_source: u64) -> Vec<String> {
    if skipped_empty_source == 0 {
        Vec::new()
    } else {
        vec![format!(
            "{skipped_empty_source} timeline event(s) skipped because the source object was empty or not materialized as a graph node; no References edges were created"
        )]
    }
}
