use persistence_fjall::{
    decode_key, encode_key, verify_sealed, DatabaseConfig, FjallStorageError, KeyPart,
    KeyspaceName, PersistMode, ScanBounds, ScanDirection, SingleWriterTxDatabase, SourceBuild,
    SourceBuildLayout, SourceStore, VersionedEnvelope, WriteAction,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Payload {
    value: String,
}

#[test]
fn key_codec_preserves_length_prefixed_parts_and_order() {
    let key = encode_key(
        "file",
        &[
            KeyPart::Text("a/b".to_owned()),
            KeyPart::U64(9),
            KeyPart::InverseU64(4),
        ],
    )
    .unwrap();
    let decoded = decode_key(&key).unwrap();
    assert_eq!(decoded.0, "file");
    assert_eq!(
        decoded.1,
        vec![
            KeyPart::Bytes(b"a/b".to_vec()),
            KeyPart::U64(9),
            KeyPart::InverseU64(4)
        ]
    );
}

#[test]
fn envelope_round_trips_payload_and_rejects_tampering() {
    let payload = Payload {
        value: "evidence".to_owned(),
    };
    let envelope =
        VersionedEnvelope::from_serializable("file", 1, "record", "source", 7, &payload).unwrap();
    let bytes = envelope.encode().unwrap();
    let decoded = VersionedEnvelope::decode(&bytes).unwrap();
    assert_eq!(decoded.decode_payload::<Payload>().unwrap(), payload);
    let mut tampered = bytes;
    *tampered.last_mut().unwrap() ^= 1;
    assert!(matches!(
        VersionedEnvelope::decode(&tampered),
        Err(FjallStorageError::EnvelopeDigestMismatch | FjallStorageError::Deserialization(_))
    ));
}

#[test]
fn database_commits_across_keyspaces_and_reopens() {
    let directory = tempfile::tempdir().unwrap();
    let database =
        SingleWriterTxDatabase::open(directory.path(), DatabaseConfig::default()).unwrap();
    let actions = vec![
        WriteAction::Put {
            keyspace: KeyspaceName::Meta,
            key: b"revision".to_vec(),
            value: b"1".to_vec(),
        },
        WriteAction::Put {
            keyspace: KeyspaceName::Batches,
            key: b"batch".to_vec(),
            value: b"done".to_vec(),
        },
    ];
    assert_eq!(
        database
            .write_batch(&actions, PersistMode::SyncData)
            .unwrap()
            .operation_count,
        2
    );
    let snapshot = database.read_snapshot();
    assert_eq!(
        snapshot.get(KeyspaceName::Meta, b"revision").unwrap(),
        Some(b"1".to_vec())
    );
    assert_eq!(
        snapshot.get(KeyspaceName::Batches, b"batch").unwrap(),
        Some(b"done".to_vec())
    );
    drop(snapshot);
    drop(database);
    let reopened =
        SingleWriterTxDatabase::open(directory.path(), DatabaseConfig::default()).unwrap();
    assert_eq!(
        reopened
            .read_snapshot()
            .get(KeyspaceName::Batches, b"batch")
            .unwrap(),
        Some(b"done".to_vec())
    );
}

#[test]
fn second_open_is_reported_as_storage_locked() {
    let directory = tempfile::tempdir().unwrap();
    let _database =
        SingleWriterTxDatabase::open(directory.path(), DatabaseConfig::default()).unwrap();
    let error = match SingleWriterTxDatabase::open(directory.path(), DatabaseConfig::default()) {
        Ok(_) => panic!("second database handle unexpectedly opened"),
        Err(error) => error,
    };
    assert!(matches!(error, FjallStorageError::StorageLocked { .. }));
}

#[test]
fn source_store_writes_and_reads_envelope() {
    let directory = tempfile::tempdir().unwrap();
    let store = SourceStore::open(directory.path(), DatabaseConfig::default()).unwrap();
    let envelope = VersionedEnvelope::new("artifact", 1, "id", "source", 1, b"payload".to_vec());
    store
        .write_envelope(
            KeyspaceName::ArtifactRecords,
            b"id".to_vec(),
            &envelope,
            PersistMode::SyncAll,
        )
        .unwrap();
    let snapshot = store.database().read_snapshot();
    assert_eq!(
        store
            .read_envelope(&snapshot, KeyspaceName::ArtifactRecords, b"id")
            .unwrap(),
        Some(envelope)
    );
}

#[test]
fn source_build_seals_and_publishes_after_sync_all() {
    let directory = tempfile::tempdir().unwrap();
    let layout = SourceBuildLayout::new(directory.path(), "source-1", "attempt-1").unwrap();
    let mut build = SourceBuild::create(layout.clone()).unwrap();
    build.write_file("records/data.bin", b"record").unwrap();
    build
        .open_database(DatabaseConfig::default())
        .unwrap()
        .write_batch(
            &[WriteAction::Put {
                keyspace: KeyspaceName::Meta,
                key: b"ready".to_vec(),
                value: b"yes".to_vec(),
            }],
            PersistMode::SyncData,
        )
        .unwrap();
    let receipt = build.seal_and_publish().unwrap();
    assert!(!receipt.reused);
    assert_eq!(
        verify_sealed(&receipt.path).unwrap(),
        receipt.manifest_digest
    );
    assert!(layout.published_path().is_dir());
    assert!(!layout.build_path().exists());
}

#[test]
fn repeated_publish_reuses_matching_published_source() {
    let directory = tempfile::tempdir().unwrap();
    let first_layout = SourceBuildLayout::new(directory.path(), "source-1", "attempt-1").unwrap();
    let first = SourceBuild::create(first_layout.clone()).unwrap();
    first.write_file("record", b"same").unwrap();
    let first_receipt = first.seal_and_publish().unwrap();

    let second_layout = SourceBuildLayout::new(directory.path(), "source-1", "attempt-2").unwrap();
    let second = SourceBuild::create(second_layout).unwrap();
    second.write_file("record", b"same").unwrap();
    let second_receipt = second.seal_and_publish().unwrap();
    assert!(second_receipt.reused);
    assert_eq!(
        first_receipt.manifest_digest,
        second_receipt.manifest_digest
    );
}

#[test]
fn missing_seal_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let layout = SourceBuildLayout::new(directory.path(), "source-1", "attempt-1").unwrap();
    std::fs::create_dir_all(layout.published_path()).unwrap();
    std::fs::write(layout.published_path().join("record"), b"data").unwrap();
    assert!(matches!(
        verify_sealed(layout.published_path()),
        Err(FjallStorageError::MissingSeal(_))
    ));
}

#[test]
fn snapshot_prefix_and_range_return_ordered_copies() {
    let directory = tempfile::tempdir().unwrap();
    let database =
        SingleWriterTxDatabase::open(directory.path(), DatabaseConfig::default()).unwrap();
    let actions = [
        (b"file/a".to_vec(), b"a".to_vec()),
        (b"file/b".to_vec(), b"b".to_vec()),
        (b"file/c".to_vec(), b"c".to_vec()),
        (b"other/a".to_vec(), b"x".to_vec()),
    ]
    .into_iter()
    .map(|(key, value)| WriteAction::Put {
        keyspace: KeyspaceName::CatalogRecords,
        key,
        value,
    })
    .collect::<Vec<_>>();
    database
        .write_batch(&actions, PersistMode::SyncData)
        .unwrap();
    let snapshot = database.read_snapshot();
    let forward = snapshot
        .prefix(
            KeyspaceName::CatalogRecords,
            b"file/",
            ScanDirection::Forward,
        )
        .unwrap();
    assert_eq!(
        forward
            .iter()
            .map(|entry| entry.key.as_slice())
            .collect::<Vec<_>>(),
        vec![
            b"file/a".as_slice(),
            b"file/b".as_slice(),
            b"file/c".as_slice()
        ]
    );
    let reverse = snapshot
        .prefix(
            KeyspaceName::CatalogRecords,
            b"file/",
            ScanDirection::Reverse,
        )
        .unwrap();
    assert_eq!(reverse[0].key, b"file/c");
    let bounded = snapshot
        .range(
            KeyspaceName::CatalogRecords,
            ScanBounds::inclusive(b"file/b", b"file/c"),
            ScanDirection::Forward,
        )
        .unwrap();
    assert_eq!(
        bounded
            .iter()
            .map(|entry| entry.key.as_slice())
            .collect::<Vec<_>>(),
        vec![b"file/b".as_slice(), b"file/c".as_slice()]
    );
}

#[test]
fn keyset_cursor_is_exclusive_and_supports_both_directions() {
    let directory = tempfile::tempdir().unwrap();
    let database =
        SingleWriterTxDatabase::open(directory.path(), DatabaseConfig::default()).unwrap();
    let actions = [b"item/1", b"item/2", b"item/3", b"item/4"]
        .into_iter()
        .map(|key| WriteAction::Put {
            keyspace: KeyspaceName::CatalogRecords,
            key: key.to_vec(),
            value: key.to_vec(),
        })
        .collect::<Vec<_>>();
    database
        .write_batch(&actions, PersistMode::SyncData)
        .unwrap();
    let snapshot = database.read_snapshot();
    let page = snapshot
        .keyset(
            KeyspaceName::CatalogRecords,
            b"item/",
            Some(b"item/1"),
            ScanDirection::Forward,
            2,
        )
        .unwrap();
    assert_eq!(page[0].key, b"item/2");
    assert_eq!(page[1].key, b"item/3");
    let reverse = snapshot
        .keyset(
            KeyspaceName::CatalogRecords,
            b"item/",
            Some(b"item/4"),
            ScanDirection::Reverse,
            2,
        )
        .unwrap();
    assert_eq!(reverse[0].key, b"item/3");
    assert_eq!(reverse[1].key, b"item/2");
}
