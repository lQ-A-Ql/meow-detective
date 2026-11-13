use crate::{ParserFieldQuery, ParserRecord, ParserRecordStore, PersistMode, ScanDirection};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

fn fixture() -> (tempfile::TempDir, ParserRecordStore) {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/android");
    std::fs::create_dir_all(&scratch).unwrap();
    let root = tempfile::tempdir_in(scratch).unwrap();
    let store = ParserRecordStore::open(root.path(), crate::DatabaseConfig::default()).unwrap();
    (root, store)
}

fn record(source: &str, family: &str, id: &str, kind: &str) -> ParserRecord {
    let payload = serde_json::json!({"name": id});
    ParserRecord {
        id: id.into(),
        data_source_id: source.into(),
        family: family.into(),
        kind: kind.into(),
        status: "ready".into(),
        summary_sha256: hex::encode(crate::digest(payload.to_string().as_bytes())),
        payload,
    }
}

fn family(store: &ParserRecordStore, source: &str, name: &str) -> Vec<ParserRecord> {
    store
        .list_by_family(
            &store.read_snapshot(),
            source,
            name,
            None,
            ScanDirection::Forward,
            64,
        )
        .unwrap()
        .records
}

#[test]
fn replacement_removes_old_records_and_indexes_and_preserves_other_owners() {
    let (_root, store) = fixture();
    let old = record("source", "android", "old", "removed-kind");
    let other_family = record("source", "other", "same-kind", "removed-kind");
    let other_source = record("other-source", "android", "foreign", "removed-kind");
    store
        .upsert_batch(
            &[old.clone(), other_family.clone(), other_source.clone()],
            PersistMode::SyncAll,
        )
        .unwrap();
    let next = record("source", "android", "new", "new-kind");
    let receipt = store
        .replace_family_with_control(
            "source",
            "android",
            &[next.clone()],
            PersistMode::SyncAll,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert!(receipt.operation_count > 0);
    assert_eq!(family(&store, "source", "android"), [next]);
    assert_eq!(family(&store, "source", "other"), [other_family]);
    assert_eq!(family(&store, "other-source", "android"), [other_source]);
    assert!(store
        .get(&store.read_snapshot(), "source", "old")
        .unwrap()
        .is_none());
    let fields = store
        .list_by_field(
            &store.read_snapshot(),
            ParserFieldQuery {
                source_id: "source",
                kind: "removed-kind",
                field: "name",
                value: "old",
            },
            None,
            ScanDirection::Forward,
            64,
        )
        .unwrap();
    assert!(fields.records.is_empty());
    assert!(store
        .replace_family_with_control(
            "source",
            "android",
            &[record("source", "android", "same-kind", "new-kind")],
            PersistMode::SyncAll,
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err());
    assert_eq!(family(&store, "source", "other")[0].id, "same-kind");
    store
        .replace_family_with_control(
            "source",
            "android",
            &[],
            PersistMode::SyncAll,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert!(family(&store, "source", "android").is_empty());
    assert!(store
        .list_by_kind(
            &store.read_snapshot(),
            "source",
            "new-kind",
            None,
            ScanDirection::Forward,
            64
        )
        .unwrap()
        .records
        .is_empty());
}

#[test]
fn cancelled_or_invalid_replacement_preserves_committed_records_and_indexes() {
    let (_root, store) = fixture();
    let old = record("source", "android", "old", "kind");
    store.upsert(&old, PersistMode::SyncAll).unwrap();
    let cancel = AtomicBool::new(false);
    let result = store.replace_family_with_control(
        "source",
        "android",
        &[record("source", "android", "new", "kind")],
        PersistMode::SyncAll,
        &cancel,
        |_| cancel.store(true, Ordering::Release),
    );
    assert!(matches!(result, Err(crate::FjallStorageError::Cancelled)));
    assert_eq!(family(&store, "source", "android"), [old.clone()]);
    cancel.store(false, Ordering::Release);
    assert!(store
        .replace_family_with_control(
            "source",
            "android",
            &[record("source", "other", "new", "kind")],
            PersistMode::SyncAll,
            &cancel,
            |_| {}
        )
        .is_err());
    let repeated = record("source", "android", "new", "kind");
    assert!(store
        .replace_family_with_control(
            "source",
            "android",
            &[repeated.clone(), repeated],
            PersistMode::SyncAll,
            &cancel,
            |_| {}
        )
        .is_err());
    assert_eq!(family(&store, "source", "android"), [old.clone()]);
    assert_eq!(
        store
            .list_by_kind(
                &store.read_snapshot(),
                "source",
                "kind",
                None,
                ScanDirection::Forward,
                64
            )
            .unwrap()
            .records,
        [old]
    );
}

#[test]
fn concurrent_upsert_observes_replacement_before_planning_index_deletes() {
    let (_root, store) = fixture();
    store
        .upsert(
            &record("source", "android", "shared", "before"),
            PersistMode::SyncAll,
        )
        .unwrap();
    let store = Arc::new(store);
    let (inside_tx, started) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let replacing = Arc::clone(&store);
    let replacement = std::thread::spawn(move || {
        let mut stopped = false;
        replacing
            .replace_family_with_control(
                "source",
                "android",
                &[record("source", "android", "shared", "replacement")],
                PersistMode::SyncAll,
                &AtomicBool::new(false),
                |_| {
                    if !stopped {
                        stopped = true;
                        inside_tx.send(()).unwrap();
                        release.recv().unwrap();
                    }
                },
            )
            .unwrap();
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let old = family(&store, "source", "android");
    assert_eq!(old[0].kind, "before");
    let (entered_tx, entered) = mpsc::channel();
    let (done_tx, done) = mpsc::channel();
    let updating = Arc::clone(&store);
    let update = std::thread::spawn(move || {
        entered_tx.send(()).unwrap();
        updating
            .upsert(
                &record("source", "android", "shared", "after"),
                PersistMode::SyncAll,
            )
            .unwrap();
        done_tx.send(()).unwrap();
    });
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(done.recv_timeout(Duration::from_millis(50)).is_err());
    release_tx.send(()).unwrap();
    replacement.join().unwrap();
    update.join().unwrap();
    assert_eq!(family(&store, "source", "android")[0].kind, "after");
    for kind in ["before", "replacement"] {
        assert!(store
            .list_by_kind(
                &store.read_snapshot(),
                "source",
                kind,
                None,
                ScanDirection::Forward,
                64
            )
            .unwrap()
            .records
            .is_empty());
    }
}

#[test]
fn corrupted_family_index_aborts_the_entire_replacement() {
    let (_root, store) = fixture();
    let old = record("source", "android", "old", "kind");
    store.upsert(&old, PersistMode::SyncAll).unwrap();
    let key = crate::encode_key(
        "parser.family",
        &[
            crate::KeyPart::Text("source".into()),
            crate::KeyPart::Text("android".into()),
            crate::KeyPart::Text("wrong-id".into()),
        ],
    )
    .unwrap();
    let value = crate::encode_key(
        "parser.record",
        &[
            crate::KeyPart::Text("source".into()),
            crate::KeyPart::Text("old".into()),
        ],
    )
    .unwrap();
    store
        .source_store
        .database()
        .write_batch(
            &[crate::WriteAction::Put {
                keyspace: crate::KeyspaceName::ParserIndexes,
                key: key.clone(),
                value: value.clone(),
            }],
            PersistMode::SyncAll,
        )
        .unwrap();
    let result = store.replace_family_with_control(
        "source",
        "android",
        &[record("source", "android", "new", "kind")],
        PersistMode::SyncAll,
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(matches!(
        result,
        Err(crate::FjallStorageError::ParserIndexCorrupt)
    ));
    let snapshot = store.read_snapshot();
    assert_eq!(store.get(&snapshot, "source", "old").unwrap(), Some(old));
    assert!(store.get(&snapshot, "source", "new").unwrap().is_none());
    assert_eq!(
        snapshot
            .get(crate::KeyspaceName::ParserIndexes, &key)
            .unwrap(),
        Some(value)
    );
}
