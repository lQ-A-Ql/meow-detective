use persistence_fjall::{
    DatabaseConfig, PartitionRecord, PartitionRecordStore, PersistMode, ScanDirection,
};

fn record(source: &str, id: &str, index: u32) -> PartitionRecord {
    PartitionRecord {
        id: id.to_owned(),
        data_source_id: source.to_owned(),
        partition_index: index,
        name: format!("partition-{index}"),
        kind_label: "filesystem".to_owned(),
        status: "ready".to_owned(),
        type_guid: Some(format!("guid-{index}")),
        offset: 4096 * index as u64,
        length: 8192,
        filesystem: Some("ntfs".to_owned()),
        unlock_hint: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        lvm_pv_offsets_json: None,
        lvm_pv_sources_json: None,
    }
}

#[test]
fn partition_records_are_source_scoped_and_keyset_indexed() {
    let dir = tempfile::tempdir().unwrap();
    let store = PartitionRecordStore::open(dir.path(), DatabaseConfig::default()).unwrap();
    store
        .upsert(&record("source-a", "p0", 0), PersistMode::SyncData)
        .unwrap();
    store
        .upsert(&record("source-a", "p1", 1), PersistMode::SyncData)
        .unwrap();
    store
        .upsert(&record("source-b", "other", 0), PersistMode::SyncData)
        .unwrap();
    let snapshot = store.read_snapshot();
    assert_eq!(
        store
            .find_by_data_source_and_index(&snapshot, "source-a", 1)
            .unwrap()
            .unwrap()
            .id,
        "p1"
    );
    let page = store
        .list_by_source(&snapshot, "source-a", None, ScanDirection::Forward, 1)
        .unwrap();
    assert_eq!(page.records.len(), 1);
    let next = store
        .list_by_source(
            &snapshot,
            "source-a",
            page.next_cursor.as_deref(),
            ScanDirection::Forward,
            1,
        )
        .unwrap();
    assert_eq!(next.records.len(), 1);
    assert!(store
        .list_by_source(&snapshot, "source-b", None, ScanDirection::Forward, 10)
        .unwrap()
        .records
        .iter()
        .all(|r| r.data_source_id == "source-b"));
}

#[test]
fn partition_delete_writes_tombstone_and_clears_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let store = PartitionRecordStore::open(dir.path(), DatabaseConfig::default()).unwrap();
    store
        .upsert(&record("source-a", "p0", 0), PersistMode::SyncData)
        .unwrap();
    assert!(store
        .delete("source-a", "p0", PersistMode::SyncData)
        .unwrap());
    let snapshot = store.read_snapshot();
    assert!(store.get(&snapshot, "source-a", "p0").unwrap().is_none());
    assert!(store
        .find_by_data_source_and_index(&snapshot, "source-a", 0)
        .unwrap()
        .is_none());
}

#[test]
fn partition_replace_removes_stale_rows_and_preserves_other_sources() {
    let dir = tempfile::tempdir().unwrap();
    let store = PartitionRecordStore::open(dir.path(), DatabaseConfig::default()).unwrap();
    store
        .replace_by_source(
            "source-a",
            &[record("source-a", "old", 0), record("source-a", "kept", 1)],
            PersistMode::SyncData,
        )
        .unwrap();
    store
        .replace_by_source(
            "source-b",
            &[record("source-b", "other", 0)],
            PersistMode::SyncData,
        )
        .unwrap();

    let replacement = record("source-a", "new", 1);
    store
        .replace_by_source(
            "source-a",
            std::slice::from_ref(&replacement),
            PersistMode::SyncData,
        )
        .unwrap();
    store
        .replace_by_source("source-a", &[replacement], PersistMode::SyncData)
        .unwrap();

    let snapshot = store.read_snapshot();
    let source_a = store
        .list_by_source(&snapshot, "source-a", None, ScanDirection::Forward, 10)
        .unwrap();
    assert_eq!(source_a.records.len(), 1);
    assert_eq!(source_a.records[0].id, "new");
    assert!(store.get(&snapshot, "source-a", "old").unwrap().is_none());
    assert!(store.get(&snapshot, "source-a", "kept").unwrap().is_none());
    assert_eq!(
        store
            .list_by_source(&snapshot, "source-b", None, ScanDirection::Forward, 10)
            .unwrap()
            .records
            .len(),
        1
    );
}
