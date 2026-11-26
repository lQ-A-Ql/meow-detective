#[test]
#[cfg(windows)]
fn database_opens_writes_and_reopens_beyond_windows_legacy_path_length() {
    use persistence_fjall::{DatabaseConfig, KeyspaceName, PersistMode, SourceStore};
    let root = tempfile::tempdir().unwrap();
    let path = root
        .path()
        .join("case-".to_owned() + &"c".repeat(70))
        .join("source-".to_owned() + &"s".repeat(70))
        .join("generation-".to_owned() + &"g".repeat(70));
    assert!(path.to_string_lossy().encode_utf16().count() > 260);
    {
        let store = SourceStore::open(&path, DatabaseConfig::default()).unwrap();
        store
            .write_raw(
                KeyspaceName::Meta,
                b"key".to_vec(),
                b"value".to_vec(),
                PersistMode::SyncAll,
            )
            .unwrap();
    }
    let store = SourceStore::open(&path, DatabaseConfig::default()).unwrap();
    let snapshot = store.database().read_snapshot();
    assert_eq!(
        store
            .read_raw(&snapshot, KeyspaceName::Meta, b"key")
            .unwrap(),
        Some(b"value".to_vec())
    );
}
