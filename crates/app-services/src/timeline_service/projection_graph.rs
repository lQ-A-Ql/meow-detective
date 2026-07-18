use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use rusqlite::{params, CachedStatement, Connection, Rows, Statement};

use super::TimelineServiceError;

const TIMELINE_GRAPH_BATCH: u32 = 5000;
pub(super) const FIRST_GRAPH_PAGE_SQL: &str =
    "SELECT id, source_object_id, event_type, title, confidence
     FROM timeline_events
     WHERE case_id = ?1
     ORDER BY id ASC
     LIMIT ?2";
pub(super) const NEXT_GRAPH_PAGE_SQL: &str =
    "SELECT id, source_object_id, event_type, title, confidence
     FROM timeline_events
     WHERE case_id = ?1 AND id > ?2
     ORDER BY id ASC
     LIMIT ?3";
const INSERT_GRAPH_NODE_SQL: &str = "INSERT OR REPLACE INTO graph_nodes
     (id, case_id, node_type, label, summary, tags, created_at)
     VALUES (?1, ?2, 'timeline_event', ?3, ?4, '[]', ?5)";
const INSERT_GRAPH_EDGE_SQL: &str = "INSERT OR REPLACE INTO graph_edges
     (id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at)
     VALUES (?1, ?2, ?3, ?4, 'references', ?5, ?6, ?7)";
const INSERT_GRAPH_EDGE_WITH_TARGET_SQL: &str = "INSERT OR REPLACE INTO graph_edges
     (id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at)
     SELECT ?1, ?2, ?3, ?4, 'references', ?5, ?6, ?7
     WHERE EXISTS (SELECT 1 FROM graph_nodes WHERE id = ?4)";

pub(super) struct TimelineGraphRow {
    pub(super) id: String,
    source_object_id: String,
    event_type: String,
    title: String,
    confidence: Option<f64>,
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
        collected.push(TimelineGraphRow {
            id: row.get(0)?,
            source_object_id: row.get(1)?,
            event_type: row.get(2)?,
            title: row.get(3)?,
            confidence: row.get(4)?,
        });
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
    let transaction = conn.unchecked_transaction().map_err(|error| {
        TimelineServiceError::Other(format!("begin timeline graph batch: {error}"))
    })?;
    let skipped = {
        let mut node_insert =
            transaction
                .prepare_cached(INSERT_GRAPH_NODE_SQL)
                .map_err(|error| {
                    TimelineServiceError::Other(format!(
                        "prepare timeline graph node insert: {error}"
                    ))
                })?;
        let edge_sql = if require_existing_target {
            INSERT_GRAPH_EDGE_WITH_TARGET_SQL
        } else {
            INSERT_GRAPH_EDGE_SQL
        };
        let mut edge_insert = transaction.prepare_cached(edge_sql).map_err(|error| {
            TimelineServiceError::Other(format!("prepare timeline graph edge insert: {error}"))
        })?;
        write_graph_rows(
            rows,
            case_id,
            created_at,
            require_existing_target,
            cancel_token,
            &mut node_insert,
            &mut edge_insert,
        )?
    };
    transaction.commit().map_err(|error| {
        TimelineServiceError::Other(format!("commit timeline graph batch: {error}"))
    })?;
    Ok(skipped)
}

fn write_graph_rows(
    rows: &[TimelineGraphRow],
    case_id: &str,
    created_at: &str,
    require_existing_target: bool,
    cancel_token: &AtomicBool,
    node_insert: &mut CachedStatement<'_>,
    edge_insert: &mut CachedStatement<'_>,
) -> Result<u64, TimelineServiceError> {
    let mut skipped = 0;
    for row in rows {
        ensure_not_cancelled(cancel_token)?;
        node_insert
            .execute(params![
                row.id,
                case_id,
                row.title,
                row.event_type,
                created_at
            ])
            .map_err(|error| {
                TimelineServiceError::Other(format!("timeline graph node insert: {error}"))
            })?;
        if row.source_object_id.is_empty() {
            skipped += 1;
            continue;
        }
        let inserted = edge_insert
            .execute(params![
                format!("references:{}:{}", row.id, row.source_object_id),
                case_id,
                row.id,
                row.source_object_id,
                row.confidence,
                format!("timeline.macb:{}", row.event_type),
                created_at
            ])
            .map_err(|error| {
                TimelineServiceError::Other(format!("timeline graph edge insert: {error}"))
            })?;
        if require_existing_target && inserted == 0 {
            skipped += 1;
        }
    }
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
