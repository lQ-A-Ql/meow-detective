use app_services::source_db;
use app_services::storage_runtime::{
    open_source_read_only, open_source_read_write, source_storage_root, ReadOnlyStorageRuntime,
    ReadWriteStorageRuntime, StorageRuntimeError,
};
use chrono::Utc;
use domain::DataSourceId;
use persistence_fjall::{CatalogEntryType, FileEntryRecord, PersistMode, ScanDirection};
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
        encrypted: false,
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
