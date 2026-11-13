use persistence_fjall::{
    close_shared_source_stores_under, open_shared_source_store, DatabaseConfig, FjallStorageError,
    KeyspaceName, PersistMode,
};
use std::path::{Path, PathBuf};

fn root() -> tempfile::TempDir {
    let scratch =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/analysis/tests");
    std::fs::create_dir_all(&scratch).unwrap();
    tempfile::tempdir_in(scratch).unwrap()
}

#[test]
fn source_close_observes_shared_handles_clones_and_snapshots_without_reading_sqlite() {
    let root = root();
    let source = root.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("source.db"), b"damaged sqlite header").unwrap();
    let path = source.join("source.fjall.build.analysis-current");
    let shared = open_shared_source_store(&path, DatabaseConfig::default()).unwrap();
    shared
        .write_raw(
            KeyspaceName::Meta,
            b"record".to_vec(),
            b"preserved".to_vec(),
            PersistMode::SyncAll,
        )
        .unwrap();
    let cloned = (*shared).clone();
    let snapshot = cloned.database().read_snapshot();
    assert!(close_shared_source_stores_under(&source).unwrap().is_none());
    drop(shared);
    assert!(close_shared_source_stores_under(&source).unwrap().is_none());
    drop(cloned);
    assert!(close_shared_source_stores_under(&source).unwrap().is_none());
    drop(snapshot);
    let closed = close_shared_source_stores_under(&source).unwrap().unwrap();
    assert!(matches!(
        open_shared_source_store(&path, DatabaseConfig::default()),
        Err(FjallStorageError::StorageLocked { .. })
    ));
    assert!(close_shared_source_stores_under(root.path())
        .unwrap()
        .is_none());
    drop(closed);
    let reopened = open_shared_source_store(&path, DatabaseConfig::default()).unwrap();
    assert_eq!(
        reopened
            .read_raw(
                &reopened.database().read_snapshot(),
                KeyspaceName::Meta,
                b"record"
            )
            .unwrap(),
        Some(b"preserved".to_vec())
    );
}

#[test]
fn absent_source_close_blocks_concurrent_creation_and_leaves_siblings_accessible() {
    let root = root();
    let source = root.path().join("missing_source");
    let closed = close_shared_source_stores_under(&source).unwrap().unwrap();
    assert!(!source.exists());
    let attempts = (0..4)
        .map(|index| {
            let path = source.join(format!("source.fjall.build.{index}"));
            std::thread::spawn(move || {
                assert!(matches!(
                    open_shared_source_store(&path, DatabaseConfig::default()),
                    Err(FjallStorageError::StorageLocked { .. })
                ));
                assert!(!path.exists());
            })
        })
        .collect::<Vec<_>>();
    for attempt in attempts {
        attempt.join().unwrap();
    }
    #[cfg(windows)]
    {
        let alias = root.path().join("MISSING_SOURCE/source.fjall");
        assert!(matches!(
            open_shared_source_store(alias, DatabaseConfig::default()),
            Err(FjallStorageError::StorageLocked { .. })
        ));
    }
    let sibling: PathBuf = root.path().join("missing_source_other/source.fjall");
    let sibling = open_shared_source_store(sibling, DatabaseConfig::default()).unwrap();
    drop(sibling);
    assert!(!source.exists());
    drop(closed);
    let reopened =
        open_shared_source_store(source.join("source.fjall"), DatabaseConfig::default()).unwrap();
    assert!(reopened.database().path().exists());
}
