use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_source::fjall::{
    encode_key, DatabaseConfig, KeyPart, KeyspaceName, ScanDirection, SourceStore, TimelineRecord,
    TimelineRecordStore, VersionedEnvelope,
};
use persistence_sqlite::repositories::graph_repo::GraphRepo;
use serde_json::json;
use std::{path::PathBuf, time::Instant};

#[test]
#[ignore = "requires explicit retained generation, source ID, copied control DB and workspace output"]
fn retained_timeline_graph_diagnostic() {
    let generation = required_path("FORENSICS_TIMELINE_GENERATION");
    let database = required_path("FORENSICS_TIMELINE_CONTROL_DB");
    let output = required_path("FORENSICS_PERF_OUTPUT");
    let source = std::env::var("FORENSICS_ANALYSIS_SOURCE_ID").unwrap();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    assert!(output.starts_with(&workspace));
    assert!(
        database.starts_with(&output),
        "SQL writes require a diagnostic copy"
    );
    let started = Instant::now();
    eprintln!("TIMELINE_DIAGNOSTIC_OPEN");
    let store = SourceStore::open(&generation, DatabaseConfig::default()).unwrap();
    let open_ms = started.elapsed().as_millis();
    let timeline = TimelineRecordStore::from_source_store(store);
    let snapshot = timeline.read_snapshot();
    let connection = persistence_sqlite::open_existing_source(&database).unwrap();
    connection
        .execute_batch("PRAGMA cache_size=-32768; PRAGMA temp_store=MEMORY;")
        .unwrap();
    let case: String = connection
        .query_row(
            "SELECT case_id FROM data_sources WHERE id=?1",
            [&source],
            |row| row.get(0),
        )
        .unwrap();
    let transaction = connection.unchecked_transaction().unwrap();
    let graph = GraphRepo::new(&transaction);
    let mut cursor = None;
    let mut rows = 0u64;
    let mut read_time = std::time::Duration::ZERO;
    let mut nodes_time = std::time::Duration::ZERO;
    let mut edges_time = std::time::Duration::ZERO;
    let mut reports = Vec::new();
    let created = chrono::Utc::now().to_rfc3339();
    persistence_source::fjall::instrumentation::enable();
    persistence_source::fjall::instrumentation::reset();
    for page_index in 0..128 {
        let read_started = Instant::now();
        let page = timeline
            .list_by_source(
                &snapshot,
                &source,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .unwrap();
        let read_elapsed = read_started.elapsed();
        read_time += read_elapsed;
        let mut nodes = Vec::with_capacity(page.records.len());
        let mut edges = Vec::with_capacity(page.records.len());
        for record in page.records {
            nodes.push(GraphNode {
                id: record.id.clone(),
                case_id: case.clone(),
                node_type: NodeType::TimelineEvent,
                label: record.title,
                summary: record.event_type.clone(),
                tags: Vec::new(),
                created_at: created.clone(),
            });
            edges.push(GraphEdge {
                id: format!("references:{}:{}", record.id, record.source_object_id),
                case_id: case.clone(),
                source_id: record.id,
                target_id: record.source_object_id,
                edge_type: EdgeType::References,
                confidence: record.confidence.map(f64::from),
                provenance: Some(format!("timeline:{}", record.event_type)),
                created_at: created.clone(),
            });
        }
        let nodes_started = Instant::now();
        graph.insert_nodes_in_transaction(&nodes).unwrap();
        let nodes_elapsed = nodes_started.elapsed();
        nodes_time += nodes_elapsed;
        let edges_started = Instant::now();
        graph.insert_edges_in_transaction(&edges).unwrap();
        let edges_elapsed = edges_started.elapsed();
        edges_time += edges_elapsed;
        rows += nodes.len() as u64;
        reports.push(json!({"page":page_index, "rows":nodes.len(),
            "readMicros":read_elapsed.as_micros(), "nodeWriteMicros":nodes_elapsed.as_micros(),
            "edgeWriteMicros":edges_elapsed.as_micros()}));
        if page_index % 16 == 0 {
            eprintln!(
                "TIMELINE_DIAGNOSTIC rows={rows} readMs={} nodesMs={} edgesMs={}",
                read_time.as_millis(),
                nodes_time.as_millis(),
                edges_time.as_millis()
            );
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let commit_started = Instant::now();
    transaction.commit().unwrap();
    let storage = persistence_source::fjall::instrumentation::snapshot();
    persistence_source::fjall::instrumentation::disable();
    let commit_ms = commit_started.elapsed().as_millis();
    let sequential_started = Instant::now();
    let prefix = encode_key("timeline.record", &[KeyPart::Text(source.clone())]).unwrap();
    let mut after = None;
    let mut sequential_rows = 0u64;
    for _ in 0..128 {
        let page = snapshot
            .keyset(
                KeyspaceName::TimelineRecords,
                &prefix,
                after.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .unwrap();
        for entry in &page {
            let envelope = VersionedEnvelope::decode(&entry.value).unwrap();
            let record: TimelineRecord = serde_json::from_slice(&envelope.payload).unwrap();
            assert_eq!(record.data_source_id, source);
        }
        sequential_rows += page.len() as u64;
        after = page.last().map(|entry| entry.key.clone());
        if page.len() < 512 {
            break;
        }
    }
    assert_eq!(sequential_rows, rows);
    let report = json!({"sourceId":source, "generation":generation, "copiedDatabase":database,
        "openMs":open_ms, "rows":rows, "readMs":read_time.as_millis(),
        "nodeWriteMs":nodes_time.as_millis(), "edgeWriteMs":edges_time.as_millis(),
        "commitMs":commit_ms,
        "sequentialRecordReadMs":sequential_started.elapsed().as_millis(),
        "sequentialRecordRows":sequential_rows,
        "totalMs":started.elapsed().as_millis(), "pages":reports,
        "storage":{"pointReads":storage.point_reads, "scanCalls":storage.scan_calls,
            "scanEntries":storage.scan_entries, "bytesRead":storage.bytes_read}});
    std::fs::write(
        output.join("timeline-graph-diagnostic.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        rows > 0,
        "retained timeline must contain actual evidence records"
    );
}

fn required_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap())
        .canonicalize()
        .unwrap()
}
