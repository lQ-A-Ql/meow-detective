use persistence_fjall::{
    read_source_generation_manifest, seal_source_generation_with_control,
    verify_source_generation_with_control, ArtifactRecord, ArtifactRecordStore, DatabaseConfig,
    FjallStorageError, KeyspaceName, PersistMode, SourceStore, WriteAction,
    SOURCE_GENERATION_MANIFEST_FILE, SOURCE_GENERATION_SEAL_FILE,
};
use std::sync::atomic::{AtomicBool, Ordering};

fn source() -> (tempfile::TempDir, SourceStore) {
    let root = tempfile::tempdir().unwrap();
    let source = SourceStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let actions = (0..8_300)
        .map(|index| WriteAction::Put {
            keyspace: KeyspaceName::CatalogRecords,
            key: format!("catalog-{index:05}").into_bytes(),
            value: vec![index as u8; 64],
        })
        .collect::<Vec<_>>();
    source
        .database()
        .write_batch(&actions, PersistMode::SyncAll)
        .unwrap();
    (root, source)
}

#[test]
fn controlled_seal_reports_bounded_pages_and_verifies() {
    let (_root, source) = source();
    let mut reports = Vec::new();
    seal_source_generation_with_control(
        &source,
        "source",
        "attempt",
        &"a".repeat(64),
        &AtomicBool::new(false),
        |records, bytes| reports.push((records, bytes)),
    )
    .unwrap();
    assert_eq!(
        reports.iter().map(|report| report.0).collect::<Vec<_>>(),
        vec![4_096, 8_192, 8_300]
    );
    assert!(reports.windows(2).all(|pair| pair[0].1 < pair[1].1));
    let mut verified = 0;
    let manifest = verify_source_generation_with_control(
        &source,
        "source",
        &AtomicBool::new(false),
        |records, _| verified = records,
    )
    .unwrap();
    assert_eq!(verified, 8_300);
    assert_eq!(manifest.attempt_id, "attempt");
}

#[test]
fn cancelling_seal_before_completion_writes_no_markers() {
    let (root, source) = source();
    let cancel = AtomicBool::new(false);
    let result = seal_source_generation_with_control(
        &source,
        "source",
        "cancelled",
        &"a".repeat(64),
        &cancel,
        |records, _| {
            if records >= 4_096 {
                cancel.store(true, Ordering::Relaxed)
            }
        },
    );
    assert!(matches!(result, Err(FjallStorageError::Cancelled)));
    assert!(!root.path().join(SOURCE_GENERATION_MANIFEST_FILE).exists());
    assert!(!root.path().join(SOURCE_GENERATION_SEAL_FILE).exists());
    assert_eq!(
        source
            .database()
            .read_snapshot()
            .get(KeyspaceName::CatalogRecords, b"catalog-08299")
            .unwrap(),
        Some(vec![8_299u16 as u8; 64])
    );
}

#[test]
fn cancelling_verification_preserves_existing_generation_markers() {
    let (root, source) = source();
    seal_source_generation_with_control(
        &source,
        "source",
        "attempt",
        &"a".repeat(64),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let manifest_path = root.path().join(SOURCE_GENERATION_MANIFEST_FILE);
    let seal_path = root.path().join(SOURCE_GENERATION_SEAL_FILE);
    let before = (
        std::fs::read(&manifest_path).unwrap(),
        std::fs::read(&seal_path).unwrap(),
    );
    let cancel = AtomicBool::new(false);
    let result = verify_source_generation_with_control(&source, "source", &cancel, |records, _| {
        if records >= 4_096 {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(matches!(result, Err(FjallStorageError::Cancelled)));
    assert_eq!(std::fs::read(&manifest_path).unwrap(), before.0);
    assert_eq!(std::fs::read(&seal_path).unwrap(), before.1);
    assert_eq!(
        read_source_generation_manifest(&source, "source")
            .unwrap()
            .attempt_id,
        "attempt"
    );
}

#[test]
fn cancelled_metric_rebuild_keeps_counts_unpublished_until_retry() {
    let root = tempfile::tempdir().unwrap();
    let source = SourceStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let artifacts = ArtifactRecordStore::from_source_store(source);
    let records = (0..1_200)
        .map(|index| ArtifactRecord {
            id: format!("record-{index:05}"),
            data_source_id: "source".into(),
            artifact_type: format!("type-{}", index % 3),
            source_object_id: None,
            content_sha256: None,
            attrs: serde_json::json!({}),
        })
        .collect::<Vec<_>>();
    artifacts
        .upsert_batch_independent(&records, PersistMode::SyncAll)
        .unwrap();
    let cancel = AtomicBool::new(false);
    let before = artifacts
        .cursor_revision(&artifacts.read_snapshot())
        .unwrap();
    let result = artifacts.rebuild_metrics_for_source_with_control(
        "source",
        PersistMode::Buffer,
        &cancel,
        |count| {
            if count >= 512 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    assert!(matches!(result, Err(FjallStorageError::Cancelled)));
    let snapshot = artifacts.read_snapshot();
    assert!(!artifacts.type_counts_ready(&snapshot, "source").unwrap());
    assert_eq!(artifacts.cursor_revision(&snapshot).unwrap(), before);
    let mut processed = 0;
    artifacts
        .rebuild_metrics_for_source_with_control(
            "source",
            PersistMode::SyncAll,
            &AtomicBool::new(false),
            |count| processed = count,
        )
        .unwrap();
    assert_eq!(processed, 1_200);
    assert_eq!(
        artifacts
            .type_counts(&artifacts.read_snapshot(), "source")
            .unwrap(),
        vec![
            ("type-0".into(), 400),
            ("type-1".into(), 400),
            ("type-2".into(), 400)
        ]
    );
}
