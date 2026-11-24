use super::{
    TimelineProgressCallback, TimelineProjectionError, TimelineWork, TimelineWriteTimings,
};
use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_source::fjall::{ScanDirection, TimelineRecord, TimelineRecordStore};
use persistence_sqlite::repositories::graph_repo::GraphRepo;
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn project(
    conn: &Connection,
    timeline: &TimelineRecordStore,
    source: &str,
    cancel: &AtomicBool,
    timings: &mut TimelineWriteTimings,
    progress: Option<&TimelineProgressCallback<'_>>,
) -> Result<(), TimelineProjectionError> {
    let previous: i64 = conn.pragma_query_value(None, "cache_size", |row| row.get(0))?;
    conn.pragma_update(
        None,
        "cache_size",
        -super::workspace::GRAPH_SQLITE_CACHE_KIB,
    )?;
    let result = write_graph(conn, timeline, source, cancel, timings, progress);
    conn.pragma_update(None, "cache_size", previous)?;
    result
}

fn write_graph(
    conn: &Connection,
    timeline: &TimelineRecordStore,
    source: &str,
    cancel: &AtomicBool,
    timings: &mut TimelineWriteTimings,
    progress: Option<&TimelineProgressCallback<'_>>,
) -> Result<(), TimelineProjectionError> {
    if let Some(progress) = progress {
        progress(TimelineWork::TimelineGraphPrepare, 0, None);
    }
    let case: String = conn.query_row(
        "SELECT case_id FROM data_sources WHERE id = ?1",
        [source],
        |row| row.get(0),
    )?;
    let created = chrono::Utc::now().to_rfc3339();
    conn.execute("DELETE FROM graph_edges WHERE source_id IN (SELECT id FROM graph_nodes WHERE case_id = ?1 AND node_type = 'timeline_event')",[&case])?;
    conn.execute(
        "DELETE FROM graph_nodes WHERE case_id = ?1 AND node_type = 'timeline_event'",
        [&case],
    )?;
    let graph = GraphRepo::new(conn);
    let snapshot = timeline.read_snapshot();
    let total = timeline.count_by_time(&snapshot, source, None, None)?;
    let mut processed = 0;
    if let Some(progress) = progress {
        progress(TimelineWork::TimelineGraph, 0, Some(total));
    }
    let mut cursor = None;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(TimelineProjectionError::Cancelled);
        }
        let page = timeline.list_by_source(
            &snapshot,
            source,
            cursor.as_deref(),
            ScanDirection::Forward,
            512,
        )?;
        timings.graph_buffer_rows = timings.graph_buffer_rows.max(page.records.len() as u64);
        let (nodes, edges) = graph_records(page.records, &case, &created);
        graph.insert_nodes_in_transaction(&nodes)?;
        graph.insert_edges_in_transaction(&edges)?;
        processed += nodes.len() as u64;
        if let Some(progress) = progress {
            progress(TimelineWork::TimelineGraph, processed, Some(total));
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    if cancel.load(Ordering::Acquire) {
        return Err(TimelineProjectionError::Cancelled);
    }
    Ok(())
}

fn graph_records(
    events: Vec<TimelineRecord>,
    case: &str,
    created: &str,
) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes = Vec::with_capacity(events.len());
    let mut edges = Vec::with_capacity(events.len());
    for event in events {
        nodes.push(GraphNode {
            id: event.id.clone(),
            case_id: case.into(),
            node_type: NodeType::TimelineEvent,
            label: event.title,
            summary: event.event_type.clone(),
            tags: Vec::new(),
            created_at: created.into(),
        });
        if !event.source_object_id.is_empty() {
            edges.push(GraphEdge {
                id: format!("references:{}:{}", event.id, event.source_object_id),
                case_id: case.into(),
                source_id: event.id,
                target_id: event.source_object_id,
                edge_type: EdgeType::References,
                confidence: event.confidence.map(f64::from),
                provenance: Some(format!("timeline:{}", event.event_type)),
                created_at: created.into(),
            });
        }
    }
    (nodes, edges)
}
