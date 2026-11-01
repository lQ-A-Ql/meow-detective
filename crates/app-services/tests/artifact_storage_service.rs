use app_services::artifact_storage_service::{
    create_reference, read_object, release_reference, write_object, ArtifactStorageServiceError,
};
use artifact_storage::ArtifactStore;
use persistence_sqlite::repositories::storage_control_repo::StorageControlRepo;
use rusqlite::Connection;
use tempfile::tempdir;

fn database() -> Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open database");
    persistence_sqlite::runner::run_all(&connection).expect("run migrations");
    connection
        .execute("INSERT INTO cases (id, name) VALUES ('case-1', 'Case')", [])
        .expect("insert case");
    connection
}

#[test]
fn writes_registers_references_reads_and_releases() {
    let connection = database();
    let root = tempdir().expect("temporary root");
    let store = ArtifactStore::new(root.path()).expect("create artifact store");
    let object = write_object(
        &connection,
        &store,
        "case-1",
        b"artifact bytes",
        "application/octet-stream",
    )
    .expect("write object");

    let state: String = connection
        .query_row(
            "SELECT write_state FROM artifact_objects WHERE object_id = ?1",
            [&object.object_id],
            |row| row.get(0),
        )
        .expect("read object state");
    assert_eq!(state, "ready");

    let reference = create_reference(
        &connection,
        "case-1",
        &object.object_id,
        None,
        "report",
        "report-1",
        "attachment",
    )
    .expect("create reference");
    assert_eq!(
        read_object(&connection, &store, "case-1", &object.object_id).unwrap(),
        b"artifact bytes"
    );
    release_reference(&connection, "case-1", &reference.reference_id).expect("release reference");
    let released: Option<String> = connection
        .query_row(
            "SELECT released_at FROM artifact_references WHERE reference_id = ?1",
            [&reference.reference_id],
            |row| row.get(0),
        )
        .expect("read release timestamp");
    assert!(released.is_some());
}

#[test]
fn reads_reject_non_ready_objects_and_references() {
    let connection = database();
    let root = tempdir().expect("temporary root");
    let store = ArtifactStore::new(root.path()).expect("create artifact store");
    let object = write_object(&connection, &store, "case-1", b"pending", "text/plain")
        .expect("write object");
    StorageControlRepo::new(&connection)
        .update_artifact_state(&object.object_id, "quarantined")
        .expect("quarantine object");

    assert!(matches!(
        read_object(&connection, &store, "case-1", &object.object_id),
        Err(ArtifactStorageServiceError::ObjectNotReady { .. })
    ));
    assert!(matches!(
        create_reference(
            &connection,
            "case-1",
            &object.object_id,
            None,
            "note",
            "note-1",
            "attachment"
        ),
        Err(ArtifactStorageServiceError::ObjectNotReady { .. })
    ));
}
