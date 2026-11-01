use std::fs;

use artifact_storage::{content_address, safe_relative_path, ArtifactStorageError, ArtifactStore};
use tempfile::tempdir;

#[test]
fn writes_reads_and_verifies_content_addressed_object() {
    let root = tempdir().unwrap();
    let store = ArtifactStore::new(root.path()).unwrap();
    let bytes = b"forensic artifact";
    let stored = store.put_bytes(bytes).unwrap();
    assert!(!stored.reused);
    assert_eq!(stored.length, bytes.len() as u64);
    assert_eq!(store.read(&stored.address).unwrap(), bytes);
    assert_eq!(store.verify(&stored.address).unwrap(), bytes.len() as u64);
    assert!(stored.path.ends_with(format!(
        "{}/{}/{}",
        &stored.address.as_str()[..2],
        &stored.address.as_str()[2..4],
        stored.address
    )));
}

#[test]
fn reuses_identical_object_and_rejects_conflicting_existing_object() {
    let root = tempdir().unwrap();
    let store = ArtifactStore::new(root.path()).unwrap();
    let first = store.put_bytes(b"same bytes").unwrap();
    let second = store.put_bytes(b"same bytes").unwrap();
    assert!(second.reused);
    assert_eq!(first.path, second.path);

    fs::write(&first.path, b"tampered").unwrap();
    let error = store.put_bytes(b"same bytes").unwrap_err();
    assert!(matches!(error, ArtifactStorageError::ObjectConflict { .. }));
}

#[test]
fn detects_tampering_when_reading() {
    let root = tempdir().unwrap();
    let store = ArtifactStore::new(root.path()).unwrap();
    let stored = store.put_bytes(b"immutable").unwrap();
    fs::write(&stored.path, b"changed").unwrap();
    let error = store.read(&stored.address).unwrap_err();
    assert!(matches!(error, ArtifactStorageError::DigestMismatch { .. }));
}

#[test]
fn rejects_absolute_and_parent_relative_paths() {
    let root = tempdir().unwrap();
    assert!(safe_relative_path(root.path(), "nested/file.bin").is_ok());
    assert!(safe_relative_path(root.path(), "../outside.bin").is_err());
    assert!(safe_relative_path(root.path(), r"nested\outside.bin").is_err());
    assert!(safe_relative_path(root.path(), r"C:\outside.bin").is_err());
}

#[test]
fn content_address_is_stable() {
    assert_eq!(
        content_address(b"").as_str(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn explicit_address_write_rejects_wrong_content() {
    let root = tempdir().unwrap();
    let store = ArtifactStore::new(root.path()).unwrap();
    let address = content_address(b"expected");
    let error = store.write(&address, b"different").unwrap_err();
    assert!(matches!(error, ArtifactStorageError::DigestMismatch { .. }));
    assert_eq!(store.write(&address, b"expected").unwrap().address, address);
}
