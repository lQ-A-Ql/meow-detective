use chrono::{TimeZone, Utc};
use persistence_fjall::{
    CatalogEntryType, CatalogRecordStore, DatabaseConfig, FileEntryRecord, PersistMode,
    ScanDirection, TimelineRecord, TimelineRecordStore,
};
use serde_json::json;
use std::collections::BTreeMap;

fn file_record(
    source: &str,
    id: &str,
    parent_id: Option<&str>,
    name: &str,
    path: &str,
    size: u64,
    hash: &str,
) -> FileEntryRecord {
    FileEntryRecord {
        id: id.to_owned(),
        parent_id: parent_id.map(str::to_owned),
        data_source_id: source.to_owned(),
        path: path.to_owned(),
        name: name.to_owned(),
        entry_type: CatalogEntryType::File,
        size: Some(size),
        ext: Some("txt".to_owned()),
        deleted: false,
        hidden: false,
        system: false,
        read_only: true,
        archive: true,
        encrypted: false,
        created: Some(Utc.timestamp_opt(1_700_000_000, 0).single().unwrap()),
        modified: Some(Utc.timestamp_opt(1_700_000_100, 0).single().unwrap()),
        accessed: None,
        changed: None,
        hash_sha256: Some(hash.to_owned()),
        partition_index: Some(2),
        unix_mode: Some(0o644),
    }
}

fn timeline_record(source: &str, id: &str, event_type: &str, seconds: i64) -> TimelineRecord {
    TimelineRecord {
        id: id.to_owned(),
        data_source_id: source.to_owned(),
        source_object_id: format!("file-{id}"),
        event_type: event_type.to_owned(),
        timestamp: Utc.timestamp_opt(seconds, 0).single().unwrap(),
        title: format!("title-{id}"),
        description: format!("description-{id}"),
        parser_id: Some("parser-v1".to_owned()),
        parser_version: Some("1".to_owned()),
        confidence: Some(0.75),
        source_attribution: Some("fixture".to_owned()),
        attrs: BTreeMap::from([(String::from("source"), json!(source))]),
    }
}

#[test]
fn catalog_oracle_preserves_all_fields_and_source_scoped_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let store = CatalogRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let root = file_record("source-a", "root", None, "root", "/", 0, "root-hash");
    let child = file_record(
        "source-a",
        "child",
        Some("root"),
        "report.txt",
        "/report.txt",
        42,
        "child-hash",
    );
    let foreign = file_record(
        "source-b",
        "child",
        Some("root"),
        "report.txt",
        "/report.txt",
        99,
        "foreign-hash",
    );
    for record in [&root, &child, &foreign] {
        store.upsert(record, PersistMode::SyncData).unwrap();
    }

    let snapshot = store.read_snapshot();
    assert_eq!(
        store.get(&snapshot, "source-a", "child").unwrap(),
        Some(child.clone())
    );
    assert_eq!(
        store
            .list_by_parent(
                &snapshot,
                "source-a",
                Some("root"),
                None,
                ScanDirection::Forward,
                10,
            )
            .unwrap()
            .records,
        vec![child.clone()]
    );
    assert_eq!(
        store
            .find_by_path(
                &snapshot,
                "source-a",
                "/report.txt",
                None,
                ScanDirection::Forward,
                10,
            )
            .unwrap()
            .records,
        vec![child.clone()]
    );
    assert_eq!(
        store
            .find_by_hash(
                &snapshot,
                "source-a",
                "child-hash",
                None,
                ScanDirection::Forward,
                10,
            )
            .unwrap()
            .records,
        vec![child]
    );
    assert_eq!(
        store
            .find_by_path(
                &snapshot,
                "source-a",
                "/report.txt",
                None,
                ScanDirection::Forward,
                10,
            )
            .unwrap()
            .records
            .len(),
        1
    );
    assert_eq!(
        store
            .find_by_path(
                &snapshot,
                "source-b",
                "/report.txt",
                None,
                ScanDirection::Forward,
                10,
            )
            .unwrap()
            .records,
        vec![foreign]
    );
}

#[test]
fn timeline_oracle_preserves_fields_order_and_source_isolation() {
    let directory = tempfile::tempdir().unwrap();
    let store = TimelineRecordStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let old = timeline_record("source-a", "old", "FILE_MODIFIED", 10);
    let newest = timeline_record("source-a", "newest", "FILE_CREATED", 30);
    let middle = timeline_record("source-a", "middle", "FILE_MODIFIED", 20);
    let foreign = timeline_record("source-b", "foreign", "FILE_CREATED", 40);
    for record in [&old, &newest, &middle, &foreign] {
        store.upsert(record, PersistMode::SyncData).unwrap();
    }

    let snapshot = store.read_snapshot();
    assert_eq!(
        store.get(&snapshot, "source-a", "middle").unwrap(),
        Some(middle.clone())
    );
    let page = store
        .list_by_time(&snapshot, "source-a", None, ScanDirection::Forward, 10)
        .unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["newest", "middle", "old"]
    );
    assert_eq!(page.records[0], newest);
    assert_eq!(page.records[1], middle);
    assert_eq!(page.records[2], old);
    assert_eq!(
        store
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
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["middle", "old"]
    );
    assert_eq!(
        store
            .list_by_time(&snapshot, "source-b", None, ScanDirection::Forward, 10)
            .unwrap()
            .records,
        vec![foreign]
    );
}
