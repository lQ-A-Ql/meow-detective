use persistence_fjall::{
    decode_key, encode_key, DatabaseConfig, FjallStorageError, KeyPart, KeyspaceName, PersistMode,
    SingleWriterTxDatabase, SourceStore, VersionedEnvelope, WriteAction,
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
