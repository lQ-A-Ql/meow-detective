use chrono::{TimeZone, Utc};
use persistence_fjall::{
    encode_key, DatabaseConfig, FjallStorageError, KeyPart, KeyspaceName, PersistMode,
    ScanDirection, TimelineRecord, TimelineRecordStore,
};
use std::sync::atomic::{AtomicBool, Ordering};

fn record(index: usize) -> TimelineRecord {
    TimelineRecord {
        id: format!("event-{index:04}"),
        data_source_id: "source".to_owned(),
        source_object_id: format!("file-{index}"),
        event_type: if index.is_multiple_of(2) {
            "CREATE"
        } else {
            "LOGON"
        }
        .to_owned(),
        timestamp: Utc.timestamp_opt(1_700_000_000 + index as i64, 0).unwrap(),
        title: format!("event {index}"),
        description: String::new(),
        parser_id: Some("test.timeline".to_owned()),
        parser_version: None,
        confidence: None,
        source_attribution: None,
        attrs: Default::default(),
    }
}

#[test]
fn cancelled_rebuild_leaves_previous_counts_and_revision_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let records = (0..1_100).map(record).collect::<Vec<_>>();
    store.upsert_batch(&records, PersistMode::SyncAll).unwrap();
    let before = store.read_snapshot();
    let prefix = encode_key("timeline.count", &[KeyPart::Text("source".to_owned())]).unwrap();
    let original = before
        .prefix(
            KeyspaceName::TimelineIndexes,
            &prefix,
            ScanDirection::Forward,
        )
        .unwrap();
    let revision = store.cursor_revision(&before).unwrap();
    let cancel = AtomicBool::new(false);
    let mut visited = 0;
    let result = store.rebuild_count_indexes_with_control(
        "source",
        PersistMode::SyncAll,
        &cancel,
        |count| {
            visited = count;
            if count >= 512 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    assert!(matches!(result, Err(FjallStorageError::Cancelled)));
    assert_eq!(visited, 512);
    let after = store.read_snapshot();
    assert_eq!(store.cursor_revision(&after).unwrap(), revision);
    assert_eq!(
        after
            .prefix(
                KeyspaceName::TimelineIndexes,
                &prefix,
                ScanDirection::Forward
            )
            .unwrap(),
        original
    );
    assert_eq!(
        store.count_by_time(&after, "source", None, None).unwrap(),
        1_100
    );
}

#[test]
fn controlled_rebuild_reports_pages_and_publishes_counts_together() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let records = (0..1_101).map(record).collect::<Vec<_>>();
    store
        .upsert_batch_independent(&records, PersistMode::SyncAll)
        .unwrap();
    let before = store.read_snapshot();
    assert!(!store.counts_ready(&before, "source").unwrap());
    let mut progress = Vec::new();
    store
        .rebuild_count_indexes_with_control(
            "source",
            PersistMode::SyncAll,
            &AtomicBool::new(false),
            |count| progress.push(count),
        )
        .unwrap();
    assert_eq!(progress, [0, 512, 1_024, 1_101]);
    let after = store.read_snapshot();
    assert!(store.counts_ready(&after, "source").unwrap());
    assert!(!store.counts_ready(&before, "source").unwrap());
    assert_eq!(
        store.count_by_time(&after, "source", None, None).unwrap(),
        1_101
    );
    assert_eq!(
        store
            .count_by_type(&after, "source", "CREATE", None, None)
            .unwrap(),
        551
    );
    assert_eq!(
        store
            .count_by_type(&after, "source", "LOGON", None, None)
            .unwrap(),
        550
    );
}

#[test]
fn concurrent_timeline_changes_prevent_publishing_stale_rebuilt_counts() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    store
        .upsert_batch(&[record(0)], PersistMode::SyncAll)
        .unwrap();
    let result = store.rebuild_count_indexes_with_control(
        "source",
        PersistMode::SyncAll,
        &AtomicBool::new(false),
        |count| {
            if count == 1 {
                store.upsert(&record(1), PersistMode::SyncAll).unwrap();
            }
        },
    );
    assert!(matches!(result, Err(FjallStorageError::InvalidEnvelope(_))));
    let after = store.read_snapshot();
    assert_eq!(
        store.count_by_time(&after, "source", None, None).unwrap(),
        2
    );
}
