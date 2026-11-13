use super::*;
use crate::{
    seal_source_generation_with_control, verify_source_generation_with_control, SourceStore,
};
use std::{sync::mpsc, time::Duration};

#[test]
fn sealing_and_concurrent_immutable_write_share_one_exclusive_boundary() {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/analysis/tests");
    std::fs::create_dir_all(&directory).unwrap();
    let root = tempfile::tempdir_in(directory).unwrap();
    let source = SourceStore::open(root.path().join("source"), DatabaseConfig::default()).unwrap();
    let receipt = source
        .write_raw(
            KeyspaceName::Meta,
            b"fixture".to_vec(),
            b"concurrent-seal".to_vec(),
            PersistMode::SyncAll,
        )
        .unwrap();
    let (begin_tx, begin_rx) = mpsc::channel();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let writer = source.clone();
        let thread = scope.spawn(move || {
            begin_rx.recv().unwrap();
            attempt_tx.send(()).unwrap();
            let result = writer.write_raw(
                KeyspaceName::CatalogIndexes,
                b"concurrent-index".to_vec(),
                b"concurrent-value".to_vec(),
                PersistMode::SyncAll,
            );
            finished_tx.send(()).unwrap();
            result
        });
        source
            .database()
            .seal_immutable(0, || {
                begin_tx.send(()).unwrap();
                attempt_rx.recv().unwrap();
                assert!(matches!(
                    finished_rx.recv_timeout(Duration::from_millis(20)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ));
                seal_source_generation_with_control(
                    &source,
                    "source",
                    "sealed",
                    &hex::encode(receipt.batch_digest),
                    &AtomicBool::new(false),
                    |_, _| {},
                )
            })
            .unwrap();
        assert!(matches!(
            thread.join().unwrap(),
            Err(FjallStorageError::InvalidEnvelope(_))
        ));
    });
    verify_source_generation_with_control(&source, "source", &AtomicBool::new(false), |_, _| {})
        .unwrap();
    assert!(source
        .read_raw(
            &source.database().read_snapshot(),
            KeyspaceName::CatalogIndexes,
            b"concurrent-index"
        )
        .unwrap()
        .is_none());
}
