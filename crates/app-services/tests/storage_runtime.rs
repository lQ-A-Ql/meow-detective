use app_services::storage_runtime::{
    open_source_read_only, open_source_read_write, source_storage_root, ReadOnlyStorageRuntime,
    ReadWriteStorageRuntime, StorageRuntimeError,
};
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
