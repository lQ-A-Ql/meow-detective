use chrono::{DateTime, TimeZone, Utc};
use persistence_fjall::{
    decode_timeline_projection_state, timeline_projection_state_key, DatabaseConfig, KeyspaceName,
    PersistMode, SegmentLimits, TimelineProjectionState, TimelineProjectionStatePayload,
    TimelineRecord, TimelineRecordStore, VersionedEnvelope,
};
use serde_json::json;
use std::collections::BTreeMap;

type ProducerResult = Result<Option<Vec<TimelineRecord>>, std::io::Error>;

fn timestamp(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0).single().unwrap()
}

fn record(
    source: &str,
    id: &str,
    event_type: &str,
    object_id: &str,
    seconds: i64,
    parser_id: &str,
) -> TimelineRecord {
    TimelineRecord {
        id: id.to_owned(),
        data_source_id: source.to_owned(),
        source_object_id: object_id.to_owned(),
        event_type: event_type.to_owned(),
        timestamp: timestamp(seconds),
        title: format!("title-{id}"),
        description: format!("description-{id}"),
        parser_id: Some(parser_id.to_owned()),
        parser_version: Some("1".to_owned()),
        confidence: Some(0.9),
        source_attribution: Some("fixture".to_owned()),
        attrs: BTreeMap::from([(String::from("key"), json!(id))]),
    }
}

fn new_record(index: usize) -> TimelineRecord {
    record(
        "source",
        &format!("new-{index:04}"),
        "FILE_MODIFIED",
        &format!("file-{index}"),
        100 + index as i64,
        "timeline.new",
    )
}

fn old_record(index: usize) -> TimelineRecord {
    record(
        "source",
        &format!("old-{index:04}"),
        "FILE_CREATED",
        &format!("old-file-{index}"),
        index as i64,
        "timeline.old",
    )
}

fn keep_record(index: usize) -> TimelineRecord {
    record(
        "source",
        &format!("keep-{index}"),
        "LOGON",
        "keep-file",
        50_000 + index as i64,
        "analysis.keep",
    )
}

fn read_marker(
    snapshot: &persistence_fjall::SourceReadSnapshot,
    source_id: &str,
) -> Option<TimelineProjectionStatePayload> {
    let key = timeline_projection_state_key(source_id).unwrap();
    let bytes = snapshot.get(KeyspaceName::Meta, &key).unwrap()?;
    let envelope = VersionedEnvelope::decode(&bytes).unwrap();
    decode_timeline_projection_state(&envelope).unwrap()
}

#[test]
fn parallel_replace_keeps_records_counts_indexes_and_publishes_one_revision() {
    let root = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let old: Vec<TimelineRecord> = (0..400).map(old_record).collect();
    let keep = vec![keep_record(0), keep_record(1)];
    store
        .upsert_batch(&[old, keep].concat(), PersistMode::SyncAll)
        .unwrap();
    let before = store.read_snapshot();
    let revision = store.cursor_revision(&before).unwrap();

    let limits = SegmentLimits {
        max_bytes: u64::MAX,
        max_records: 300,
    };
    let mut batches: Vec<Vec<TimelineRecord>> = Vec::new();
    let all: Vec<TimelineRecord> = (0..1205).map(new_record).collect();
    for chunk in all.chunks(250) {
        batches.push(chunk.to_vec());
    }
    let mut calls = 0usize;
    let outcome = store
        .replace_by_parser_prefix_parallel(
            "source",
            "timeline.",
            "identity-segmented",
            PersistMode::SyncAll,
            limits,
            Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            move || -> ProducerResult {
                calls += 1;
                if calls <= batches.len() {
                    Ok(Some(std::mem::take(&mut batches[calls - 1])))
                } else {
                    Ok(None)
                }
            },
        )
        .unwrap();

    assert_eq!(outcome.inserted_records, 1205);
    assert_eq!(outcome.deleted_records, 400);
    assert_eq!(outcome.segments, outcome.native_writes.batches);
    assert_eq!(outcome.native_writes.workers_used, 8);
    assert!(outcome.max_segment_bytes > 0);

    let after = store.read_snapshot();
    assert_eq!(
        store.count_by_time(&after, "source", None, None).unwrap(),
        1207
    );
    assert_eq!(
        store
            .count_by_type(&after, "source", "FILE_CREATED", None, None)
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .count_by_type(&after, "source", "FILE_MODIFIED", None, None)
            .unwrap(),
        1205
    );
    assert!(store.get(&after, "source", "old-0000").unwrap().is_none());
    assert!(store.get(&after, "source", "new-0000").unwrap().is_some());
    assert!(store.get(&after, "source", "keep-0").unwrap().is_some());
    assert_eq!(store.cursor_revision(&after).unwrap(), revision + 1);

    let marker = read_marker(&after, "source").unwrap();
    assert_eq!(marker.status, TimelineProjectionState::Complete);
    assert_eq!(marker.inserted_events, 1205);
    assert_eq!(marker.identity, "identity-segmented");
}

#[test]
fn small_record_limit_bounds_parallel_batches_and_stays_consistent() {
    let root = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let before = store.read_snapshot();
    let revision = store.cursor_revision(&before).unwrap();

    let limits = SegmentLimits {
        max_bytes: u64::MAX,
        max_records: 3,
    };
    let records: Vec<TimelineRecord> = (0..10).map(new_record).collect();
    let mut batches = [records];
    let mut calls = 0usize;
    let outcome = store
        .replace_by_parser_prefix_parallel(
            "source",
            "timeline.",
            "identity-small",
            PersistMode::SyncAll,
            limits,
            Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            move || -> ProducerResult {
                calls += 1;
                if calls == 1 {
                    Ok(Some(std::mem::take(&mut batches[0])))
                } else {
                    Ok(None)
                }
            },
        )
        .unwrap();

    assert_eq!(outcome.inserted_records, 10);
    assert_eq!(outcome.deleted_records, 0);
    assert!(outcome.segments >= 4);
    let after = store.read_snapshot();
    assert_eq!(
        store.count_by_time(&after, "source", None, None).unwrap(),
        10
    );
    assert_eq!(store.cursor_revision(&after).unwrap(), revision + 1);
    let marker = read_marker(&after, "source").unwrap();
    assert_eq!(marker.status, TimelineProjectionState::Complete);
    assert_eq!(marker.inserted_events, 10);
}

#[test]
fn producer_failure_leaves_zero_effect_and_rerun_is_exact_and_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let old: Vec<TimelineRecord> = (0..400).map(old_record).collect();
    let keep = vec![keep_record(0), keep_record(1)];
    store
        .upsert_batch(&[old, keep].concat(), PersistMode::SyncAll)
        .unwrap();
    let before = store.read_snapshot();
    let revision = store.cursor_revision(&before).unwrap();

    let limits = SegmentLimits::default_limits();
    let first_batch = vec![new_record(0), new_record(1)];
    let mut calls = 0usize;
    let error = store
        .replace_by_parser_prefix_parallel(
            "source",
            "timeline.",
            "identity-fail",
            PersistMode::SyncAll,
            limits,
            Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            move || -> ProducerResult {
                calls += 1;
                match calls {
                    1 => Ok(Some(first_batch.clone())),
                    _ => Err(std::io::Error::other("injected producer failure")),
                }
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        persistence_fjall::TimelineStreamError::Producer(_)
    ));

    let failed = store.read_snapshot();
    assert_eq!(
        store.count_by_time(&failed, "source", None, None).unwrap(),
        402
    );
    assert!(store.get(&failed, "source", "old-0000").unwrap().is_some());
    assert!(store.get(&failed, "source", "new-0000").unwrap().is_none());
    assert_eq!(store.cursor_revision(&failed).unwrap(), revision);
    assert!(read_marker(&failed, "source").is_none());

    let rerun: Vec<TimelineRecord> = (0..10).map(new_record).collect();
    let mut rerun_calls = 0usize;
    let outcome = store
        .replace_by_parser_prefix_parallel(
            "source",
            "timeline.",
            "identity-fail",
            PersistMode::SyncAll,
            SegmentLimits::default_limits(),
            Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            move || -> ProducerResult {
                rerun_calls += 1;
                if rerun_calls == 1 {
                    Ok(Some(rerun.clone()))
                } else {
                    Ok(None)
                }
            },
        )
        .unwrap();
    assert_eq!(outcome.inserted_records, 10);
    assert_eq!(outcome.deleted_records, 400);

    let after = store.read_snapshot();
    assert_eq!(
        store.count_by_time(&after, "source", None, None).unwrap(),
        12
    );
    let marker = read_marker(&after, "source").unwrap();
    assert_eq!(marker.status, TimelineProjectionState::Complete);

    let second: Vec<TimelineRecord> = (0..10).map(new_record).collect();
    let mut second_calls = 0usize;
    store
        .replace_by_parser_prefix_parallel(
            "source",
            "timeline.",
            "identity-fail",
            PersistMode::SyncAll,
            SegmentLimits::default_limits(),
            Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            move || -> ProducerResult {
                second_calls += 1;
                if second_calls == 1 {
                    Ok(Some(second.clone()))
                } else {
                    Ok(None)
                }
            },
        )
        .unwrap();
    let repeated = store.read_snapshot();
    assert_eq!(
        store
            .count_by_time(&repeated, "source", None, None)
            .unwrap(),
        12
    );
}

#[test]
fn empty_replacement_writes_complete_in_its_own_transaction() {
    let root = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let before = store.read_snapshot();
    let revision = store.cursor_revision(&before).unwrap();

    let outcome = store
        .replace_by_parser_prefix_parallel(
            "source",
            "timeline.",
            "identity-empty",
            PersistMode::SyncAll,
            SegmentLimits::default_limits(),
            Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            || -> ProducerResult { Ok(None) },
        )
        .unwrap();
    assert_eq!(outcome.inserted_records, 0);
    assert_eq!(outcome.deleted_records, 0);
    assert_eq!(outcome.segments, 0);

    let after = store.read_snapshot();
    assert_eq!(
        store.count_by_time(&after, "source", None, None).unwrap(),
        0
    );
    assert_eq!(store.cursor_revision(&after).unwrap(), revision + 1);
    let marker = read_marker(&after, "source").unwrap();
    assert_eq!(marker.status, TimelineProjectionState::Complete);
    assert_eq!(marker.inserted_events, 0);
}

#[test]
fn invalid_replacement_identity_preserves_records_revision_and_marker() {
    for malformed in ["source", "parser", "id"] {
        let root = tempfile::tempdir().unwrap();
        let store = TimelineRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
        let mut initial = Some(vec![old_record(0)]);
        store
            .replace_by_parser_prefix_parallel(
                "source",
                "timeline.",
                "published",
                PersistMode::SyncAll,
                SegmentLimits::default_limits(),
                Default::default(),
                &std::sync::atomic::AtomicBool::new(false),
                || -> ProducerResult { Ok(initial.take()) },
            )
            .unwrap();
        let before = store.read_snapshot();
        let revision = store.cursor_revision(&before).unwrap();
        let mut invalid = new_record(0);
        match malformed {
            "source" => invalid.data_source_id = "another-source".into(),
            "parser" => invalid.parser_id = Some("another.parser".into()),
            "id" => invalid.id.clear(),
            _ => unreachable!(),
        }
        let mut replacement = Some(vec![new_record(1), invalid]);
        assert!(store
            .replace_by_parser_prefix_parallel(
                "source",
                "timeline.",
                "replacement",
                PersistMode::SyncAll,
                SegmentLimits::default_limits(),
                Default::default(),
                &std::sync::atomic::AtomicBool::new(false),
                || -> ProducerResult { Ok(replacement.take()) },
            )
            .is_err());
        let after = store.read_snapshot();
        assert_eq!(store.cursor_revision(&after).unwrap(), revision);
        assert_eq!(
            store.count_by_time(&after, "source", None, None).unwrap(),
            1
        );
        assert!(store.get(&after, "source", "old-0000").unwrap().is_some());
        assert!(store.get(&after, "source", "new-0001").unwrap().is_none());
        let marker = read_marker(&after, "source").unwrap();
        assert_eq!(marker.identity, "published");
        assert_eq!(marker.status, TimelineProjectionState::Complete);
    }
}

#[test]
fn oversized_record_is_rejected_before_replacing_existing_projection() {
    let root = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    store
        .upsert_batch(&[old_record(0)], PersistMode::SyncAll)
        .unwrap();
    let revision = store.cursor_revision(&store.read_snapshot()).unwrap();
    let mut replacement = Some(vec![new_record(0)]);
    assert!(store
        .replace_by_parser_prefix_parallel(
            "source",
            "timeline.",
            "oversized",
            PersistMode::SyncAll,
            SegmentLimits {
                max_bytes: 1,
                max_records: 1
            },
            Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            || -> ProducerResult { Ok(replacement.take()) },
        )
        .is_err());
    let after = store.read_snapshot();
    assert_eq!(store.cursor_revision(&after).unwrap(), revision);
    assert!(store.get(&after, "source", "old-0000").unwrap().is_some());
    assert!(read_marker(&after, "source").is_none());
}
