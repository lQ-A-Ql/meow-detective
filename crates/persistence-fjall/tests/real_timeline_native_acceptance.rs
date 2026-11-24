use persistence_fjall::{DatabaseConfig, PersistMode, SourceStore, TimelineRecordStore};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Instant};

#[test]
#[ignore = "requires explicit retained generation, source ID and workspace diagnostic output"]
fn retained_timeline_parallel_acceptance() {
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
    let original = TimelineRecordStore::from_source_store(
        SourceStore::open(generation, DatabaseConfig::default()).unwrap(),
    );
    let snapshot = original.read_snapshot();
    let target =
        TimelineRecordStore::open(output.join("target"), DatabaseConfig::default()).unwrap();
    let started = Instant::now();
    let mut rows = 0usize;
    let mut cursor = None;
    let mut expected_digest = Sha256::new();
    let mut expected_types = std::collections::BTreeMap::<String, u64>::new();
    let producer = || -> Result<_, std::io::Error> {
        if rows >= 524_288 {
            eprintln!("NATIVE_WRITE_START {}", chrono::Utc::now().to_rfc3339());
            return Ok(None);
        }
        let page = original
            .scan_by_source(&snapshot, &source, cursor.as_deref(), 4096)
            .unwrap();
        assert!(!page.records.is_empty());
        rows += page.records.len();
        for record in &page.records {
            expected_digest.update(serde_json::to_vec(record).unwrap());
            *expected_types.entry(record.event_type.clone()).or_default() += 1;
        }
        cursor = page.next_cursor;
        Ok(Some(page.records))
    };
    let outcome = target
        .replace_by_parser_prefix_parallel(
            &source,
            "timeline.",
            "retained-evidence-memory",
            PersistMode::Buffer,
            persistence_fjall::SegmentLimits::default_limits(),
            Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            producer,
        )
        .unwrap();
    eprintln!("NATIVE_WRITE_COMPLETE {}", chrono::Utc::now().to_rfc3339());
    let metrics = target.source_store().database().memory_metrics();
    let retained_versions = metrics
        .keyspaces
        .iter()
        .filter(|entry| {
            matches!(
                entry.keyspace,
                persistence_fjall::KeyspaceName::TimelineRecords
                    | persistence_fjall::KeyspaceName::TimelineIndexes
            )
        })
        .map(|entry| entry.retained_versions)
        .max()
        .unwrap();
    let write_elapsed_ms = started.elapsed().as_millis();
    assert_eq!(outcome.inserted_records, 524_288);
    assert_eq!(outcome.native_writes.configured_workers, 8);
    assert!(outcome.native_writes.workers_used >= 4);
    assert!(outcome.native_writes.peak_native_calls >= 4);
    assert!(
        retained_versions <= 4,
        "obsolete versions must be reclaimable"
    );
    assert_eq!(
        target
            .count_by_time(&target.read_snapshot(), &source, None, None)
            .unwrap(),
        524_288
    );
    let expected_digest = hex::encode(expected_digest.finalize());
    let mut actual_digest = Sha256::new();
    let snapshot = target.read_snapshot();
    let mut cursor = None;
    loop {
        let page = target
            .scan_by_source(&snapshot, &source, cursor.as_deref(), 4096)
            .unwrap();
        for record in page.records {
            actual_digest.update(serde_json::to_vec(&record).unwrap());
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(hex::encode(actual_digest.finalize()), expected_digest);
    for (kind, count) in &expected_types {
        assert_eq!(
            target
                .count_by_type(&snapshot, &source, kind, None, None)
                .unwrap(),
            *count
        );
    }
    let report = json!({"rows":rows,"elapsedMs":write_elapsed_ms,
        "segments":outcome.segments,"maxSegmentBytes":outcome.max_segment_bytes,
        "writeBufferBytes":metrics.write_buffer_bytes,"retainedVersions":retained_versions,
        "nativeWrites":outcome.native_writes,
        "recordDigest":expected_digest,"typeCounts":expected_types,"oraclesPassed":true});
    std::fs::write(
        output.join("memory.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
