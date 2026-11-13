use persistence_fjall::{
    instrumentation, DatabaseConfig, FjallStorageError, KeyspaceName, PersistMode, ScanBounds,
    SnapshotEntry, SourceStore, WriteAction,
};
use std::sync::atomic::{AtomicBool, Ordering};

fn source_store() -> (tempfile::TempDir, SourceStore) {
    let root = tempfile::tempdir().unwrap();
    let source = SourceStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let mut actions = (0..8_300)
        .map(|index| WriteAction::Put {
            keyspace: KeyspaceName::CatalogRecords,
            key: format!("catalog/{index:05}").into_bytes(),
            value: format!("value-{index}").into_bytes(),
        })
        .collect::<Vec<_>>();
    actions.push(WriteAction::Put {
        keyspace: KeyspaceName::Meta,
        key: b"original".to_vec(),
        value: b"unchanged".to_vec(),
    });
    source
        .database()
        .write_batch(&actions, PersistMode::SyncAll)
        .unwrap();
    (root, source)
}

fn entries(source: &SourceStore) -> Vec<(KeyspaceName, Vec<SnapshotEntry>)> {
    let snapshot = source.database().read_snapshot();
    let mut keyspaces = source.database().keyspace_names().collect::<Vec<_>>();
    keyspaces.sort_by_key(|keyspace| keyspace.as_str());
    keyspaces
        .into_iter()
        .map(|keyspace| {
            let mut entries = Vec::new();
            let mut cursor = None;
            loop {
                let page = snapshot
                    .range_keyset(keyspace, ScanBounds::unbounded(), cursor.as_deref(), 4_096)
                    .unwrap();
                if page.is_empty() {
                    break;
                }
                cursor = page.last().map(|entry| entry.key.clone());
                entries.extend(page);
            }
            (keyspace, entries)
        })
        .collect()
}

#[test]
fn controlled_clone_copies_4096_entry_pages_in_stable_order_with_one_sync() {
    let (_root, source) = source_store();
    let expected = entries(&source);
    let expected_count = expected
        .iter()
        .map(|(_, entries)| entries.len() as u64)
        .sum::<u64>();
    let expected_bytes = expected
        .iter()
        .flat_map(|(_, entries)| entries)
        .map(|entry| (entry.key.len() + entry.value.len()) as u64)
        .sum::<u64>();
    let output = tempfile::tempdir().unwrap();
    let mut updates = Vec::new();
    instrumentation::enable();
    instrumentation::reset();
    let cloned = source
        .clone_to_with_control(
            output.path().join("clone"),
            DatabaseConfig::default(),
            &AtomicBool::new(false),
            |progress| updates.push(progress),
        )
        .unwrap();
    let counters = instrumentation::snapshot();
    instrumentation::disable();
    assert_eq!(counters.sync_all_persists, 1);
    assert_eq!(counters.bytes_written, expected_bytes);
    assert!(counters.scan_entries >= expected_count);
    assert!(counters.bytes_read >= expected_bytes);
    let final_progress = updates.last().unwrap();
    assert!(final_progress.durable);
    assert_eq!(final_progress.records_copied, expected_count);
    assert_eq!(final_progress.bytes_copied, expected_bytes);
    assert_eq!(final_progress.completed_keyspaces, expected.len());
    let completed = updates
        .windows(2)
        .filter(|pair| pair[1].completed_keyspaces > pair[0].completed_keyspaces)
        .map(|pair| pair[1].keyspace.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        completed,
        expected
            .iter()
            .map(|(keyspace, _)| *keyspace)
            .collect::<Vec<_>>()
    );
    let catalog_batches = updates
        .windows(2)
        .filter_map(|pair| {
            let count = pair[1].records_copied - pair[0].records_copied;
            (pair[1].keyspace == Some(KeyspaceName::CatalogRecords) && count > 0).then_some(count)
        })
        .collect::<Vec<_>>();
    assert_eq!(catalog_batches, vec![4_096, 4_096, 108]);
    let snapshot = cloned.database().read_snapshot();
    for (keyspace, entries) in &expected {
        for entry in entries {
            assert_eq!(
                snapshot.get(*keyspace, &entry.key).unwrap(),
                Some(entry.value.clone())
            );
        }
    }
    assert_eq!(entries(&source), expected);
}

#[test]
fn a_preexisting_cancellation_does_not_create_a_destination() {
    let (_root, source) = source_store();
    let expected = entries(&source);
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("cancelled");
    let result = source.clone_to_with_control(
        &path,
        DatabaseConfig::default(),
        &AtomicBool::new(true),
        |_| panic!("progress after cancellation"),
    );
    assert!(matches!(result, Err(FjallStorageError::Cancelled)));
    assert!(!path.exists());
    assert_eq!(entries(&source), expected);
}

#[test]
fn cancellation_after_a_page_preserves_the_source_and_never_seals_the_clone() {
    let (_root, source) = source_store();
    let expected = entries(&source);
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("cancelled");
    let cancel = AtomicBool::new(false);
    let mut copied = 0;
    let result =
        source.clone_to_with_control(&path, DatabaseConfig::default(), &cancel, |progress| {
            copied = progress.records_copied;
            assert!(!progress.durable);
            if progress.keyspace == Some(KeyspaceName::CatalogRecords) && copied >= 4_096 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
    assert!(matches!(result, Err(FjallStorageError::Cancelled)));
    assert!((4_096..8_300).contains(&copied));
    assert!(!path
        .join(persistence_fjall::SOURCE_GENERATION_SEAL_FILE)
        .exists());
    assert_eq!(entries(&source), expected);
}

#[test]
fn a_destination_io_error_preserves_the_source() {
    let (_root, source) = source_store();
    let expected = entries(&source);
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("not-a-directory");
    std::fs::write(&path, b"existing file").unwrap();
    let result = source.clone_to_with_control(
        &path,
        DatabaseConfig::default(),
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"existing file");
    assert_eq!(entries(&source), expected);
}

#[test]
fn large_values_use_a_bounded_payload_batch() {
    let root = tempfile::tempdir().unwrap();
    let source = SourceStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let actions = (0..5)
        .map(|index| WriteAction::Put {
            keyspace: KeyspaceName::CatalogRecords,
            key: vec![index],
            value: vec![index; 3 * 1024 * 1024],
        })
        .collect::<Vec<_>>();
    source
        .database()
        .write_batch(&actions, PersistMode::SyncAll)
        .unwrap();
    let target = tempfile::tempdir().unwrap();
    let mut updates = Vec::new();
    let cloned = source
        .clone_to_with_control(
            target.path().join("clone"),
            DatabaseConfig::default(),
            &AtomicBool::new(false),
            |progress| updates.push(progress),
        )
        .unwrap();
    let batches = updates
        .windows(2)
        .filter_map(|pair| {
            let records = pair[1].records_copied - pair[0].records_copied;
            (pair[1].keyspace == Some(KeyspaceName::CatalogRecords) && records > 0)
                .then_some((records, pair[1].bytes_copied - pair[0].bytes_copied))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        batches.iter().map(|batch| batch.0).collect::<Vec<_>>(),
        vec![2, 2, 1]
    );
    assert!(batches.iter().all(|batch| batch.1 <= 8 * 1024 * 1024));
    let snapshot = cloned.database().read_snapshot();
    assert_eq!(
        snapshot.get(KeyspaceName::CatalogRecords, &[4]).unwrap(),
        Some(vec![4; 3 * 1024 * 1024])
    );
}
