use persistence_fjall::{
    DatabaseConfig, FjallStorageError, KeyspaceName, SnapshotEntry, SourceDatabase, WriteAction,
    WriteSessionConfig,
};
use std::sync::atomic::AtomicBool;

#[test]
fn eight_workers_execute_overlapping_native_ingestion_calls_and_reopen() {
    assert_eq!(WriteSessionConfig::default().workers, 8);
    verify_native_workers(8);
}

#[test]
fn four_workers_execute_overlapping_native_ingestion_calls_and_reopen() {
    verify_native_workers(4);
}

fn verify_native_workers(workers: usize) {
    let root = tempfile::tempdir().unwrap();
    let database = SourceDatabase::open(root.path(), DatabaseConfig::default()).unwrap();
    let session = database
        .begin_write_session(WriteSessionConfig {
            workers,
            ..Default::default()
        })
        .unwrap();
    let mut batch = 0;
    session
        .run(
            &AtomicBool::new(false),
            || {
                if batch == 8 {
                    return Ok(None);
                }
                let start = batch * 8192;
                batch += 1;
                Ok(Some(
                    (start..start + 8192u64)
                        .map(|record| SnapshotEntry {
                            key: record.to_be_bytes().to_vec(),
                            value: record.to_be_bytes().repeat(64),
                        })
                        .collect::<Vec<_>>(),
                ))
            },
            |record| u64::from_be_bytes(record.key.as_slice().try_into().unwrap()),
            |_, records| {
                Ok(records
                    .into_iter()
                    .map(|record| WriteAction::Put {
                        keyspace: KeyspaceName::TimelineRecords,
                        key: record.key,
                        value: record.value,
                    })
                    .collect())
            },
        )
        .unwrap();
    let report = session.finish().unwrap();
    assert_eq!(report.configured_workers, workers);
    assert_eq!(report.workers_used as usize, workers);
    assert!(report.worker_native_calls[..workers]
        .iter()
        .all(|calls| *calls > 0));
    assert_eq!(
        report.worker_native_calls.iter().sum::<u64>(),
        report.native_calls
    );
    assert!(report.peak_native_calls >= 4, "{report:?}");
    assert_eq!(report.actions, 65536);
    drop(database);
    let database = SourceDatabase::open(root.path(), DatabaseConfig::default()).unwrap();
    let snapshot = database.read_handle().read_snapshot();
    for record in [0u64, 8191, 32768, 65535] {
        assert_eq!(
            snapshot
                .get(KeyspaceName::TimelineRecords, &record.to_be_bytes())
                .unwrap(),
            Some(record.to_be_bytes().repeat(64))
        );
    }
}

#[test]
fn failed_native_session_prevents_following_writes_and_publication() {
    let root = tempfile::tempdir().unwrap();
    let database = SourceDatabase::open(root.path(), DatabaseConfig::default()).unwrap();
    let session = database
        .begin_write_session(WriteSessionConfig::default())
        .unwrap();
    let result = session.run::<SnapshotEntry>(
        &AtomicBool::new(true),
        || {
            Ok(Some(vec![SnapshotEntry {
                key: vec![1],
                value: vec![1],
            }]))
        },
        |_| 0,
        |_, _| Ok(Vec::new()),
    );
    assert!(result.is_err());
    assert!(matches!(
        session.finish(),
        Err(FjallStorageError::WriteSessionFailed)
    ));
    assert!(matches!(
        database.write_batch(&[], persistence_fjall::PersistMode::Buffer),
        Err(FjallStorageError::WriteSessionFailed)
    ));
    assert!(matches!(
        persistence_fjall::seal_build(root.path()),
        Err(FjallStorageError::WriteSessionFailed)
    ));
    drop(database);
    assert!(matches!(
        SourceDatabase::open(root.path(), DatabaseConfig::default()),
        Err(FjallStorageError::WriteSessionFailed)
    ));
}

#[test]
fn process_exit_keeps_the_published_database_and_rejects_the_incomplete_generation() {
    const CHILD_ROOT: &str = "FORENSICS_NATIVE_CRASH_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let database = SourceDatabase::open(
            std::path::PathBuf::from(root).join("building"),
            DatabaseConfig::default(),
        )
        .unwrap();
        let session = database
            .begin_write_session(WriteSessionConfig::default())
            .unwrap();
        let mut input = Some(
            (0..8192u64)
                .map(|id| SnapshotEntry {
                    key: id.to_be_bytes().to_vec(),
                    value: id.to_be_bytes().to_vec(),
                })
                .collect(),
        );
        session
            .run(
                &AtomicBool::new(false),
                || Ok(input.take()),
                |entry| u64::from_be_bytes(entry.key.as_slice().try_into().unwrap()),
                |_, entries| {
                    Ok(entries
                        .into_iter()
                        .map(|entry| WriteAction::Put {
                            keyspace: KeyspaceName::TimelineRecords,
                            key: entry.key,
                            value: entry.value,
                        })
                        .collect())
                },
            )
            .unwrap();
        std::process::exit(77);
    }
    let root = tempfile::tempdir().unwrap();
    let published_path = root.path().join("published");
    let database = SourceDatabase::open(&published_path, DatabaseConfig::default()).unwrap();
    database
        .write_batch(
            &[WriteAction::Put {
                keyspace: KeyspaceName::Meta,
                key: b"published".to_vec(),
                value: b"stable".to_vec(),
            }],
            persistence_fjall::PersistMode::SyncAll,
        )
        .unwrap();
    drop(database);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "process_exit_keeps_the_published_database_and_rejects_the_incomplete_generation",
        ])
        .env(CHILD_ROOT, root.path())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(77));
    assert!(matches!(
        SourceDatabase::open(root.path().join("building"), DatabaseConfig::default()),
        Err(FjallStorageError::WriteSessionFailed)
    ));
    assert!(matches!(
        persistence_fjall::seal_build(root.path().join("building")),
        Err(FjallStorageError::WriteSessionFailed)
    ));
    let published = SourceDatabase::open(published_path, DatabaseConfig::default()).unwrap();
    assert_eq!(
        published
            .read_snapshot()
            .get(KeyspaceName::Meta, b"published")
            .unwrap(),
        Some(b"stable".to_vec())
    );
}
