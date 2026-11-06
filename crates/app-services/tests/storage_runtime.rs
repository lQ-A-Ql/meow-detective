use app_services::source_db;
use app_services::storage_runtime::{
    open_source_read_only, open_source_read_write, source_storage_root, ReadOnlyStorageRuntime,
    ReadWriteStorageRuntime, StorageRuntimeError,
};
use chrono::Utc;
use domain::DataSourceId;
use persistence_fjall::TimelineRecord;
use persistence_fjall::{
    encode_key, CatalogEntryType, FileEntryRecord, KeyPart, KeyspaceName, PersistMode,
    ScanDirection, VersionedEnvelope,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn read_write_runtime_exposes_case_and_source_handles() {
    let root = tempdir().unwrap();
    let runtime = ReadWriteStorageRuntime::open(root.path()).unwrap();
    assert_eq!(runtime.source_root(), root.path());
    assert_eq!(runtime.case().source_root(), root.path());
    assert_eq!(runtime.source().source_root(), root.path());
}

#[test]
fn read_only_runtime_requires_an_existing_storage_layout() {
    let root = tempdir().unwrap();
    assert!(ReadOnlyStorageRuntime::open(root.path()).is_err());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);

    let writable = ReadWriteStorageRuntime::open(root.path()).unwrap();
    drop(writable);
    let readonly = ReadOnlyStorageRuntime::open(root.path()).unwrap();
    assert_eq!(readonly.source_root(), root.path());
}

#[test]
fn source_runtime_uses_the_fixed_case_source_layout() {
    let root = tempdir().unwrap();
    let expected = root
        .path()
        .join("sources")
        .join("source-01")
        .join("source.fjall");
    assert_eq!(
        source_storage_root(root.path(), "source-01").unwrap(),
        expected
    );

    let source = open_source_read_write(root.path(), "source-01").unwrap();
    assert_eq!(source.source_root(), Path::new(&expected));
    assert_eq!(
        source.artifacts().objects_root(),
        root.path().join("artifacts").join("objects")
    );
    drop(source);

    let source = open_source_read_only(root.path(), "source-01").unwrap();
    assert_eq!(source.source_root(), Path::new(&expected));
}

#[test]
fn source_id_must_be_a_single_path_component() {
    let root = tempdir().unwrap();
    for source_id in [
        "",
        ".",
        "..",
        "nested/source",
        r"nested\source",
        r"C:\source",
    ] {
        assert!(matches!(
            source_storage_root(root.path(), source_id),
            Err(StorageRuntimeError::InvalidSourceId(_))
        ));
    }
}

#[test]
fn source_db_facade_opens_the_same_fjall_source_layout() {
    let root = tempdir().unwrap();
    let source_id = DataSourceId("source-02".to_owned());
    let writable =
        source_db::open_registered_fjall_source_read_write(root.path(), &source_id).unwrap();
    assert!(writable
        .source_root()
        .ends_with("sources/source-02/source.fjall"));
    drop(writable);
    let readonly =
        source_db::open_registered_fjall_source_read_only(root.path(), &source_id).unwrap();
    assert_eq!(
        readonly.source_root(),
        source_db::fjall_source_storage_root(root.path(), "source-02").unwrap()
    );
}

#[test]
fn read_only_source_reuses_an_active_write_handle() {
    let root = tempdir().unwrap();
    let writable = open_source_read_write(root.path(), "source-shared").unwrap();
    let readonly = open_source_read_only(root.path(), "source-shared").unwrap();
    let record = FileEntryRecord {
        id: "shared-file".to_owned(),
        parent_id: None,
        data_source_id: "source-shared".to_owned(),
        path: "/shared.txt".to_owned(),
        name: "shared.txt".to_owned(),
        entry_type: CatalogEntryType::File,
        size: Some(1),
        ext: Some("txt".to_owned()),
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: Some(false),
        created: None,
        modified: None,
        accessed: None,
        changed: None,
        hash_sha256: None,
        partition_index: None,
        unix_mode: None,
    };
    writable
        .catalog_upsert(&record, PersistMode::SyncData)
        .unwrap();
    let snapshot = readonly.snapshot();
    assert_eq!(
        readonly
            .catalog_get(&snapshot, "source-shared", "shared-file")
            .unwrap(),
        Some(record)
    );
}

#[test]
fn concurrent_source_opens_share_one_fjall_database() {
    let root = tempdir().unwrap();
    drop(open_source_read_write(root.path(), "source-concurrent").unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let workers = (0..8)
        .map(|_| {
            let root = root.path().to_path_buf();
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                open_source_read_write(root, "source-concurrent").unwrap()
            })
        })
        .collect::<Vec<_>>();
    let handles = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(handles.len(), 8);
}

#[test]
fn source_runtime_reopens_after_handles_are_dropped() {
    let root = tempdir().unwrap();
    drop(open_source_read_write(root.path(), "source-cached").unwrap());
    let reopened = open_source_read_only(root.path(), "source-cached").unwrap();
    assert_eq!(
        reopened.source_root(),
        source_storage_root(root.path(), "source-cached").unwrap()
    );
}

#[test]
fn incomplete_analysis_projection_hides_fjall_source_data_until_complete() {
    let root = tempdir().unwrap();
    let source_id = "source-projection-state";
    let writable = open_source_read_write(root.path(), source_id).unwrap();
    let marker = |family: &str, kind: &str| {
        let key = encode_key(family, &[KeyPart::Text(source_id.to_owned())]).unwrap();
        let envelope = VersionedEnvelope::new(
            kind,
            1,
            format!("projection:{source_id}"),
            source_id,
            1,
            source_id.as_bytes().to_vec(),
        );
        writable
            .write_envelope(KeyspaceName::Meta, key, &envelope, PersistMode::SyncAll)
            .unwrap();
    };
    marker("projection", "source_projection_complete");
    let readonly = open_source_read_only(root.path(), source_id).unwrap();
    assert!(readonly
        .source_projection_complete(&readonly.snapshot(), source_id)
        .unwrap());

    marker("projection", "source_projection_building");
    assert!(!readonly
        .source_projection_complete(&readonly.snapshot(), source_id)
        .unwrap());
    marker("projection", "source_projection_complete");

    marker("analysis_projection_state", "analysis_projection_building");
    assert!(!readonly
        .source_projection_complete(&readonly.snapshot(), source_id)
        .unwrap());

    marker("analysis_projection", "analysis_projection_complete");
    marker("analysis_projection_state", "analysis_projection_complete");
    assert!(readonly
        .source_projection_complete(&readonly.snapshot(), source_id)
        .unwrap());
}

#[test]
fn source_facade_reads_and_writes_catalog_records_with_keyset_pages() {
    let root = tempdir().unwrap();
    let source = open_source_read_write(root.path(), "source-catalog").unwrap();
    let record = FileEntryRecord {
        id: "file-1".to_owned(),
        parent_id: Some("root".to_owned()),
        data_source_id: "source-catalog".to_owned(),
        path: "/note.txt".to_owned(),
        name: "note.txt".to_owned(),
        entry_type: CatalogEntryType::File,
        size: Some(4),
        ext: Some("txt".to_owned()),
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: Some(false),
        created: Some(Utc::now()),
        modified: None,
        accessed: None,
        changed: None,
        hash_sha256: Some("a".repeat(64)),
        partition_index: Some(0),
        unix_mode: None,
    };
    source
        .catalog_upsert(&record, PersistMode::SyncData)
        .unwrap();
    let snapshot = source.snapshot();
    let page = source
        .catalog_list_by_parent(
            &snapshot,
            "source-catalog",
            Some("root"),
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap();
    assert_eq!(page.records, vec![record.clone()]);
    assert_eq!(
        source
            .catalog_get(&snapshot, "source-catalog", "file-1")
            .unwrap(),
        Some(record)
    );
}

#[test]
fn source_facade_reads_and_writes_timeline_records() {
    let root = tempdir().unwrap();
    let source = open_source_read_write(root.path(), "source-timeline").unwrap();
    let event = TimelineRecord {
        id: "event-1".to_owned(),
        data_source_id: "source-timeline".to_owned(),
        source_object_id: "file-1".to_owned(),
        event_type: "FILE_MODIFIED".to_owned(),
        timestamp: Utc::now(),
        title: "Changed".to_owned(),
        description: "Changed during test".to_owned(),
        parser_id: Some("test".to_owned()),
        parser_version: Some("1".to_owned()),
        confidence: Some(1.0),
        source_attribution: Some("fixture".to_owned()),
        attrs: BTreeMap::from([(String::from("key"), json!("value"))]),
    };
    source
        .timeline_upsert(&event, PersistMode::SyncData)
        .unwrap();
    let snapshot = source.snapshot();
    let page = source
        .timeline_list_by_type(
            &snapshot,
            "source-timeline",
            "FILE_MODIFIED",
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap();
    assert_eq!(page.records, vec![event]);
}
