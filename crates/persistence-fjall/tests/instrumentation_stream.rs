use persistence_fjall::{
    instrumentation::ProcessMeasurement, ArtifactRecord, ArtifactRecordStore, DatabaseConfig,
    PersistMode,
};

#[test]
fn streamed_deletion_records_only_committed_write_work() {
    let directory = tempfile::tempdir().unwrap();
    let store = ArtifactRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let records = (0..4)
        .map(|index| ArtifactRecord {
            id: format!("record-{index}"),
            data_source_id: "source".into(),
            artifact_type: "example".into(),
            source_object_id: Some("object".into()),
            content_sha256: None,
            attrs: serde_json::json!({}),
        })
        .collect::<Vec<_>>();
    store.upsert_batch(&records, PersistMode::Buffer).unwrap();
    let measurement = ProcessMeasurement::start().unwrap();
    assert_eq!(
        store
            .delete_by_source_object_where("source", "object", PersistMode::SyncAll, |_| true)
            .unwrap(),
        4
    );
    let counters = measurement.snapshot();
    assert_eq!(counters.write_batches, 1);
    assert_eq!(counters.sync_all_persists, 1);
    assert!(counters.written_actions >= 4);
    assert!(counters.bytes_written > 0);
}
