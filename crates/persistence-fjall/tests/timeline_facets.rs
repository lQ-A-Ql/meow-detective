use chrono::{DateTime, Duration, TimeZone, Utc};
use persistence_fjall::{
    DatabaseConfig, FacetSummary, PersistMode, TimelineRecord, TimelineRecordStore,
};
use std::collections::BTreeMap;

const SOURCE: &str = "source";

fn timestamp(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn record(source: &str, id: &str, event_type: &str, when: DateTime<Utc>) -> TimelineRecord {
    TimelineRecord {
        id: id.to_owned(),
        data_source_id: source.to_owned(),
        source_object_id: format!("file-{id}"),
        event_type: event_type.to_owned(),
        timestamp: when,
        title: format!("title-{id}"),
        description: format!("description-{id}"),
        parser_id: Some("parser".to_owned()),
        parser_version: Some("1".to_owned()),
        confidence: Some(0.9),
        source_attribution: Some("fixture".to_owned()),
        attrs: BTreeMap::new(),
    }
}

fn base_seconds() -> i64 {
    timestamp_base().timestamp()
}

fn timestamp_base() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
}

/// 50 records over five days and three event types, one per hourly slot.
fn dataset() -> Vec<TimelineRecord> {
    let types = ["FILE_CREATED", "FILE_MODIFIED", "LOGON"];
    let mut records = Vec::new();
    for day in 0..5 {
        for hour in 0..10 {
            let when = timestamp_base() + Duration::hours(day * 24 + hour);
            let event_type = types[(day * 10 + hour) as usize % types.len()];
            records.push(record(
                SOURCE,
                &format!("r{day:02}{hour:02}"),
                event_type,
                when,
            ));
        }
    }
    records
}

fn open_store() -> (tempfile::TempDir, TimelineRecordStore) {
    let root = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    (root, store)
}

#[derive(Clone, Copy)]
enum WriteMode {
    Counted,
    Rebuilt,
}

fn publish(store: &TimelineRecordStore, records: &[TimelineRecord], mode: WriteMode) {
    match mode {
        WriteMode::Counted => store.upsert_batch(records, PersistMode::SyncAll).unwrap(),
        WriteMode::Rebuilt => {
            store
                .upsert_batch_independent(records, PersistMode::SyncAll)
                .unwrap();
            store
                .rebuild_count_indexes_with_control(
                    SOURCE,
                    PersistMode::SyncAll,
                    &std::sync::atomic::AtomicBool::new(false),
                    |_| {},
                )
                .unwrap();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Oracle {
    total: u64,
    min_epoch: Option<i64>,
    max_epoch: Option<i64>,
    type_totals: BTreeMap<String, u64>,
}

fn brute_force(
    records: &[TimelineRecord],
    event_type: Option<&str>,
    start: Option<i64>,
    end: Option<i64>,
) -> Oracle {
    let mut oracle = Oracle {
        total: 0,
        min_epoch: None,
        max_epoch: None,
        type_totals: BTreeMap::new(),
    };
    for record in records {
        *oracle
            .type_totals
            .entry(record.event_type.clone())
            .or_default() += 1;
        if event_type.is_some_and(|kind| kind != record.event_type) {
            continue;
        }
        let epoch = record.timestamp.timestamp();
        if start.is_some_and(|bound| epoch < bound) || end.is_some_and(|bound| epoch > bound) {
            continue;
        }
        oracle.total += 1;
        oracle.min_epoch = Some(oracle.min_epoch.map_or(epoch, |value| value.min(epoch)));
        oracle.max_epoch = Some(oracle.max_epoch.map_or(epoch, |value| value.max(epoch)));
    }
    oracle
}

fn summarize(
    store: &TimelineRecordStore,
    source_id: &str,
    event_type: Option<&str>,
    start: Option<i64>,
    end: Option<i64>,
) -> FacetSummary {
    let snapshot = store.read_snapshot();
    store
        .facet_summary(
            &snapshot,
            source_id,
            event_type,
            start.map(timestamp),
            end.map(timestamp),
        )
        .unwrap()
}

fn assert_filter_matrix(store: &TimelineRecordStore, records: &[TimelineRecord]) {
    let base = base_seconds();
    let day = 86_400;
    let filters: Vec<(Option<&str>, Option<i64>, Option<i64>)> = vec![
        (None, None, None),
        (Some("FILE_CREATED"), None, None),
        (Some("MISSING_TYPE"), None, None),
        (Some("MISSING_TYPE"), Some(base), None),
        (Some("MISSING_TYPE"), None, Some(base + day)),
        (None, Some(base + 2 * day - 1), None),
        (None, None, Some(base + 2 * day + 5)),
        (None, Some(base + day), Some(base + 2 * day)),
        (
            Some("FILE_MODIFIED"),
            Some(base + day),
            Some(base + 3 * day),
        ),
        (None, Some(base + day + 3_600), Some(base + day + 3_600)),
        (Some("FILE_CREATED"), Some(base), Some(base)),
        (None, Some(base + 30 * day), Some(base + 31 * day)),
        (None, Some(base + 3 * day), Some(base + day)),
    ];
    for (event_type, start, end) in filters {
        let expected = brute_force(records, event_type, start, end);
        let summary = summarize(store, SOURCE, event_type, start, end);
        let label = format!("type={event_type:?} start={start:?} end={end:?}");
        assert_eq!(summary.total, expected.total, "{label}");
        assert_eq!(
            summary.min_timestamp.map(|value| value.timestamp()),
            expected.min_epoch,
            "{label}"
        );
        assert_eq!(
            summary.max_timestamp.map(|value| value.timestamp()),
            expected.max_epoch,
            "{label}"
        );
        assert_eq!(summary.type_totals, expected.type_totals, "{label}");
        if summary.total == 0 {
            assert!(summary.min_timestamp.is_none(), "{label}");
            assert!(summary.max_timestamp.is_none(), "{label}");
        }
    }
}

#[test]
fn facet_summary_matches_the_oracle_when_counts_are_ready() {
    let (_root, store) = open_store();
    let records = dataset();
    publish(&store, &records, WriteMode::Counted);
    assert_filter_matrix(&store, &records);
}

#[test]
fn facet_summary_matches_the_oracle_after_count_rebuild() {
    let (_root, store) = open_store();
    let records = dataset();
    publish(&store, &records, WriteMode::Rebuilt);
    assert_filter_matrix(&store, &records);
}

#[test]
fn facet_summary_keeps_equal_and_subsecond_timestamps_inside_second_bounds() {
    let base = timestamp_base();
    let records = vec![
        record(SOURCE, "a", "FILE_CREATED", base),
        record(SOURCE, "b", "FILE_CREATED", base),
        record(
            SOURCE,
            "c",
            "FILE_MODIFIED",
            base + Duration::microseconds(1),
        ),
        record(
            SOURCE,
            "d",
            "FILE_CREATED",
            base + Duration::microseconds(999_999),
        ),
        record(SOURCE, "e", "LOGON", base + Duration::seconds(1)),
        record(
            SOURCE,
            "f",
            "FILE_CREATED",
            base + Duration::seconds(1) + Duration::microseconds(500_000),
        ),
        record(SOURCE, "g", "FILE_MODIFIED", base + Duration::seconds(2)),
    ];
    let filters: Vec<(Option<&str>, Option<i64>, Option<i64>)> = vec![
        (None, None, None),
        (None, Some(base.timestamp()), Some(base.timestamp())),
        (None, Some(base.timestamp() + 1), Some(base.timestamp() + 1)),
        (
            Some("FILE_CREATED"),
            Some(base.timestamp()),
            Some(base.timestamp()),
        ),
        (None, Some(base.timestamp()), Some(base.timestamp() + 1)),
        (Some("LOGON"), None, Some(base.timestamp())),
    ];
    for mode in [WriteMode::Counted, WriteMode::Rebuilt] {
        let (_root, store) = open_store();
        publish(&store, &records, mode);
        for (event_type, start, end) in &filters {
            let expected = brute_force(&records, *event_type, *start, *end);
            let summary = summarize(&store, SOURCE, *event_type, *start, *end);
            let label = format!("type={event_type:?} start={start:?} end={end:?}");
            assert_eq!(summary.total, expected.total, "{label}");
            assert_eq!(
                summary.min_timestamp.map(|value| value.timestamp()),
                expected.min_epoch,
                "{label}"
            );
            assert_eq!(
                summary.max_timestamp.map(|value| value.timestamp()),
                expected.max_epoch,
                "{label}"
            );
            assert_eq!(summary.type_totals, expected.type_totals, "{label}");
        }
        let bounded = summarize(
            &store,
            SOURCE,
            None,
            Some(base.timestamp()),
            Some(base.timestamp()),
        );
        assert_eq!(bounded.total, 4);
        assert_eq!(bounded.min_timestamp, Some(base));
        assert_eq!(
            bounded.max_timestamp,
            Some(base + Duration::microseconds(999_999))
        );
    }
}

#[test]
fn facet_summary_of_an_empty_source_is_zero() {
    let (_root, store) = open_store();
    let summary = summarize(&store, SOURCE, None, None, None);
    assert_eq!(summary.total, 0);
    assert_eq!(summary.min_timestamp, None);
    assert_eq!(summary.max_timestamp, None);
    assert!(summary.type_totals.is_empty());
    let filtered = summarize(&store, SOURCE, Some("FILE_CREATED"), Some(1), Some(2));
    assert_eq!(filtered.total, 0);
    assert_eq!(filtered.min_timestamp, None);
}

#[test]
fn facet_summary_rejects_an_unprepared_nonempty_source() {
    let (_root, store) = open_store();
    store
        .upsert_batch_independent(&dataset(), PersistMode::SyncAll)
        .unwrap();
    let result = store.facet_summary(&store.read_snapshot(), SOURCE, None, None, None);
    assert!(matches!(
        result,
        Err(persistence_fjall::FjallStorageError::TimelineIndexCorrupt)
    ));
}

#[test]
fn bounded_index_pages_stay_inside_both_window_edges() {
    let (_root, store) = open_store();
    let records = dataset();
    publish(&store, &records, WriteMode::Counted);
    let snapshot = store.read_snapshot();
    let from = timestamp_base() + Duration::days(1);
    let until = timestamp_base() + Duration::hours(24 * 2 + 3) + Duration::microseconds(999_999);

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = store
            .list_in_time_window(&snapshot, SOURCE, None, from..=until, cursor.as_deref(), 7)
            .unwrap();
        seen.extend(
            page.records
                .iter()
                .map(|record| (record.timestamp, record.id.clone())),
        );
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut expected = records
        .iter()
        .filter(|record| (from..=until).contains(&record.timestamp))
        .map(|record| (record.timestamp, record.id.clone()))
        .collect::<Vec<_>>();
    expected.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    assert_eq!(seen, expected);
    assert!(seen.iter().all(|(when, _)| (from..=until).contains(when)));

    let mut typed = Vec::new();
    let mut cursor = None;
    loop {
        let page = store
            .list_in_time_window(
                &snapshot,
                SOURCE,
                Some("FILE_CREATED"),
                from..=until,
                cursor.as_deref(),
                5,
            )
            .unwrap();
        typed.extend(page.records.iter().map(|record| record.timestamp));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut expected = records
        .iter()
        .filter(|record| {
            record.event_type == "FILE_CREATED" && (from..=until).contains(&record.timestamp)
        })
        .map(|record| record.timestamp)
        .collect::<Vec<_>>();
    expected.sort_by(|left, right| right.cmp(left));
    assert_eq!(typed, expected);
}

#[test]
fn facet_summary_rejects_an_index_pointing_to_a_missing_record() {
    let (_root, store) = open_store();
    let records = dataset();
    publish(&store, &records, WriteMode::Counted);
    let key = persistence_fjall::encode_key(
        "timeline.record",
        &[
            persistence_fjall::KeyPart::Text(SOURCE.to_owned()),
            persistence_fjall::KeyPart::Text(records[0].id.clone()),
        ],
    )
    .unwrap();
    store
        .source_store()
        .database()
        .write_batch(
            &[persistence_fjall::WriteAction::Delete {
                keyspace: persistence_fjall::KeyspaceName::TimelineRecords,
                key,
            }],
            PersistMode::Buffer,
        )
        .unwrap();
    let error = store
        .facet_summary(&store.read_snapshot(), SOURCE, None, None, None)
        .unwrap_err();
    assert!(matches!(
        error,
        persistence_fjall::FjallStorageError::TimelineIndexCorrupt
    ));
}
