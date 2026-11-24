use persistence_fjall::{DatabaseConfig, PersistMode, SourceStore, TimelineRecordStore};
use serde_json::json;
use std::{path::PathBuf, time::Instant};

#[test]
#[ignore = "requires explicit retained generation, source ID and workspace diagnostic output"]
fn retained_timeline_snapshot_memory() {
    let generation = PathBuf::from(std::env::var_os("FORENSICS_TIMELINE_GENERATION").unwrap());
    let output = PathBuf::from(std::env::var_os("FORENSICS_PERF_OUTPUT").unwrap())
        .canonicalize()
        .unwrap();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    assert!(output.starts_with(workspace));
    let source = std::env::var("FORENSICS_ANALYSIS_SOURCE_ID").unwrap();
    let mode = std::env::var("FORENSICS_MEMORY_SNAPSHOT").unwrap();
    assert!(matches!(mode.as_str(), "held" | "released"));
    let original = TimelineRecordStore::from_source_store(
        SourceStore::open(generation, DatabaseConfig::default()).unwrap(),
    );
    let original_snapshot = original.read_snapshot();
    let target =
        TimelineRecordStore::open(output.join("target"), DatabaseConfig::default()).unwrap();
    let mut retained = (mode == "held").then(|| target.read_snapshot());
    let started = Instant::now();
    let mut cursor = None;
    let mut rows = 0usize;
    let mut samples = Vec::new();
    while rows < 524_288 {
        let page = original
            .scan_by_source(&original_snapshot, &source, cursor.as_deref(), 4096)
            .unwrap();
        assert!(!page.records.is_empty());
        target
            .upsert_batch(&page.records, PersistMode::Buffer)
            .unwrap();
        rows += page.records.len();
        cursor = page.next_cursor;
        if rows.is_multiple_of(65_536) {
            let metrics = target.source_store().database().memory_metrics();
            let versions = metrics
                .keyspaces
                .iter()
                .map(|entry| {
                    json!({"keyspace":entry.keyspace.as_str(),
                    "activeBytes":entry.active_memtable_bytes,
                    "sealedMemtables":entry.sealed_memtables,
                    "retainedVersions":entry.retained_versions})
                })
                .collect::<Vec<_>>();
            let sample = json!({"rows":rows,"elapsedMs":started.elapsed().as_millis(),
                "snapshotHeld":retained.is_some(),"writeBufferBytes":metrics.write_buffer_bytes,
                "outstandingFlushes":metrics.outstanding_flushes,
                "activeCompactions":metrics.active_compactions,"keyspaces":versions});
            eprintln!("TIMELINE_MEMORY {sample}");
            samples.push(sample);
        }
        if rows == 393_216 {
            drop(retained.take());
        }
        if cursor.is_none() {
            break;
        }
    }
    let report = json!({"mode":mode,"rows":rows,"elapsedMs":started.elapsed().as_millis(),
        "samples":samples});
    std::fs::write(
        output.join("memory.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        rows >= 524_288,
        "diagnostic requires enough retained evidence records"
    );
}
