use app_services::storage_runtime::{ReadOnlyStorageRuntime, ReadWriteStorageRuntime};
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
