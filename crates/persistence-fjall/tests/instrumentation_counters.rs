use persistence_fjall::{
    instrumentation::{self, StorageCounters},
    DatabaseConfig, KeyspaceName, PersistMode, ScanDirection, SingleWriterTxDatabase, WriteAction,
};

#[test]
fn counters_report_storage_work_only_while_enabled() {
    instrumentation::enable();
    // Reset before the open so this thread's counters start from zero and the
    // open below is the only database_open seen in this test.
    instrumentation::reset();
    let directory = tempfile::tempdir().unwrap();
    let database =
        SingleWriterTxDatabase::open(directory.path(), DatabaseConfig::default()).unwrap();
    let actions = vec![
        WriteAction::Put {
            keyspace: KeyspaceName::CatalogRecords,
            key: b"file/a".to_vec(),
            value: b"alpha".to_vec(),
        },
        WriteAction::Put {
            keyspace: KeyspaceName::CatalogRecords,
            key: b"file/b".to_vec(),
            value: b"beta".to_vec(),
        },
    ];
    database
        .write_batch(&actions, PersistMode::SyncAll)
        .unwrap();
    let snapshot = database.read_snapshot();
    assert_eq!(
        snapshot
            .get(KeyspaceName::CatalogRecords, b"file/a")
            .unwrap(),
        Some(b"alpha".to_vec())
    );
    let entries = snapshot
        .prefix(
            KeyspaceName::CatalogRecords,
            b"file/",
            ScanDirection::Forward,
        )
        .unwrap();
    assert_eq!(entries.len(), 2);

    let counters = instrumentation::snapshot();
    assert_eq!(counters.write_batches, 1);
    assert_eq!(counters.written_actions, 2);
    assert!(counters.bytes_written > 0);
    assert!(counters.sync_all_persists >= 1);
    assert_eq!(counters.point_reads, 1);
    assert!(counters.bytes_read > 0);
    assert_eq!(counters.scan_calls, 1);
    assert!(counters.scan_entries >= 2);
    assert!(counters.database_opens >= 1);

    instrumentation::disable();
    instrumentation::reset();
    assert_eq!(
        snapshot
            .get(KeyspaceName::CatalogRecords, b"file/b")
            .unwrap(),
        Some(b"beta".to_vec())
    );
    database
        .write_batch(&actions[..1], PersistMode::SyncAll)
        .unwrap();
    snapshot
        .prefix(
            KeyspaceName::CatalogRecords,
            b"file/",
            ScanDirection::Forward,
        )
        .unwrap();

    let disabled = instrumentation::snapshot();
    assert_eq!(disabled, StorageCounters::default());
    assert_eq!(disabled.point_reads, 0);
    assert_eq!(disabled.bytes_read, 0);
    assert_eq!(disabled.write_batches, 0);
    assert_eq!(disabled.written_actions, 0);
    assert_eq!(disabled.bytes_written, 0);
    assert_eq!(disabled.sync_all_persists, 0);
    assert_eq!(disabled.scan_calls, 0);
    assert_eq!(disabled.scan_entries, 0);
    assert_eq!(disabled.database_opens, 0);

    drop(snapshot);
    drop(database);
}
