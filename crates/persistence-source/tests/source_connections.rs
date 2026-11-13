use domain::DataSourceId;
use persistence_source::control::connections::try_begin_generation_gc;
use persistence_source::control::opening::open_source_database_read_only;
use persistence_source::control::{bound_source_generation_root, open_source_db, source_db_path};
use persistence_source::detail::bind_generation;
use persistence_source::fjall::{KeyspaceName, PersistMode, VersionedEnvelope};
use persistence_source::runtime::{open_source_read_only, ReadWriteStorageRuntime};
use std::path::Path;

fn seal(path: &Path, id: &DataSourceId) {
    let runtime = ReadWriteStorageRuntime::open(path).unwrap();
    let source = runtime.source();
    let receipt = source
        .write_envelope(
            KeyspaceName::Meta,
            b"connection-test".to_vec(),
            &VersionedEnvelope::new(
                "test",
                1,
                "test",
                &id.0,
                1,
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .as_bytes()
                    .to_vec(),
            ),
            PersistMode::SyncAll,
        )
        .unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        source.source_store(),
        &id.0,
        "connection-test",
        &hex::encode(receipt.batch_digest),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
}

#[test]
fn a_source_connection_pins_before_any_generation_lookup() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("connection-pin".into());
    let control = open_source_db(root.path(), &id).unwrap();
    let path = bound_source_generation_root(&control, &id).unwrap();
    seal(&path, &id);
    drop(control);
    let source_directory = path.parent().unwrap();
    let untouched = open_source_database_read_only(&source_db_path(root.path(), &id)).unwrap();
    assert!(try_begin_generation_gc(source_directory).unwrap().is_none());
    drop(untouched);
    assert!(try_begin_generation_gc(source_directory).unwrap().is_some());
}

#[test]
fn old_sqlite_transactions_and_fjall_handles_both_block_retirement() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("old-transaction".into());
    let writer = open_source_db(root.path(), &id).unwrap();
    let original = bound_source_generation_root(&writer, &id).unwrap();
    let next = original
        .parent()
        .unwrap()
        .join("source.fjall.build.analysis-next");
    seal(&original, &id);
    seal(&next, &id);
    let pinned = open_source_read_only(root.path(), &id.0).unwrap();
    let mut old = open_source_database_read_only(&source_db_path(root.path(), &id)).unwrap();
    let snapshot = old.transaction().unwrap();
    let before = bound_source_generation_root(&snapshot, &id).unwrap();
    bind_generation(&writer, "source.fjall.build.analysis-next").unwrap();
    assert_eq!(
        bound_source_generation_root(&snapshot, &id).unwrap(),
        before
    );
    let directory = original.parent().unwrap();
    assert!(try_begin_generation_gc(directory).unwrap().is_none());
    drop(snapshot);
    drop(old);
    assert!(try_begin_generation_gc(directory).unwrap().is_none());
    drop(writer);
    let exclusive = try_begin_generation_gc(directory).unwrap().unwrap();
    assert_eq!(
        exclusive.current_generation_root(&id).unwrap(),
        std::fs::canonicalize(&next).unwrap()
    );
    assert!(exclusive.try_close_retired_generation(&next).is_err());
    assert!(exclusive
        .try_close_retired_generation(&original)
        .unwrap()
        .is_none());
    drop(pinned);
    assert!(exclusive
        .try_close_retired_generation(&original)
        .unwrap()
        .is_some());
    drop(exclusive);
    assert!(open_source_database_read_only(&source_db_path(root.path(), &id)).is_ok());
}

#[test]
fn failed_source_open_releases_its_pin() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("source");
    std::fs::create_dir(&directory).unwrap();
    assert!(open_source_database_read_only(&directory.join("missing.db")).is_err());
    assert!(try_begin_generation_gc(&directory).unwrap().is_some());
}

#[test]
fn a_direct_detail_store_reference_blocks_retirement_after_sql_closes() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("detail-reference".into());
    let control = open_source_db(root.path(), &id).unwrap();
    let original = bound_source_generation_root(&control, &id).unwrap();
    let next = original
        .parent()
        .unwrap()
        .join("source.fjall.build.analysis-next");
    seal(&original, &id);
    seal(&next, &id);
    let direct = persistence_source::detail::require_source_store(&control).unwrap();
    bind_generation(&control, "source.fjall.build.analysis-next").unwrap();
    drop(control);
    let exclusive = try_begin_generation_gc(original.parent().unwrap())
        .unwrap()
        .unwrap();
    assert!(exclusive
        .try_close_retired_generation(&original)
        .unwrap()
        .is_none());
    drop(direct);
    assert!(exclusive
        .try_close_retired_generation(&original)
        .unwrap()
        .is_some());
}

#[test]
fn a_new_reader_waits_for_gc_and_reads_the_retained_generation() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("concurrent-open".into());
    let control = open_source_db(root.path(), &id).unwrap();
    let original = bound_source_generation_root(&control, &id).unwrap();
    let next = original
        .parent()
        .unwrap()
        .join("source.fjall.build.analysis-next");
    seal(&original, &id);
    seal(&next, &id);
    bind_generation(&control, "source.fjall.build.analysis-next").unwrap();
    drop(control);
    let exclusive = try_begin_generation_gc(original.parent().unwrap())
        .unwrap()
        .unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let reader_barrier = std::sync::Arc::clone(&barrier);
    let case_root = root.path().to_path_buf();
    let reader = std::thread::spawn(move || {
        reader_barrier.wait();
        let result = open_source_read_only(&case_root, &id.0).map(|source| {
            source
                .read_envelope(&source.snapshot(), KeyspaceName::Meta, b"connection-test")
                .unwrap()
                .unwrap()
                .payload
        });
        sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });
    barrier.wait();
    assert!(matches!(
        receiver.recv_timeout(std::time::Duration::from_millis(50)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    let closed = exclusive
        .try_close_retired_generation(&original)
        .unwrap()
        .unwrap();
    assert!(std::fs::canonicalize(&original)
        .unwrap()
        .starts_with(std::fs::canonicalize(root.path()).unwrap()));
    std::fs::remove_dir_all(&original).unwrap();
    drop(closed);
    drop(exclusive);
    assert_eq!(
        receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap(),
        b"source.fjall.build.analysis-next"
    );
    reader.join().unwrap();
}

#[test]
fn an_extended_gc_window_returns_a_typed_busy_error() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("gc-busy".into());
    let control = open_source_db(root.path(), &id).unwrap();
    drop(control);
    let database = source_db_path(root.path(), &id);
    let exclusive = try_begin_generation_gc(database.parent().unwrap())
        .unwrap()
        .unwrap();
    let error = open_source_database_read_only(&database).unwrap_err();
    let persistence_sqlite::DbError::Io(error) = error else {
        panic!("expected typed I/O busy");
    };
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert!(error
        .get_ref()
        .unwrap()
        .downcast_ref::<persistence_source::control::connections::SourceGenerationBusy>()
        .is_some());
    drop(exclusive);
    assert!(open_source_database_read_only(&database).is_ok());
}

#[test]
fn source_deletion_waits_for_a_direct_handle_to_a_retired_generation() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("delete-direct".into());
    let control = open_source_db(root.path(), &id).unwrap();
    let original = bound_source_generation_root(&control, &id).unwrap();
    let retired = original
        .parent()
        .unwrap()
        .join("source.fjall.build.analysis-retired");
    seal(&original, &id);
    seal(&retired, &id);
    drop(
        persistence_source::runtime::close_source_store_at_path(&retired)
            .unwrap()
            .unwrap(),
    );
    bind_generation(&control, "source.fjall.build.analysis-retired").unwrap();
    let direct = persistence_source::detail::require_source_store(&control).unwrap();
    bind_generation(&control, "source.fjall").unwrap();
    drop(control);
    assert!(
        persistence_source::runtime::close_source_store(root.path(), &id.0)
            .unwrap()
            .is_none()
    );
    drop(direct);
    let closed = persistence_source::runtime::close_source_store(root.path(), &id.0)
        .unwrap()
        .unwrap();
    assert!(
        persistence_source::fjall::open_shared_source_store(&retired, Default::default()).is_err()
    );
    drop(closed);
    assert!(
        persistence_source::fjall::open_shared_source_store(&retired, Default::default()).is_ok()
    );
}
