use persistence_fjall::{
    publish_source_generation, read_source_generation_manifest, replace_source_generation,
    scan_recovery_consistency, seal_source_generation_with_control,
    verify_source_generation_with_control, BatchReceiptState, DatabaseConfig, KeyspaceName,
    PersistMode, RecoveryScan, RecoveryState, SourceBuildLayout, SourceStore, WriteAction,
};

fn put(keyspace: KeyspaceName, key: &[u8], value: &[u8]) -> WriteAction {
    WriteAction::Put {
        keyspace,
        key: key.to_vec(),
        value: value.to_vec(),
    }
}

#[test]
fn worker_shutdown_releases_generation_for_immediate_publication() {
    let root = tempfile::tempdir().unwrap();
    for iteration in 0..32 {
        let source_id = format!("source-{iteration}");
        let layout =
            SourceBuildLayout::for_case_source(root.path(), &source_id, "attempt").unwrap();
        std::fs::create_dir_all(layout.build_path()).unwrap();
        let source = SourceStore::open(layout.build_path(), DatabaseConfig::default()).unwrap();
        source
            .database()
            .write_batch(
                &[put(KeyspaceName::CatalogRecords, b"catalog/1", b"file")],
                PersistMode::SyncAll,
            )
            .unwrap();
        seal_source_generation_with_control(
            &source,
            &source_id,
            "attempt",
            &"d".repeat(64),
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        drop(source);
        publish_source_generation(layout.build_path(), layout.published_path()).unwrap();
        let published =
            SourceStore::open(layout.published_path(), DatabaseConfig::default()).unwrap();
        verify_source_generation_with_control(
            &published,
            &source_id,
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
    }
}

#[test]
fn source_generation_manifest_survives_mutable_analysis_writes() {
    let directory = tempfile::tempdir().unwrap();
    let layout =
        SourceBuildLayout::for_case_source(directory.path(), "source-1", "attempt-1").unwrap();
    std::fs::create_dir_all(layout.build_path()).unwrap();
    let source = SourceStore::open(layout.build_path(), DatabaseConfig::default()).unwrap();
    source
        .database()
        .write_batch(
            &[
                put(KeyspaceName::CatalogRecords, b"catalog/1", b"file-one"),
                put(KeyspaceName::CatalogIndexes, b"path/one", b"catalog/1"),
                put(
                    KeyspaceName::PartitionRecords,
                    b"partition/1",
                    b"partition-one",
                ),
                put(KeyspaceName::PartitionIndexes, b"source/1", b"partition/1"),
                put(KeyspaceName::ParserRecords, b"parser/1", b"parser-before"),
            ],
            PersistMode::SyncAll,
        )
        .unwrap();
    let digest = seal_source_generation_with_control(
        &source,
        "source-1",
        "attempt-1",
        &"a".repeat(64),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();

    source
        .database()
        .write_batch(
            &[
                put(
                    KeyspaceName::ArtifactRecords,
                    b"artifact/1",
                    b"analysis-output",
                ),
                put(
                    KeyspaceName::TimelineRecords,
                    b"timeline/1",
                    b"analysis-event",
                ),
                put(KeyspaceName::Meta, b"analysis-state", b"complete"),
                put(KeyspaceName::ParserRecords, b"parser/1", b"parser-after"),
            ],
            PersistMode::SyncAll,
        )
        .unwrap();

    let manifest = verify_source_generation_with_control(
        &source,
        "source-1",
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(manifest.attempt_id, "attempt-1");
    assert_eq!(manifest.projection_batch_digest, "a".repeat(64));
    assert_eq!(
        persistence_fjall::source_generation_manifest_digest(layout.build_path()).unwrap(),
        digest
    );
}

#[test]
fn source_generation_manifest_rejects_catalog_changes() {
    let directory = tempfile::tempdir().unwrap();
    let layout =
        SourceBuildLayout::for_case_source(directory.path(), "source-1", "attempt-1").unwrap();
    std::fs::create_dir_all(layout.build_path()).unwrap();
    let source = SourceStore::open(layout.build_path(), DatabaseConfig::default()).unwrap();
    source
        .database()
        .write_batch(
            &[put(KeyspaceName::CatalogRecords, b"catalog/1", b"before")],
            PersistMode::SyncAll,
        )
        .unwrap();
    seal_source_generation_with_control(
        &source,
        "source-1",
        "attempt-1",
        &"b".repeat(64),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    source
        .database()
        .write_batch(
            &[put(KeyspaceName::CatalogRecords, b"catalog/1", b"after")],
            PersistMode::SyncAll,
        )
        .unwrap();

    assert!(matches!(
        verify_source_generation_with_control(
            &source,
            "source-1",
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {}
        ),
        Err(persistence_fjall::FjallStorageError::ManifestDigestMismatch { .. })
    ));
    let manifest = read_source_generation_manifest(&source, "source-1").unwrap();
    assert_eq!(manifest.attempt_id, "attempt-1");
    assert_eq!(manifest.projection_batch_digest, "b".repeat(64));
    replace_source_generation(&source, "source-1", "attempt-2", &"c".repeat(64)).unwrap();
    let manifest = verify_source_generation_with_control(
        &source,
        "source-1",
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(manifest.attempt_id, "attempt-2");
    assert_eq!(manifest.projection_batch_digest, "c".repeat(64));
}

#[test]
fn published_source_generation_reopens_and_verifies() {
    let directory = tempfile::tempdir().unwrap();
    let layout =
        SourceBuildLayout::for_case_source(directory.path(), "source-1", "attempt-1").unwrap();
    std::fs::create_dir_all(layout.build_path()).unwrap();
    let source = SourceStore::open(layout.build_path(), DatabaseConfig::default()).unwrap();
    source
        .database()
        .write_batch(
            &[put(KeyspaceName::CatalogRecords, b"catalog/1", b"file-one")],
            PersistMode::SyncAll,
        )
        .unwrap();
    let digest = seal_source_generation_with_control(
        &source,
        "source-1",
        "attempt-1",
        &"c".repeat(64),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    drop(source);

    let receipt = publish_source_generation(layout.build_path(), layout.published_path()).unwrap();
    assert_eq!(receipt.manifest_digest, digest);
    assert!(!receipt.reused);
    assert!(!layout.build_path().exists());

    let published = SourceStore::open(layout.published_path(), DatabaseConfig::default()).unwrap();
    let manifest = verify_source_generation_with_control(
        &published,
        "source-1",
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(manifest.attempt_id, "attempt-1");
    let receipt = published
        .database()
        .write_batch(
            &[put(
                KeyspaceName::ArtifactRecords,
                b"artifact/1",
                b"analysis",
            )],
            PersistMode::SyncAll,
        )
        .unwrap();
    assert_eq!(
        scan_recovery_consistency(RecoveryScan {
            build_path: &layout.build_path(),
            published_path: &layout.published_path(),
            batch_receipt: BatchReceiptState::Present {
                operation_count: receipt.operation_count,
            },
        })
        .unwrap()
        .state,
        RecoveryState::Published
    );
}
