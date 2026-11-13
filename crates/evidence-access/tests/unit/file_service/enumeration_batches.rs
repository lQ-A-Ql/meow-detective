use super::*;
use evidence_core::LogicalFsReader;
use std::collections::BTreeSet;
use std::sync::atomic::AtomicUsize;

crate::native_source_test_support!();

fn add_files(root: &std::path::Path, prefix: &str, count: usize) -> BTreeSet<String> {
    std::fs::create_dir_all(root).unwrap();
    (0..count)
        .map(|index| {
            let name = format!("{prefix}-{index:05}.bin");
            std::fs::write(root.join(&name), [index as u8]).unwrap();
            name
        })
        .collect()
}

fn walk(reader: &LogicalFsReader) -> (Vec<FileEntry>, EnumerationStats) {
    let source = DataSourceId("batch-source".into());
    let mut walker = ChildBatchWalker::new(FileEntryId("root".into()));
    let mut rows = Vec::new();
    while let Some(batch) = walker.next_batch(reader, &source, None, None).unwrap() {
        assert!(batch.len() <= FILE_INSERT_BATCH_SIZE);
        rows.extend(batch);
    }
    (rows, walker.into_stats())
}

#[test]
fn real_directory_preserves_children_across_batch_boundaries() {
    for count in [499, 500, 501, 2000] {
        let root = tempfile::tempdir().unwrap();
        let expected = add_files(root.path(), "item", count);
        let reader = LogicalFsReader::open(root.path(), "batch-files").unwrap();
        let (rows, stats) = walk(&reader);
        let actual = rows
            .into_iter()
            .map(|row| row.path)
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
        assert_eq!(stats.file_count, count as u64);
        assert_eq!(stats.dir_count, 1);
        assert_eq!(stats.total_size, count as u64);
    }
}

#[test]
fn real_nested_directories_preserve_pending_children_and_directory_queue() {
    let root = tempfile::tempdir().unwrap();
    let mut expected = add_files(root.path(), "root", 503);
    for (relative, prefix, count) in [("a", "a", 503), ("b", "b", 1001), ("b/nested", "c", 503)] {
        let names = add_files(&root.path().join(relative), prefix, count);
        expected.extend(names.into_iter().map(|name| format!("{relative}/{name}")));
    }
    let reader = LogicalFsReader::open(root.path(), "nested-files").unwrap();
    let (rows, stats) = walk(&reader);
    let actual = rows
        .iter()
        .filter(|row| row.is_file())
        .map(|row| row.path.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert_eq!(stats.file_count, expected.len() as u64);
    assert_eq!(stats.total_size, expected.len() as u64);
    assert_eq!(stats.dir_count, 4);
    assert_eq!(rows.len(), expected.len() + 3);
}

#[test]
fn native_catalog_persists_every_file_after_multiple_batches() {
    let root = tempfile::tempdir().unwrap();
    let expected = add_files(root.path(), "stored", 2000);
    let reader = LogicalFsReader::open(root.path(), "native-files").unwrap();
    let fixture = test_support::NativeSourceFixture::new("batch-source", "batch-case");
    let stats = enumerate_filesystem(
        &fixture.connection,
        &DataSourceId(fixture.source_id.clone()),
        &reader,
    )
    .unwrap();
    assert_eq!(stats.file_count, 2000);
    let mut paths = BTreeSet::new();
    FileRepo::new(&fixture.connection)
        .visit_all(None, |entry| {
            if entry.is_file() {
                paths.insert(entry.path);
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(paths, expected);
    assert_eq!(
        FileRepo::new(&fixture.connection).count_all().unwrap(),
        2001
    );
}

#[test]
fn cancellation_does_not_consume_more_pending_children_or_change_counts() {
    let root = tempfile::tempdir().unwrap();
    add_files(root.path(), "cancel", 2000);
    let reader = LogicalFsReader::open(root.path(), "cancel-files").unwrap();
    let source = DataSourceId("batch-source".into());
    let cancel = AtomicBool::new(false);
    let mut walker = ChildBatchWalker::new(FileEntryId("root".into()));
    let first = walker
        .next_batch(&reader, &source, None, Some(&cancel))
        .unwrap()
        .unwrap();
    assert_eq!(first.len(), FILE_INSERT_BATCH_SIZE);
    cancel.store(true, Ordering::Release);
    assert!(walker
        .next_batch(&reader, &source, None, Some(&cancel))
        .is_err());
    let stats = walker.into_stats();
    assert_eq!(stats.file_count, FILE_INSERT_BATCH_SIZE as u64);
    assert_eq!(stats.total_size, FILE_INSERT_BATCH_SIZE as u64);
}

#[test]
fn cancelled_native_enumeration_does_not_publish_partial_batches() {
    let root = tempfile::tempdir().unwrap();
    add_files(root.path(), "transaction", 2000);
    let reader = LogicalFsReader::open(root.path(), "cancel-native").unwrap();
    let fixture = test_support::NativeSourceFixture::new("batch-source", "batch-case");
    let cancel = AtomicBool::new(false);
    let callbacks = AtomicUsize::new(0);
    let progress = |_: u32| {
        if callbacks.fetch_add(1, Ordering::AcqRel) >= 5 {
            cancel.store(true, Ordering::Release);
        }
    };
    let result = enumerate_filesystem_with_root_name_and_cancel(
        &fixture.connection,
        &DataSourceId(fixture.source_id.clone()),
        &reader,
        None,
        Some(&progress),
        Some(&cancel),
    );
    assert!(result.is_err());
    assert!(cancel.load(Ordering::Acquire));
    assert_eq!(FileRepo::new(&fixture.connection).count_all().unwrap(), 0);
}

#[test]
fn explicit_catalog_cancellation_preserves_committed_catalog() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/import");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let original = root.path().join("original");
    add_files(&original, "kept", 3);
    let fixture = test_support::NativeSourceFixture::new("batch-source", "batch-case");
    let source = DataSourceId(fixture.source_id.clone());
    enumerate_filesystem(
        &fixture.connection,
        &source,
        &LogicalFsReader::open(&original, "original").unwrap(),
    )
    .unwrap();
    let before = FileRepo::new(&fixture.connection).count_all().unwrap();
    let replacement = root.path().join("replacement");
    add_files(&replacement, "cancel", 2000);
    let cancel = AtomicBool::new(false);
    let callbacks = AtomicUsize::new(0);
    let progress = |_: u32| {
        if callbacks.fetch_add(1, Ordering::AcqRel) >= 5 {
            cancel.store(true, Ordering::Release);
        }
    };
    let catalog = crate::storage_runtime::open_source_read_write_at_path(
        fixture.source_dir.join("source.fjall"),
    )
    .unwrap();
    let error = enumerate_filesystem_with_root_name_and_cancel_with_catalog(
        &fixture.connection,
        &source,
        &LogicalFsReader::open(&replacement, "replacement").unwrap(),
        None,
        Some(&progress),
        Some(&cancel),
        &catalog,
    )
    .err()
    .expect("enumeration must be cancelled");
    assert!(error.to_string().contains("cancelled"));
    assert!(callbacks.load(Ordering::Acquire) >= 6);
    assert_eq!(
        FileRepo::new(&fixture.connection).count_all().unwrap(),
        before
    );
    let mut files = BTreeSet::new();
    FileRepo::new(&fixture.connection)
        .visit_all(None, |entry| {
            if entry.is_file() {
                files.insert(entry.name);
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(
        files,
        BTreeSet::from([
            "kept-00000.bin".into(),
            "kept-00001.bin".into(),
            "kept-00002.bin".into()
        ])
    );
}
