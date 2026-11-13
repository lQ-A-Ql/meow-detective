use persistence_fjall::{
    encode_key, ArtifactRecord, ArtifactRecordStore, DatabaseConfig, KeyPart, KeyspaceName,
    PersistMode, ScanDirection, SourceStore, WriteAction,
};
use serde_json::json;
use std::collections::BTreeMap;

const SOURCE: &str = "source";

fn setup() -> (tempfile::TempDir, SourceStore, ArtifactRecordStore) {
    let root = tempfile::tempdir().unwrap();
    let source = SourceStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let store = ArtifactRecordStore::from_source_store(source.clone());
    (root, source, store)
}

fn record(source_id: &str, id: &str, artifact_type: &str) -> ArtifactRecord {
    ArtifactRecord {
        id: id.into(),
        data_source_id: source_id.into(),
        artifact_type: artifact_type.into(),
        source_object_id: Some(format!("object-{id}")),
        content_sha256: None,
        attrs: json!({ "title": id }),
    }
}

fn pairs(items: &[(&str, u64)]) -> Vec<(String, u64)> {
    items
        .iter()
        .map(|(name, count)| ((*name).to_string(), *count))
        .collect()
}

fn counts(store: &ArtifactRecordStore, source_id: &str) -> Vec<(String, u64)> {
    store
        .type_counts(&store.read_snapshot(), source_id)
        .unwrap()
}

/// Independent oracle: page every record of the source (small pages so paging
/// itself is exercised) and tally artifact types.
fn scan_oracle(store: &ArtifactRecordStore, source_id: &str) -> Vec<(String, u64)> {
    let snapshot = store.read_snapshot();
    let mut tally = BTreeMap::<String, u64>::new();
    let mut cursor = None;
    loop {
        let page = store
            .list_by_source(
                &snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                7,
            )
            .unwrap();
        for record in &page.records {
            *tally.entry(record.artifact_type.clone()).or_default() += 1;
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return tally.into_iter().collect();
        }
    }
}

fn assert_matches_oracle(store: &ArtifactRecordStore, source_id: &str, ready: bool) {
    let snapshot = store.read_snapshot();
    let observed = store.type_counts(&snapshot, source_id).unwrap();
    assert_eq!(observed, scan_oracle(store, source_id));
    let is_ready = store.type_counts_ready(&snapshot, source_id).unwrap();
    assert_eq!(is_ready, ready);
    let families = observed
        .iter()
        .map(|(family, _)| family.clone())
        .collect::<Vec<_>>();
    let mut sorted = families.clone();
    sorted.sort();
    assert_eq!(families, sorted, "type counts must be sorted by type name");
    for (family, count) in &observed {
        let read = store.count_by_type(&snapshot, source_id, family).unwrap();
        assert_eq!(read, *count);
    }
}

fn assert_counts_unavailable(store: &ArtifactRecordStore, source_id: &str) {
    let snapshot = store.read_snapshot();
    assert!(!store.type_counts_ready(&snapshot, source_id).unwrap());
    assert!(matches!(
        store.type_counts(&snapshot, source_id),
        Err(persistence_fjall::FjallStorageError::ArtifactIndexCorrupt)
    ));
    assert!(matches!(
        store.count_by_type(&snapshot, source_id, "evtx"),
        Err(persistence_fjall::FjallStorageError::ArtifactIndexCorrupt)
    ));
}

#[test]
fn upsert_batch_counts_records_and_writes_the_ready_marker() {
    let (_root, _source, store) = setup();
    store
        .upsert_batch(
            &[
                record(SOURCE, "a1", "evtx"),
                record(SOURCE, "a2", "evtx"),
                record(SOURCE, "b1", "prefetch"),
            ],
            PersistMode::Buffer,
        )
        .unwrap();
    let snapshot = store.read_snapshot();
    assert!(store.type_counts_ready(&snapshot, SOURCE).unwrap());
    assert_eq!(
        counts(&store, SOURCE),
        pairs(&[("evtx", 2), ("prefetch", 1)])
    );
    let evtx = store.count_by_type(&snapshot, SOURCE, "evtx").unwrap();
    assert_eq!(evtx, 2);
    let absent = store.count_by_type(&snapshot, SOURCE, "absent").unwrap();
    assert_eq!(absent, 0);
    assert_matches_oracle(&store, SOURCE, true);
}

#[test]
fn same_id_replacement_moves_the_count_between_types() {
    let (_root, _source, store) = setup();
    store
        .upsert_batch(
            &[
                record(SOURCE, "a1", "evtx"),
                record(SOURCE, "a2", "evtx"),
                record(SOURCE, "b1", "prefetch"),
            ],
            PersistMode::Buffer,
        )
        .unwrap();
    store
        .upsert_batch(&[record(SOURCE, "a1", "prefetch")], PersistMode::Buffer)
        .unwrap();
    assert_eq!(
        counts(&store, SOURCE),
        pairs(&[("evtx", 1), ("prefetch", 2)])
    );
    let mut replaced = record(SOURCE, "a2", "evtx");
    replaced.attrs = json!({ "title": "a2", "runCount": 2 });
    store
        .upsert_batch(&[replaced], PersistMode::Buffer)
        .unwrap();
    assert_matches_oracle(&store, SOURCE, true);
    assert_eq!(
        counts(&store, SOURCE),
        pairs(&[("evtx", 1), ("prefetch", 2)])
    );
}

#[test]
fn delete_batch_decrements_and_prunes_zero_types() {
    let (_root, _source, store) = setup();
    store
        .upsert_batch(
            &[
                record(SOURCE, "a1", "evtx"),
                record(SOURCE, "a2", "evtx"),
                record(SOURCE, "b1", "prefetch"),
            ],
            PersistMode::Buffer,
        )
        .unwrap();
    let ids = ["a1".to_string(), "a2".to_string()];
    let pruned = store
        .delete_batch(SOURCE, &ids, PersistMode::Buffer)
        .unwrap();
    assert_eq!(pruned, 2);
    let missing = store
        .delete_batch(SOURCE, &["missing".to_string()], PersistMode::Buffer)
        .unwrap();
    assert_eq!(missing, 0);
    let snapshot = store.read_snapshot();
    assert_eq!(counts(&store, SOURCE), pairs(&[("prefetch", 1)]));
    let evtx = store.count_by_type(&snapshot, SOURCE, "evtx").unwrap();
    assert_eq!(evtx, 0);
    assert_matches_oracle(&store, SOURCE, true);
}

#[test]
fn delete_by_source_object_where_decrements_matching_records_only() {
    let (_root, _source, store) = setup();
    let mut first = record(SOURCE, "a1", "evtx");
    first.source_object_id = Some("object-1".into());
    let mut second = record(SOURCE, "a2", "evtx");
    second.source_object_id = Some("object-1".into());
    let mut third = record(SOURCE, "b1", "prefetch");
    third.source_object_id = Some("object-1".into());
    let mut other = record(SOURCE, "c1", "evtx");
    other.source_object_id = Some("object-2".into());
    store
        .upsert_batch(&[first, second, third, other], PersistMode::Buffer)
        .unwrap();
    let deleted = store
        .delete_by_source_object_where(SOURCE, "object-1", PersistMode::Buffer, |record| {
            record.artifact_type == "evtx"
        })
        .unwrap();
    assert_eq!(deleted, 2);
    let skipped = store
        .delete_by_source_object_where(SOURCE, "object-1", PersistMode::Buffer, |_| false)
        .unwrap();
    assert_eq!(skipped, 0);
    assert_eq!(
        counts(&store, SOURCE),
        pairs(&[("evtx", 1), ("prefetch", 1)])
    );
    assert_matches_oracle(&store, SOURCE, true);
}

#[test]
fn independent_writes_need_a_rebuild_before_counts_are_authoritative() {
    let (_root, _source, store) = setup();
    store
        .upsert_batch_independent(
            &[
                record(SOURCE, "a1", "evtx"),
                record(SOURCE, "a2", "evtx"),
                record(SOURCE, "b1", "prefetch"),
            ],
            PersistMode::Buffer,
        )
        .unwrap();
    let snapshot = store.read_snapshot();
    assert!(!store.type_counts_ready(&snapshot, SOURCE).unwrap());
    assert_counts_unavailable(&store, SOURCE);

    store
        .rebuild_metrics_for_source_with_control(
            SOURCE,
            PersistMode::Buffer,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    let snapshot = store.read_snapshot();
    assert!(store.type_counts_ready(&snapshot, SOURCE).unwrap());
    let total = store.count(&snapshot, Some(SOURCE)).unwrap();
    assert_eq!(total, 3);
    assert_eq!(
        counts(&store, SOURCE),
        pairs(&[("evtx", 2), ("prefetch", 1)])
    );
    assert_matches_oracle(&store, SOURCE, true);

    store
        .upsert_batch_independent(&[record(SOURCE, "c1", "linux")], PersistMode::Buffer)
        .unwrap();
    assert!(!store
        .type_counts_ready(&store.read_snapshot(), SOURCE)
        .unwrap());
    assert_counts_unavailable(&store, SOURCE);
    assert_eq!(
        store.type_counts(&snapshot, SOURCE).unwrap(),
        pairs(&[("evtx", 2), ("prefetch", 1)])
    );
    store
        .rebuild_metrics_for_source_with_control(
            SOURCE,
            PersistMode::Buffer,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_matches_oracle(&store, SOURCE, true);
    assert_eq!(
        counts(&store, SOURCE),
        pairs(&[("evtx", 2), ("linux", 1), ("prefetch", 1)])
    );
}

#[test]
fn rebuild_prunes_type_keys_for_records_that_disappeared() {
    let (_root, source, store) = setup();
    store
        .upsert_batch(
            &[
                record(SOURCE, "a1", "evtx"),
                record(SOURCE, "b1", "prefetch"),
            ],
            PersistMode::Buffer,
        )
        .unwrap();
    // Delete the record envelope without touching the count family to simulate
    // drift; the rebuild must detect and prune the stale type key.
    let stale_key = encode_key(
        "artifact.record",
        &[KeyPart::Text(SOURCE.into()), KeyPart::Text("b1".into())],
    )
    .unwrap();
    let delete = WriteAction::Delete {
        keyspace: KeyspaceName::ArtifactRecords,
        key: stale_key,
    };
    source
        .database()
        .write_batch(&[delete], PersistMode::Buffer)
        .unwrap();
    assert_eq!(counts(&store, SOURCE).len(), 2);
    store
        .rebuild_metrics_for_source_with_control(
            SOURCE,
            PersistMode::Buffer,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_eq!(counts(&store, SOURCE), pairs(&[("evtx", 1)]));
    assert_matches_oracle(&store, SOURCE, true);
}

#[test]
fn upsert_does_not_publish_counts_for_an_unprepared_source() {
    let (_root, _source, store) = setup();
    store
        .upsert_batch_independent(&[record(SOURCE, "a1", "evtx")], PersistMode::Buffer)
        .unwrap();
    store
        .upsert_batch(&[record(SOURCE, "b1", "prefetch")], PersistMode::Buffer)
        .unwrap();
    let snapshot = store.read_snapshot();
    assert!(!store.type_counts_ready(&snapshot, SOURCE).unwrap());
    assert_counts_unavailable(&store, SOURCE);

    store
        .rebuild_metrics_for_source_with_control(
            SOURCE,
            PersistMode::Buffer,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_matches_oracle(&store, SOURCE, true);
}

#[test]
fn type_counts_are_isolated_per_source() {
    let (_root, _source, store) = setup();
    store
        .upsert_batch(
            &[
                record("source-a", "a1", "evtx"),
                record("source-b", "b1", "prefetch"),
                record("source-b", "b2", "prefetch"),
            ],
            PersistMode::Buffer,
        )
        .unwrap();
    assert_eq!(counts(&store, "source-a"), pairs(&[("evtx", 1)]));
    assert_eq!(counts(&store, "source-b"), pairs(&[("prefetch", 2)]));
    let ids = ["b1".to_string(), "b2".to_string()];
    let deleted = store
        .delete_batch("source-b", &ids, PersistMode::Buffer)
        .unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(counts(&store, "source-a"), pairs(&[("evtx", 1)]));
    assert!(counts(&store, "source-b").is_empty());
    assert_matches_oracle(&store, "source-a", true);
    assert_matches_oracle(&store, "source-b", true);
}

#[test]
fn count_underflow_is_reported_instead_of_saturating() {
    let (_root, source, store) = setup();
    store
        .upsert_batch(&[record(SOURCE, "a1", "evtx")], PersistMode::Buffer)
        .unwrap();
    let count_key = encode_key(
        "artifact.type-count",
        &[KeyPart::Text(SOURCE.into()), KeyPart::Text("evtx".into())],
    )
    .unwrap();
    let remove = WriteAction::Delete {
        keyspace: KeyspaceName::ArtifactRecords,
        key: count_key,
    };
    source
        .database()
        .write_batch(&[remove], PersistMode::Buffer)
        .unwrap();
    let error = store
        .delete_batch(SOURCE, &["a1".to_string()], PersistMode::Buffer)
        .unwrap_err();
    assert!(
        error.to_string().contains("underflowed"),
        "unexpected error: {error}"
    );
    let snapshot = store.read_snapshot();
    let record = store.get(&snapshot, SOURCE, "a1").unwrap();
    assert!(
        record.is_some(),
        "failed delete must not commit record changes"
    );
}

#[test]
fn type_count_queries_read_only_counter_entries() {
    let (_root, _source, store) = setup();
    let records = (0..1024)
        .map(|id| {
            record(
                SOURCE,
                &format!("row-{id}"),
                if id % 2 == 0 { "evtx" } else { "prefetch" },
            )
        })
        .collect::<Vec<_>>();
    store.upsert_batch(&records, PersistMode::Buffer).unwrap();
    let snapshot = store.read_snapshot();
    persistence_fjall::instrumentation::enable();
    persistence_fjall::instrumentation::reset();
    let all = store.type_counts(&snapshot, SOURCE);
    let one = store.count_by_type(&snapshot, SOURCE, "evtx");
    let work = persistence_fjall::instrumentation::snapshot();
    persistence_fjall::instrumentation::disable();
    assert_eq!(all.unwrap(), pairs(&[("evtx", 512), ("prefetch", 512)]));
    assert_eq!(one.unwrap(), 512);
    assert_eq!(work.scan_entries, 2, "{work:?}");
    assert_eq!(work.point_reads, 3, "{work:?}");
}

#[test]
fn refilling_an_uncounted_empty_source_does_not_reuse_stale_counts() {
    let (_root, _source, store) = setup();
    let artifact = record(SOURCE, "a1", "evtx");
    store
        .upsert_batch(std::slice::from_ref(&artifact), PersistMode::Buffer)
        .unwrap();
    store
        .upsert_batch_independent(&[artifact], PersistMode::Buffer)
        .unwrap();
    store
        .delete_batch(SOURCE, &["a1".into()], PersistMode::Buffer)
        .unwrap();
    assert!(counts(&store, SOURCE).is_empty());
    store
        .upsert_batch(&[record(SOURCE, "b1", "prefetch")], PersistMode::Buffer)
        .unwrap();
    assert_counts_unavailable(&store, SOURCE);
    store
        .rebuild_metrics_for_source_with_control(
            SOURCE,
            PersistMode::Buffer,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_matches_oracle(&store, SOURCE, true);
}
