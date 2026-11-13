use persistence_fjall::{
    clone_verified_generation_with_control, instrumentation, seal_source_generation_with_control,
    verify_source_generation_with_control, CatalogEntryType, CatalogRecordStore, DatabaseConfig,
    FileEntryRecord, FjallStorageError, KeyspaceName, PersistMode, SourceStore,
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

fn record(index: usize) -> FileEntryRecord {
    FileEntryRecord {
        id: format!("file-{index:05}"),
        parent_id: None,
        data_source_id: "source".into(),
        path: format!("/data/{index}.txt"),
        name: format!("{index}.txt"),
        entry_type: CatalogEntryType::File,
        size: Some(17),
        ext: Some("txt".into()),
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: Some(false),
        created: None,
        modified: None,
        accessed: None,
        changed: None,
        hash_sha256: None,
        partition_index: Some(0),
        unix_mode: None,
    }
}

fn source(count: usize) -> (tempfile::TempDir, SourceStore) {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/analysis/tests");
    std::fs::create_dir_all(&directory).unwrap();
    let root = tempfile::tempdir_in(directory).unwrap();
    let source = SourceStore::open(root.path().join("source"), DatabaseConfig::default()).unwrap();
    CatalogRecordStore::from_source_store(source.clone())
        .upsert_batch(
            &(0..count).map(record).collect::<Vec<_>>(),
            PersistMode::SyncAll,
        )
        .unwrap();
    let receipt = source
        .write_raw(
            KeyspaceName::Meta,
            b"fixture".to_vec(),
            count.to_be_bytes().to_vec(),
            PersistMode::SyncAll,
        )
        .unwrap();
    seal_source_generation_with_control(
        &source,
        "source",
        "original",
        &hex::encode(receipt.batch_digest),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    (root, source)
}

#[test]
fn verified_clone_hashes_during_one_copy_and_seals_without_another_scan() {
    let (root, source) = source(5_000);
    let cancel = AtomicBool::new(false);
    let expected =
        verify_source_generation_with_control(&source, "source", &cancel, |_, _| {}).unwrap();
    instrumentation::enable();
    instrumentation::reset();
    let mut copied = 0;
    let (target, verified) = clone_verified_generation_with_control(
        &source,
        "source",
        root.path().join("copy"),
        DatabaseConfig::default(),
        &cancel,
        |progress| copied = progress.records_copied,
    )
    .unwrap();
    assert!(copied > 5_000);
    assert_eq!(instrumentation::snapshot().scan_entries, copied);
    target
        .write_raw(
            KeyspaceName::Meta,
            b"derived".to_vec(),
            b"value".to_vec(),
            PersistMode::Buffer,
        )
        .unwrap();
    let reads = instrumentation::snapshot().scan_entries;
    verified.seal(&target, "analysis-copy", &cancel).unwrap();
    assert_eq!(instrumentation::snapshot().scan_entries, reads);
    let actual =
        verify_source_generation_with_control(&target, "source", &cancel, |_, _| {}).unwrap();
    assert_eq!(actual.immutable_data_digest, expected.immutable_data_digest);
}

#[test]
fn verified_receipt_rejects_another_target_and_changed_immutable_records() {
    let (root, source) = source(1);
    let cancel = AtomicBool::new(false);
    let (target, verified) = clone_verified_generation_with_control(
        &source,
        "source",
        root.path().join("copy"),
        DatabaseConfig::default(),
        &cancel,
        |_| {},
    )
    .unwrap();
    let unrelated =
        SourceStore::open(root.path().join("unrelated"), DatabaseConfig::default()).unwrap();
    assert!(matches!(
        verified.seal(&unrelated, "wrong-target", &cancel),
        Err(FjallStorageError::InvalidEnvelope(_))
    ));
    drop(target);
    let (target, verified) = clone_verified_generation_with_control(
        &source,
        "source",
        root.path().join("second-copy"),
        DatabaseConfig::default(),
        &cancel,
        |_| {},
    )
    .unwrap();
    let mut changed = record(0);
    changed.path = "/changed.txt".into();
    CatalogRecordStore::from_source_store(target.clone())
        .upsert(&changed, PersistMode::Buffer)
        .unwrap();
    assert!(matches!(
        verified.seal(&target, "changed-target", &cancel),
        Err(FjallStorageError::InvalidEnvelope(_))
    ));
    assert!(!target
        .database()
        .path()
        .join("source-generation.seal")
        .exists());
}

#[test]
fn corrupt_source_and_mid_copy_cancellation_never_get_a_sealed_target() {
    let (root, source) = source(5_000);
    let cancel = AtomicBool::new(false);
    let result = clone_verified_generation_with_control(
        &source,
        "source",
        root.path().join("cancelled"),
        DatabaseConfig::default(),
        &cancel,
        |progress| {
            if progress.records_copied > 0 {
                cancel.store(true, Ordering::Release);
            }
        },
    );
    assert!(matches!(result, Err(FjallStorageError::Cancelled)));
    assert!(!root
        .path()
        .join("cancelled/source-generation.seal")
        .exists());
    verify_source_generation_with_control(&source, "source", &AtomicBool::new(false), |_, _| {})
        .unwrap();
    let mut changed = record(0);
    changed.size = Some(99);
    CatalogRecordStore::from_source_store(source.clone())
        .upsert(&changed, PersistMode::SyncAll)
        .unwrap();
    assert!(matches!(
        clone_verified_generation_with_control(
            &source,
            "source",
            root.path().join("corrupt"),
            DatabaseConfig::default(),
            &AtomicBool::new(false),
            |_| {}
        ),
        Err(FjallStorageError::ManifestDigestMismatch { .. })
    ));
    assert!(!root.path().join("corrupt/source-generation.seal").exists());
}
