use domain::DataSourceId;
use persistence_source::control::{open_source_db, source_db_path};
use persistence_source::detail::bind_generation;
use persistence_source::fjall::{KeyspaceName, PersistMode, VersionedEnvelope};
use persistence_source::runtime::{
    open_source_read_only, open_source_read_write, source_storage_root,
    ReadOnlySourceStorageHandle, ReadWriteStorageRuntime,
};
use std::path::Path;

fn write_generation(path: &Path, source: &str, value: &[u8]) {
    let runtime = ReadWriteStorageRuntime::open(path).unwrap();
    let record = VersionedEnvelope::new("generation_test", 1, "record", source, 1, value.to_vec());
    let receipt = runtime
        .source()
        .write_envelope(
            KeyspaceName::Meta,
            b"generation_test".to_vec(),
            &record,
            PersistMode::SyncAll,
        )
        .unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        runtime.source().source_store(),
        source,
        "generation-test",
        &hex::encode(receipt.batch_digest),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
}

fn read_value(source: &ReadOnlySourceStorageHandle) -> Vec<u8> {
    source
        .read_envelope(&source.snapshot(), KeyspaceName::Meta, b"generation_test")
        .unwrap()
        .unwrap()
        .payload
}

#[test]
fn current_generation_changes_only_after_the_sqlite_binding_commits() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("source-generation".into());
    let mut control = open_source_db(root.path(), &id).unwrap();
    let original =
        persistence_source::control::bound_source_generation_root(&control, &id).unwrap();
    let next = original
        .parent()
        .unwrap()
        .join("source.fjall.build.analysis-next");
    write_generation(&original, &id.0, b"original");
    write_generation(&next, &id.0, b"next");
    let next = std::fs::canonicalize(next).unwrap();
    let pinned_original = open_source_read_only(root.path(), &id.0).unwrap();

    let transaction = control.transaction().unwrap();
    bind_generation(&transaction, "source.fjall.build.analysis-next").unwrap();
    assert_eq!(
        read_value(&open_source_read_only(root.path(), &id.0).unwrap()),
        b"original"
    );
    transaction.commit().unwrap();

    let current = open_source_read_only(root.path(), &id.0).unwrap();
    assert_eq!(current.source_root(), next);
    assert_eq!(read_value(&current), b"next");
    assert_eq!(
        open_source_read_write(root.path(), &id.0)
            .unwrap()
            .source_root(),
        next
    );
    assert_eq!(read_value(&pinned_original), b"original");
    let details = persistence_source::detail::require_source_store(&control).unwrap();
    assert_eq!(details.database().path(), current.source_root());
}

#[test]
fn missing_control_database_does_not_select_an_unregistered_directory() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("unregistered".into());
    let original = root.path().join("sources/unregistered/source.fjall");
    write_generation(&original, &id.0, b"unregistered");
    assert!(open_source_read_only(root.path(), &id.0).is_err());
    assert!(open_source_read_write(root.path(), &id.0).is_err());
    assert!(!source_db_path(root.path(), &id).exists());
}

#[test]
fn current_generation_rejects_a_mismatched_control_owner() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("source-owner".into());
    let control = open_source_db(root.path(), &id).unwrap();
    control
        .execute(
            "UPDATE source_meta SET value='another-source' WHERE key='data_source_id'",
            [],
        )
        .unwrap();
    assert!(source_storage_root(root.path(), &id.0).is_err());
}

#[test]
fn current_generation_rejects_an_invalid_binding_without_creating_it() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("source-invalid".into());
    let control = open_source_db(root.path(), &id).unwrap();
    control
        .execute(
            "UPDATE source_meta SET value='../outside' WHERE key='fjall_root'",
            [],
        )
        .unwrap();
    assert!(source_storage_root(root.path(), &id.0).is_err());
    assert!(!root.path().join("sources/outside").exists());
}

#[test]
fn current_generation_rejects_a_foreign_source_manifest() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("expected-owner".into());
    let control = open_source_db(root.path(), &id).unwrap();
    let path = persistence_source::control::bound_source_generation_root(&control, &id).unwrap();
    write_generation(&path, "foreign-owner", b"foreign");
    assert!(open_source_read_only(root.path(), &id.0).is_err());
}

#[test]
fn current_generation_rejects_a_missing_seal() {
    let root = tempfile::tempdir().unwrap();
    let id = DataSourceId("missing-seal".into());
    let control = open_source_db(root.path(), &id).unwrap();
    let path = persistence_source::control::bound_source_generation_root(&control, &id).unwrap();
    write_generation(&path, &id.0, b"sealed");
    std::fs::remove_file(path.join(persistence_source::fjall::SOURCE_GENERATION_SEAL_FILE))
        .unwrap();
    assert!(open_source_read_only(root.path(), &id.0).is_err());
}
