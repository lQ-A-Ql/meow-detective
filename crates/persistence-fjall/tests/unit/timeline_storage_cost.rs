use super::*;
use crate::{DatabaseConfig, KeyspaceName, PersistMode};
use chrono::{TimeZone, Utc};

fn record() -> TimelineRecord {
    TimelineRecord {
        id: "event".into(),
        data_source_id: "source".into(),
        source_object_id: "file".into(),
        event_type: "FILE_MODIFIED".into(),
        timestamp: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        title: "Modified".into(),
        description: "documents/file.txt modified".into(),
        parser_id: Some("timeline.file_modified".into()),
        parser_version: Some("2".into()),
        confidence: Some(1.0),
        source_attribution: None,
        attrs: Default::default(),
    }
}

#[test]
fn storage_cost_matches_actual_record_and_index_write_actions() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let record = record();
    let cost = TimelineRecordStore::record_storage_cost(&record).unwrap();
    crate::instrumentation::enable();
    crate::instrumentation::reset();
    store
        .upsert_batch_independent(std::slice::from_ref(&record), PersistMode::Buffer)
        .unwrap();
    let written = crate::instrumentation::snapshot();
    crate::instrumentation::disable();
    assert_eq!(cost.entries, 6);
    assert_eq!(cost.entries, written.written_actions);
    assert_eq!(cost.write_bytes, written.bytes_written);
    let mut deletes = vec![WriteAction::Delete {
        keyspace: KeyspaceName::TimelineRecords,
        key: super::super::record_key(&record.data_source_id, &record.id).unwrap(),
    }];
    deletes.extend(
        index_keys(&record)
            .unwrap()
            .into_iter()
            .map(|key| WriteAction::Delete {
                keyspace: KeyspaceName::TimelineIndexes,
                key,
            }),
    );
    let delete_bytes = deletes
        .iter()
        .map(|action| match action {
            WriteAction::Delete { key, .. } => key.len() as u64,
            _ => unreachable!(),
        })
        .sum::<u64>();
    assert_eq!(cost.delete_bytes, delete_bytes);
}

#[test]
fn long_descriptions_are_counted_in_payload_and_cluster_index() {
    let original = record();
    let before = TimelineRecordStore::record_storage_cost(&original).unwrap();
    let mut longer = original.clone();
    longer.description.push_str(&"x".repeat(4_096));
    let after = TimelineRecordStore::record_storage_cost(&longer).unwrap();
    assert_eq!(after.entries, before.entries);
    assert_eq!(after.delete_bytes - before.delete_bytes, 4_096);
    assert!(after.write_bytes - before.write_bytes >= 8_192);
}

#[test]
fn invalid_record_identity_is_rejected_before_estimation() {
    let mut record = record();
    record.id.clear();
    assert!(TimelineRecordStore::record_storage_cost(&record).is_err());
}
