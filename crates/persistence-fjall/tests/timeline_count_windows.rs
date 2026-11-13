use chrono::{DateTime, Duration, TimeZone, Utc};
use persistence_fjall::{
    encode_key, DatabaseConfig, FjallStorageError, KeyPart, KeyspaceName, PersistMode,
    TimelineRecord, TimelineRecordStore, WriteAction,
};
use std::collections::BTreeMap;

fn fixture() -> (tempfile::TempDir, TimelineRecordStore) {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/timeline");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let store = TimelineRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    (root, store)
}

fn record(id: usize, timestamp: DateTime<Utc>) -> TimelineRecord {
    TimelineRecord {
        id: format!("record-{id}"),
        data_source_id: "source".into(),
        source_object_id: "file".into(),
        event_type: if id.is_multiple_of(2) { "a" } else { "b" }.into(),
        timestamp,
        title: "event".into(),
        description: "event payload".into(),
        parser_id: None,
        parser_version: None,
        confidence: None,
        source_attribution: None,
        attrs: BTreeMap::new(),
    }
}

#[test]
fn batch_counts_preserve_nanosecond_edges_before_and_after_epoch() {
    let (_root, store) = fixture();
    let base = Utc.with_ymd_and_hms(1969, 12, 31, 23, 59, 59).unwrap();
    let records = [
        0,
        1,
        499,
        999,
        1000,
        1001,
        1999,
        999_999_999,
        1_000_000_000,
        1_000_000_001,
    ]
    .into_iter()
    .enumerate()
    .map(|(id, nanos)| record(id, base + Duration::nanoseconds(nanos)))
    .collect::<Vec<_>>();
    store.upsert_batch(&records, PersistMode::SyncAll).unwrap();
    let windows = [
        (0, 999),
        (1, 1000),
        (999, 1001),
        (1000, 1999),
        (999_999_999, 1_000_000_000),
        (1_000_000_001, 1_000_000_001),
    ]
    .map(|(a, b)| (base + Duration::nanoseconds(a))..=(base + Duration::nanoseconds(b)));
    let snapshot = store.read_snapshot();
    for kind in [None, Some("a"), Some("b"), Some("")] {
        let expected = windows
            .iter()
            .map(|window| {
                records
                    .iter()
                    .filter(|record| {
                        kind.is_none_or(|kind| record.event_type == kind)
                            && window.contains(&record.timestamp)
                    })
                    .count() as u64
            })
            .collect::<Vec<_>>();
        assert_eq!(
            store
                .count_windows(&snapshot, "source", kind, &windows)
                .unwrap(),
            expected
        );
        for (window, expected) in windows.iter().zip(expected) {
            let count = match kind {
                Some(kind) => store.count_by_type(
                    &snapshot,
                    "source",
                    kind,
                    Some(*window.start()),
                    Some(*window.end()),
                ),
                None => store.count_by_time(
                    &snapshot,
                    "source",
                    Some(*window.start()),
                    Some(*window.end()),
                ),
            }
            .unwrap();
            assert_eq!(count, expected);
        }
    }
}

#[test]
fn count_windows_reject_corrupt_time_index_identity_and_rollup_values() {
    let (_root, store) = fixture();
    let timestamp = Utc.with_ymd_and_hms(2024, 2, 29, 12, 0, 0).unwrap();
    store
        .upsert(&record(0, timestamp), PersistMode::SyncAll)
        .unwrap();
    let key = encode_key(
        "timeline.time",
        &[
            KeyPart::Text("source".into()),
            KeyPart::InverseU64((timestamp.timestamp_micros() as u64) ^ (1 << 63)),
            KeyPart::Text("record-0".into()),
        ],
    )
    .unwrap();
    let value = encode_key(
        "timeline.record",
        &[
            KeyPart::Text("source".into()),
            KeyPart::Text("other".into()),
        ],
    )
    .unwrap();
    store
        .source_store()
        .database()
        .write_batch(
            &[WriteAction::Put {
                keyspace: KeyspaceName::TimelineIndexes,
                key,
                value,
            }],
            PersistMode::SyncAll,
        )
        .unwrap();
    assert!(matches!(
        store.count_windows(
            &store.read_snapshot(),
            "source",
            None,
            &[timestamp..=(timestamp + Duration::seconds(1))]
        ),
        Err(FjallStorageError::TimelineRecordIdentityMismatch)
    ));

    let key = encode_key(
        "timeline.count",
        &[
            KeyPart::Text("source".into()),
            KeyPart::Text("all".into()),
            KeyPart::Text(String::new()),
            KeyPart::Text("day:2024-02-29".into()),
        ],
    )
    .unwrap();
    store
        .source_store()
        .database()
        .write_batch(
            &[WriteAction::Put {
                keyspace: KeyspaceName::TimelineIndexes,
                key,
                value: vec![1],
            }],
            PersistMode::SyncAll,
        )
        .unwrap();
    let start = Utc.with_ymd_and_hms(2024, 2, 29, 0, 0, 0).unwrap();
    assert!(matches!(
        store.count_windows(
            &store.read_snapshot(),
            "source",
            None,
            &[start..=(start + Duration::days(1) - Duration::nanoseconds(1))]
        ),
        Err(FjallStorageError::TimelineIndexCorrupt)
    ));
    assert!(store
        .count_windows(
            &store.read_snapshot(),
            "source",
            None,
            &vec![start..=start; 181]
        )
        .is_err());
}

#[test]
fn facet_edges_reject_cross_source_and_cross_type_record_pointers() {
    let (_root, store) = fixture();
    let timestamp = Utc.with_ymd_and_hms(2024, 2, 29, 12, 0, 0).unwrap();
    let first = record(0, timestamp);
    let second = record(1, timestamp + Duration::seconds(1));
    let mut foreign = first.clone();
    foreign.data_source_id = "other".into();
    store
        .upsert_batch(
            &[first.clone(), second.clone(), foreign.clone()],
            PersistMode::SyncAll,
        )
        .unwrap();
    for (family, kind, replacement) in [
        ("timeline.time", None, foreign),
        ("timeline.type", Some("a"), second),
    ] {
        let mut parts = vec![KeyPart::Text("source".into())];
        if let Some(kind) = kind {
            parts.push(KeyPart::Text(kind.into()));
        }
        parts.push(KeyPart::InverseU64(
            (timestamp.timestamp_micros() as u64) ^ (1 << 63),
        ));
        parts.push(KeyPart::Text(first.id.clone()));
        let key = encode_key(family, &parts).unwrap();
        let value = encode_key(
            "timeline.record",
            &[
                KeyPart::Text(replacement.data_source_id),
                KeyPart::Text(replacement.id),
            ],
        )
        .unwrap();
        assert!(store
            .read_snapshot()
            .get(KeyspaceName::TimelineRecords, &value)
            .unwrap()
            .is_some());
        store
            .source_store()
            .database()
            .write_batch(
                &[WriteAction::Put {
                    keyspace: KeyspaceName::TimelineIndexes,
                    key,
                    value,
                }],
                PersistMode::SyncAll,
            )
            .unwrap();
        assert!(matches!(
            store.facet_summary(&store.read_snapshot(), "source", kind, None, None),
            Err(FjallStorageError::TimelineIndexCorrupt)
        ));
    }
}
