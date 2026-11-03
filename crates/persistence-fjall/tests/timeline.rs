use chrono::{DateTime, TimeZone, Utc};
use persistence_fjall::{
    decode_timestamp_desc, encode_timestamp_desc, DatabaseConfig, PersistMode, ScanDirection,
    TimelineRecord, TimelineRecordStore,
};
use serde_json::json;
use std::collections::BTreeMap;

fn timestamp(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn record(
    source: &str,
    id: &str,
    event_type: &str,
    object_id: &str,
    seconds: i64,
) -> TimelineRecord {
    TimelineRecord {
        id: id.to_owned(),
        data_source_id: source.to_owned(),
        source_object_id: object_id.to_owned(),
        event_type: event_type.to_owned(),
        timestamp: timestamp(seconds),
        title: format!("title-{id}"),
        description: format!("description-{id}"),
        parser_id: Some("parser".to_owned()),
        parser_version: Some("1".to_owned()),
        confidence: Some(0.9),
        source_attribution: Some("fixture".to_owned()),
        attrs: BTreeMap::from([(String::from("key"), json!(id))]),
    }
}

#[test]
fn upsert_and_delete_clean_all_timeline_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let mut event = record("source-a", "event-1", "FILE_MODIFIED", "file-1", 10);
    store.upsert(&event, PersistMode::SyncData).unwrap();
    event.title = "updated".to_owned();
    event.event_type = "FILE_CREATED".to_owned();
    event.source_object_id = "file-2".to_owned();
    event.timestamp = timestamp(20);
    store.upsert(&event, PersistMode::SyncData).unwrap();

    let snapshot = store.read_snapshot();
    assert_eq!(
        store
            .list_by_type(
                &snapshot,
                "source-a",
                "FILE_CREATED",
                None,
                ScanDirection::Forward,
                10,
            )
            .unwrap()
            .records,
        vec![event.clone()]
    );
    assert!(store
        .list_by_type(
            &snapshot,
            "source-a",
            "FILE_MODIFIED",
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap()
        .records
        .is_empty());
    assert!(store
        .list_by_source_object(
            &snapshot,
            "source-a",
            "file-1",
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap()
        .records
        .is_empty());

    assert!(store
        .delete("source-a", "event-1", PersistMode::SyncData)
        .unwrap());
    let snapshot = store.read_snapshot();
    assert!(store
        .get(&snapshot, "source-a", "event-1")
        .unwrap()
        .is_none());
    assert!(store
        .list_by_source(&snapshot, "source-a", None, ScanDirection::Forward, 10)
        .unwrap()
        .records
        .is_empty());
    assert!(!store
        .delete("source-a", "event-1", PersistMode::SyncData)
        .unwrap());
}

#[test]
fn timeline_batch_upsert_writes_all_records_and_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let records = vec![
        record("source-a", "batch-a", "FILE_MODIFIED", "file-a", 30),
        record("source-a", "batch-b", "FILE_MODIFIED", "file-b", 20),
    ];
    store.upsert_batch(&records, PersistMode::SyncData).unwrap();
    let snapshot = store.read_snapshot();
    assert_eq!(
        store
            .list_by_source(&snapshot, "source-a", None, ScanDirection::Forward, 10)
            .unwrap()
            .records,
        records
    );
}

#[test]
fn timeline_index_counts_match_type_and_time_filters() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let records = [
        record("source-a", "a", "CREATE", "file-a", 30),
        record("source-a", "b", "MODIFY", "file-b", 20),
        record("source-a", "c", "CREATE", "file-c", 10),
    ];
    store.upsert_batch(&records, PersistMode::SyncData).unwrap();
    let snapshot = store.read_snapshot();
    assert_eq!(
        store
            .count_by_time(
                &snapshot,
                "source-a",
                Some(timestamp(15)),
                Some(timestamp(30))
            )
            .unwrap(),
        2
    );
    assert_eq!(
        store
            .count_by_type(&snapshot, "source-a", "CREATE", Some(timestamp(15)), None,)
            .unwrap(),
        1
    );
}

#[test]
fn timeline_aggregate_counts_cover_cross_day_ranges_and_deletes() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let first = record("source-a", "first", "CREATE", "file-a", 86_400 + 10);
    let second = record("source-a", "second", "CREATE", "file-b", 2 * 86_400 + 10);
    let third = record("source-a", "third", "MODIFY", "file-c", 3 * 86_400 + 10);
    store
        .upsert_batch(
            &[first.clone(), second.clone(), third.clone()],
            PersistMode::SyncAll,
        )
        .unwrap();
    let snapshot = store.read_snapshot();
    assert_eq!(
        store
            .count_by_time(
                &snapshot,
                "source-a",
                Some(timestamp(86_400)),
                Some(timestamp(3 * 86_400 + 10)),
            )
            .unwrap(),
        3
    );
    assert_eq!(
        store
            .count_by_type(
                &snapshot,
                "source-a",
                "CREATE",
                Some(timestamp(86_400)),
                Some(timestamp(2 * 86_400 + 10)),
            )
            .unwrap(),
        2
    );
    store
        .delete("source-a", "second", PersistMode::SyncAll)
        .unwrap();
    let snapshot = store.read_snapshot();
    assert_eq!(
        store
            .count_by_time(&snapshot, "source-a", None, None)
            .unwrap(),
        2
    );
    assert_eq!(
        store
            .count_by_type(&snapshot, "source-a", "CREATE", None, None)
            .unwrap(),
        1
    );
}

#[test]
fn repeated_identical_upsert_keeps_timeline_revision_stable() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let event = record("source-a", "same", "EVENT", "file", 10);
    store.upsert(&event, PersistMode::SyncData).unwrap();
    let first_revision = store.cursor_revision(&store.read_snapshot()).unwrap();
    store.upsert(&event, PersistMode::SyncData).unwrap();
    let second_revision = store.cursor_revision(&store.read_snapshot()).unwrap();
    assert_eq!(first_revision, second_revision);
}

#[test]
fn type_and_source_queries_are_keyset_paginated() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    for (id, seconds) in [("a", 30), ("b", 20), ("c", 10)] {
        store
            .upsert(
                &record("source-a", id, "FILE_MODIFIED", id, seconds),
                PersistMode::SyncData,
            )
            .unwrap();
    }
    let snapshot = store.read_snapshot();
    let first = store
        .list_by_type(
            &snapshot,
            "source-a",
            "FILE_MODIFIED",
            None,
            ScanDirection::Forward,
            2,
        )
        .unwrap();
    assert_eq!(
        first
            .records
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    let second = store
        .list_by_type(
            &snapshot,
            "source-a",
            "FILE_MODIFIED",
            first.next_cursor.as_deref(),
            ScanDirection::Forward,
            2,
        )
        .unwrap();
    assert_eq!(
        second
            .records
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["c"]
    );
    assert!(second.next_cursor.is_none());
}

#[test]
fn time_indexes_return_newest_first_and_reverse_order() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    for (id, seconds) in [("old", 1), ("new", 3), ("middle", 2)] {
        store
            .upsert(
                &record("source-a", id, "EVENT", id, seconds),
                PersistMode::SyncData,
            )
            .unwrap();
    }
    let snapshot = store.read_snapshot();
    let newest = store
        .list_by_time(&snapshot, "source-a", None, ScanDirection::Forward, 10)
        .unwrap();
    assert_eq!(
        newest
            .records
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["new", "middle", "old"]
    );
    let oldest = store
        .list_by_time(&snapshot, "source-a", None, ScanDirection::Reverse, 10)
        .unwrap();
    assert_eq!(
        oldest
            .records
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["old", "middle", "new"]
    );
}

#[test]
fn indexes_are_isolated_by_source() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    store
        .upsert(
            &record("source-a", "same-id", "EVENT", "same-object", 2),
            PersistMode::SyncData,
        )
        .unwrap();
    store
        .upsert(
            &record("source-b", "same-id", "EVENT", "same-object", 1),
            PersistMode::SyncData,
        )
        .unwrap();
    let snapshot = store.read_snapshot();
    let source_a = store
        .list_by_source_object(
            &snapshot,
            "source-a",
            "same-object",
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap();
    assert_eq!(source_a.records.len(), 1);
    assert_eq!(source_a.records[0].data_source_id, "source-a");
    let source_b = store
        .list_by_source(&snapshot, "source-b", None, ScanDirection::Forward, 10)
        .unwrap();
    assert_eq!(source_b.records.len(), 1);
    assert_eq!(source_b.records[0].data_source_id, "source-b");
}

#[test]
fn timestamp_encoding_round_trips_and_orders_descending() {
    let earlier = encode_timestamp_desc(timestamp(10));
    let later = encode_timestamp_desc(timestamp(20));
    assert!(later < earlier);
    assert_eq!(decode_timestamp_desc(&earlier).unwrap(), timestamp(10));
    assert_eq!(decode_timestamp_desc(&later).unwrap(), timestamp(20));
}
