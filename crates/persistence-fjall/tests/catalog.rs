use chrono::Utc;
use persistence_fjall::{
    CatalogEntryType, CatalogRecordStore, DatabaseConfig, FileEntryRecord, PersistMode,
    ScanDirection,
};

fn record(
    source: &str,
    id: &str,
    parent_id: Option<&str>,
    name: &str,
    path: &str,
) -> FileEntryRecord {
    FileEntryRecord {
        id: id.to_owned(),
        parent_id: parent_id.map(str::to_owned),
        data_source_id: source.to_owned(),
        path: path.to_owned(),
        name: name.to_owned(),
        entry_type: CatalogEntryType::File,
        size: Some(42),
        ext: Some("txt".to_owned()),
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: false,
        created: Some(Utc::now()),
        modified: None,
        accessed: None,
        changed: None,
        hash_sha256: Some("abc".to_owned()),
        partition_index: Some(1),
        unix_mode: Some(0o644),
    }
}

#[test]
fn upsert_updates_indexes_and_removes_old_entries() {
    let directory = tempfile::tempdir().unwrap();
    let store = CatalogRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let mut entry = record("source-a", "file-1", Some("dir-1"), "one.txt", "/one.txt");
    store.upsert(&entry, PersistMode::SyncData).unwrap();
    entry.name = "two.txt".to_owned();
    entry.path = "/two.txt".to_owned();
    entry.hash_sha256 = Some("def".to_owned());
    store.upsert(&entry, PersistMode::SyncData).unwrap();
    let snapshot = store.read_snapshot();
    assert!(store
        .find_by_path(
            &snapshot,
            "source-a",
            "/one.txt",
            None,
            ScanDirection::Forward,
            10
        )
        .unwrap()
        .records
        .is_empty());
    assert_eq!(
        store
            .find_by_path(
                &snapshot,
                "source-a",
                "/two.txt",
                None,
                ScanDirection::Forward,
                10
            )
            .unwrap()
            .records,
        vec![entry.clone()]
    );
    assert!(store
        .find_by_hash(
            &snapshot,
            "source-a",
            "abc",
            None,
            ScanDirection::Forward,
            10
        )
        .unwrap()
        .records
        .is_empty());
    assert_eq!(
        store
            .find_by_hash(
                &snapshot,
                "source-a",
                "def",
                None,
                ScanDirection::Forward,
                10
            )
            .unwrap()
            .records,
        vec![entry]
    );
}

#[test]
fn delete_removes_record_and_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let store = CatalogRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let entry = record("source-a", "file-1", None, "one.txt", "/one.txt");
    store.upsert(&entry, PersistMode::SyncData).unwrap();
    assert!(store
        .delete("source-a", "file-1", PersistMode::SyncData)
        .unwrap());
    let snapshot = store.read_snapshot();
    assert!(store
        .get(&snapshot, "source-a", "file-1")
        .unwrap()
        .is_none());
    assert!(store
        .list_by_parent(
            &snapshot,
            "source-a",
            None,
            None,
            ScanDirection::Forward,
            10
        )
        .unwrap()
        .records
        .is_empty());
}

#[test]
fn parent_listing_is_keyset_paginated_and_source_isolated() {
    let directory = tempfile::tempdir().unwrap();
    let store = CatalogRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    for id in ["a", "b", "c"] {
        store
            .upsert(
                &record("source-a", id, Some("dir"), id, &format!("/{id}")),
                PersistMode::SyncData,
            )
            .unwrap();
    }
    store
        .upsert(
            &record("source-b", "other", Some("dir"), "other", "/other"),
            PersistMode::SyncData,
        )
        .unwrap();
    let snapshot = store.read_snapshot();
    let first = store
        .list_by_parent(
            &snapshot,
            "source-a",
            Some("dir"),
            None,
            ScanDirection::Forward,
            2,
        )
        .unwrap();
    assert_eq!(first.records.len(), 2);
    let second = store
        .list_by_parent(
            &snapshot,
            "source-a",
            Some("dir"),
            first.next_cursor.as_deref(),
            ScanDirection::Forward,
            2,
        )
        .unwrap();
    assert_eq!(second.records.len(), 1);
    assert!(store
        .list_by_parent(
            &snapshot,
            "source-b",
            Some("dir"),
            None,
            ScanDirection::Forward,
            10
        )
        .unwrap()
        .records
        .iter()
        .all(|item| item.data_source_id == "source-b"));
}

#[test]
fn batch_upsert_preserves_parent_and_orphan_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let store = CatalogRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let records = vec![
        record("source-a", "parent", None, "parent", "/parent"),
        record(
            "source-a",
            "child",
            Some("parent"),
            "child",
            "/parent/child",
        ),
    ];
    store.upsert_batch(&records, PersistMode::SyncData).unwrap();
    let snapshot = store.read_snapshot();
    assert_eq!(
        store
            .list_by_parent(
                &snapshot,
                "source-a",
                Some("parent"),
                None,
                ScanDirection::Forward,
                10,
            )
            .unwrap()
            .records,
        vec![records[1].clone()]
    );
    assert!(store
        .list_orphan_directories(&snapshot, "source-a", None, ScanDirection::Forward, 10,)
        .unwrap()
        .records
        .is_empty());
}
