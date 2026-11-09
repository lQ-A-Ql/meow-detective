use super::{projection::TimelineProjectionStats, TimelineServiceError, TimelineWriteTimings};
use domain::{DataSourcePlatform, EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_fjall::{
    CatalogRecordStore, PersistMode, ScanDirection, TimelineRecord, TimelineRecordStore,
};
use persistence_sqlite::repositories::graph_repo::GraphRepo;
use rusqlite::Connection;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

const FILE_KEY: &str = "file_activity_v2";
const GRAPH_KEY: &str = "timeline_graph_v3";

pub(super) fn identity(
    conn: &Connection,
    platform: DataSourcePlatform,
) -> Result<String, TimelineServiceError> {
    use sha2::Digest;
    let store = persistence_sqlite::source_fjall::open_store(conn)?
        .ok_or_else(|| TimelineServiceError::Other("source generation is missing".into()))?;
    let source = persistence_sqlite::source_fjall::source_id(conn)?
        .ok_or_else(|| TimelineServiceError::Other("source identity is missing".into()))?;
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut hasher = sha2::Sha256::new();
    hasher.update(platform.as_storage_str());
    let mut cursor = None;
    loop {
        let page = catalog.list_all(
            &snapshot,
            &source,
            cursor.as_deref(),
            ScanDirection::Forward,
            512,
        )?;
        for record in page.records {
            let bytes = serde_json::to_vec(&record)
                .map_err(|error| TimelineServiceError::Other(error.to_string()))?;
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(format!(
                "file-activity-v2:{}",
                hex::encode(hasher.finalize())
            ));
        }
    }
}

pub(super) fn materialize(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel: &AtomicBool,
    identity: &str,
) -> Result<TimelineProjectionStats, TimelineServiceError> {
    let store = persistence_sqlite::source_fjall::open_store(conn)?
        .ok_or_else(|| TimelineServiceError::Other("source generation is missing".into()))?;
    let source = persistence_sqlite::source_fjall::source_id(conn)?
        .ok_or_else(|| TimelineServiceError::Other("source identity is missing".into()))?;
    let meta =
        persistence_sqlite::repositories::timeline_projection_repo::TimelineProjectionRepo::new(
            conn,
        );
    meta.ensure_meta_table()?;
    let started = Instant::now();
    let file_done = meta.is_done(FILE_KEY, identity)?;
    let graph_done = meta.is_done(GRAPH_KEY, identity)?;
    if file_done && graph_done {
        return Ok(TimelineProjectionStats {
            already_projected: true,
            graph_complete: true,
            ..Default::default()
        });
    }
    let timeline = TimelineRecordStore::from_source_store((*store).clone());
    let count = if file_done {
        0
    } else {
        stream_file_events(
            &timeline,
            CatalogRecordStore::from_source_store((*store).clone()),
            &source,
            platform,
            cancel,
            identity,
            &meta,
        )?
    };
    let events_elapsed_ms = started.elapsed().as_millis();
    let graph_started = Instant::now();
    let mut timings = TimelineWriteTimings::default();
    if !graph_done {
        project_graph(conn, &timeline, &source, cancel, &mut timings)?;
        meta.mark_done(GRAPH_KEY, 0, identity)?;
    }
    Ok(TimelineProjectionStats {
        inserted_count: count,
        elapsed_ms: started.elapsed().as_millis(),
        events_elapsed_ms,
        graph_elapsed_ms: graph_started.elapsed().as_millis(),
        timings,
        already_projected: false,
        graph_complete: true,
        warnings: Vec::new(),
    })
}

fn stream_file_events(
    timeline: &TimelineRecordStore,
    catalog: CatalogRecordStore,
    source: &str,
    platform: DataSourcePlatform,
    cancel: &AtomicBool,
    identity: &str,
    meta: &persistence_sqlite::repositories::timeline_projection_repo::TimelineProjectionRepo<'_>,
) -> Result<u64, TimelineServiceError> {
    let snapshot = catalog.read_snapshot();
    let mut cursor = None;
    let mut complete = false;
    let mut next_batch = || {
        if complete {
            return Ok(None);
        }
        loop {
            ensure_active(cancel)?;
            let page = catalog.list_all(
                &snapshot,
                source,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )?;
            let mut events = Vec::new();
            for record in page.records {
                if eligible(&record, platform) {
                    events.extend(file_events_for_record(record, source, platform));
                }
            }
            cursor = page.next_cursor;
            if !events.is_empty() {
                complete = cursor.is_none();
                return Ok(Some(events));
            }
            if cursor.is_none() {
                complete = true;
                return Ok(None);
            }
        }
    };
    let count = timeline
        .replace_by_parser_prefix_streamed(
            source,
            "timeline.file_",
            PersistMode::SyncAll,
            &mut next_batch,
        )
        .map_err(|error| match error {
            persistence_fjall::TimelineStreamError::Storage(error) => {
                TimelineServiceError::Storage(error)
            }
            persistence_fjall::TimelineStreamError::Producer(error) => error,
        })?;
    ensure_active(cancel)?;
    meta.mark_done(FILE_KEY, count, identity)?;
    Ok(count)
}

fn file_events_for_record(
    record: persistence_fjall::FileEntryRecord,
    source: &str,
    platform: DataSourcePlatform,
) -> Vec<TimelineRecord> {
    let file = crate::file_service::record_to_domain(record);
    timeline::project_file_activity(&file)
        .into_iter()
        .map(|mut event| {
            event.attrs.insert(
                "platform".into(),
                serde_json::Value::String(platform.as_storage_str().into()),
            );
            TimelineRecord {
                id: event.id.0,
                data_source_id: source.into(),
                source_object_id: event.source_object_id,
                event_type: event.event_type,
                timestamp: event.timestamp,
                title: event.title,
                description: event.description,
                parser_id: event.parser_id,
                parser_version: event.parser_version,
                confidence: event.confidence,
                source_attribution: event.source_attribution,
                attrs: event.attrs,
            }
        })
        .collect()
}

fn eligible(record: &persistence_fjall::FileEntryRecord, platform: DataSourcePlatform) -> bool {
    if record.entry_type != persistence_fjall::CatalogEntryType::File {
        return false;
    }
    if platform != DataSourcePlatform::Windows {
        return !record.read_only;
    }
    if record.system || record.name.starts_with('$') {
        return false;
    }
    let path = crate::analysis_service::normalize_evidence_path(&record.path).to_ascii_lowercase();
    ![
        "windows/",
        "program files/",
        "program files (x86)/",
        "programdata/",
        "system volume information/",
    ]
    .iter()
    .any(|prefix| path.trim_start_matches('/').starts_with(prefix))
}

fn project_graph(
    conn: &Connection,
    timeline: &TimelineRecordStore,
    source: &str,
    cancel: &AtomicBool,
    timings: &mut TimelineWriteTimings,
) -> Result<(), TimelineServiceError> {
    let case_id: String = conn.query_row(
        "SELECT case_id FROM data_sources WHERE id = ?1",
        [source],
        |row| row.get(0),
    )?;
    let created = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM graph_edges WHERE source_id IN (SELECT id FROM graph_nodes WHERE case_id = ?1 AND node_type = 'timeline_event')",[&case_id])?;
    tx.execute(
        "DELETE FROM graph_nodes WHERE case_id = ?1 AND node_type = 'timeline_event'",
        [&case_id],
    )?;
    let graph = GraphRepo::new(&tx);
    let snapshot = timeline.read_snapshot();
    let mut cursor = None;
    loop {
        ensure_active(cancel)?;
        let page = timeline.list_by_source(
            &snapshot,
            source,
            cursor.as_deref(),
            ScanDirection::Forward,
            512,
        )?;
        timings.graph_buffer_rows = timings.graph_buffer_rows.max(page.records.len() as u64);
        let mut nodes = Vec::with_capacity(page.records.len());
        let mut edges = Vec::with_capacity(page.records.len());
        for event in page.records {
            nodes.push(GraphNode {
                id: event.id.clone(),
                case_id: case_id.clone(),
                node_type: NodeType::TimelineEvent,
                label: event.title,
                summary: event.event_type.clone(),
                tags: Vec::new(),
                created_at: created.clone(),
            });
            if !event.source_object_id.is_empty() {
                edges.push(GraphEdge {
                    id: format!("references:{}:{}", event.id, event.source_object_id),
                    case_id: case_id.clone(),
                    source_id: event.id,
                    target_id: event.source_object_id,
                    edge_type: EdgeType::References,
                    confidence: event.confidence.map(f64::from),
                    provenance: Some(format!("timeline:{}", event.event_type)),
                    created_at: created.clone(),
                });
            }
        }
        graph.insert_nodes_in_transaction(&nodes)?;
        graph.insert_edges_in_transaction(&edges)?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    ensure_active(cancel)?;
    tx.commit()?;
    Ok(())
}

fn ensure_active(cancel: &AtomicBool) -> Result<(), TimelineServiceError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(TimelineServiceError::Cancelled);
    }
    Ok(())
}
