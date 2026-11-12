use persistence_fjall::{DatabaseConfig, KeyspaceName, PersistMode, SourceStore, WriteAction};

fn put(keyspace: KeyspaceName, key: &[u8], value: &[u8]) -> WriteAction {
    WriteAction::Put {
        keyspace,
        key: key.to_vec(),
        value: value.to_vec(),
    }
}

#[test]
fn clone_to_copies_paged_keyspaces_into_an_independent_store() {
    let source_root = tempfile::tempdir().unwrap();
    let clone_root = tempfile::tempdir().unwrap();
    let source = SourceStore::open(source_root.path(), DatabaseConfig::default()).unwrap();
    let mut actions = (0..600u64)
        .map(|index| {
            let key = format!("catalog/{index:04}");
            let value = format!("value-{index}");
            put(
                KeyspaceName::CatalogRecords,
                key.as_bytes(),
                value.as_bytes(),
            )
        })
        .collect::<Vec<_>>();
    actions.push(put(KeyspaceName::Meta, b"clone/marker", b"source"));
    source
        .database()
        .write_batch(&actions, PersistMode::SyncAll)
        .unwrap();

    let clone_path = clone_root.path().join("source.fjall.clone");
    std::fs::create_dir_all(&clone_path).unwrap();
    let clone = source
        .clone_to(&clone_path, DatabaseConfig::default())
        .unwrap();
    assert_ne!(source.database().path(), clone.database().path());
    let snapshot = clone.database().read_snapshot();
    for index in [0u64, 511, 512, 599] {
        assert_eq!(
            snapshot
                .get(
                    KeyspaceName::CatalogRecords,
                    format!("catalog/{index:04}").as_bytes(),
                )
                .unwrap(),
            Some(format!("value-{index}").into_bytes())
        );
    }
    assert_eq!(
        snapshot.get(KeyspaceName::Meta, b"clone/marker").unwrap(),
        Some(b"source".to_vec())
    );
    drop(snapshot);

    clone
        .write_raw(
            KeyspaceName::Meta,
            b"clone/marker".to_vec(),
            b"clone".to_vec(),
            PersistMode::SyncAll,
        )
        .unwrap();
    let source_snapshot = source.database().read_snapshot();
    assert_eq!(
        source_snapshot
            .get(KeyspaceName::Meta, b"clone/marker")
            .unwrap(),
        Some(b"source".to_vec())
    );
    assert_eq!(
        source_snapshot
            .get(KeyspaceName::CatalogRecords, b"catalog/0599")
            .unwrap(),
        Some(b"value-599".to_vec())
    );
    drop(source_snapshot);
    drop(clone);

    let reopened = SourceStore::open(&clone_path, DatabaseConfig::default()).unwrap();
    let snapshot = reopened.database().read_snapshot();
    assert_eq!(
        snapshot.get(KeyspaceName::Meta, b"clone/marker").unwrap(),
        Some(b"clone".to_vec())
    );
    assert_eq!(
        snapshot
            .get(KeyspaceName::CatalogRecords, b"catalog/0000")
            .unwrap(),
        Some(b"value-0".to_vec())
    );
}
