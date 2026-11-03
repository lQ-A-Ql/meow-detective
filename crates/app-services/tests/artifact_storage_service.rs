use app_services::artifact_storage_service::{
    create_reference, read_object, register_artifact_objects_and_references_batch,
    release_reference, write_object, ArtifactObjectBatchItem, ArtifactStorageService,
    ArtifactStorageServiceError,
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
    let duplicate = create_reference(
        &connection,
        "case-1",
        &object.object_id,
        None,
        "report",
        "report-1",
        "attachment",
    )
    .expect("duplicate reference is idempotent");
    assert_eq!(duplicate.reference_id, reference.reference_id);
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

#[test]
fn cleanup_removes_only_released_unreferenced_objects() {
    let connection = database();
    let root = tempdir().expect("temporary root");
    let store = ArtifactStore::new(root.path()).expect("create artifact store");
    let service = ArtifactStorageService::new(&connection, &store);
    let object = service
        .write_object("case-1", b"cleanup", "text/plain")
        .expect("write object");
    let reference = service
        .create_reference("case-1", &object.object_id, None, "test", "1", "input")
        .expect("reference");
    assert_eq!(service.cleanup_unreferenced("case-1").unwrap(), 0);
    service
        .release_reference("case-1", &reference.reference_id)
        .unwrap();
    assert_eq!(service.cleanup_unreferenced("case-1").unwrap(), 1);
    assert!(matches!(
        service.read_object("case-1", &object.object_id),
        Err(ArtifactStorageServiceError::ObjectNotReady { .. })
            | Err(ArtifactStorageServiceError::ObjectNotFound(_))
    ));
}

#[test]
fn corrupted_ready_object_is_rejected_before_read_or_publish() {
    let connection = database();
    let root = tempdir().expect("temporary root");
    let store = ArtifactStore::new(root.path()).expect("create artifact store");
    let service = ArtifactStorageService::new(&connection, &store);
    let object = service
        .write_object("case-1", b"immutable", "text/plain")
        .expect("write object");
    std::fs::write(store.object_path(&object.address), b"tampered").expect("tamper object");

    assert!(matches!(
        service.read_object("case-1", &object.object_id),
        Err(ArtifactStorageServiceError::Storage(
            artifact_storage::ArtifactStorageError::DigestMismatch { .. }
        ))
    ));
    assert!(matches!(
        service.create_reference("case-1", &object.object_id, None, "report", "r1", "file"),
        Err(ArtifactStorageServiceError::Storage(
            artifact_storage::ArtifactStorageError::DigestMismatch { .. }
        ))
    ));
}

#[test]
fn writes_file_through_content_addressed_service() {
    let connection = database();
    let root = tempdir().expect("temporary root");
    let store = ArtifactStore::new(root.path()).expect("create artifact store");
    let input = root.path().join("input.bin");
    std::fs::write(&input, b"file bytes").expect("write input");
    let object = ArtifactStorageService::new(&connection, &store)
        .write_file("case-1", &input, "application/octet-stream")
        .expect("write file");
    assert_eq!(
        ArtifactStorageService::new(&connection, &store)
            .read_object("case-1", &object.object_id)
            .expect("read file object"),
        b"file bytes"
    );
}

#[test]
fn batch_registers_objects_and_references_atomically() {
    let connection = database();
    let root = tempdir().expect("temporary root");
    let store = ArtifactStore::new(root.path()).expect("create artifact store");
    let first = store.put_bytes(b"batch-one").expect("store first object");
    let second = store.put_bytes(b"batch-two").expect("store second object");
    register_artifact_objects_and_references_batch(
        &connection,
        "case-1",
        &[
            ArtifactObjectBatchItem {
                stored: first,
                media_type: "application/json".to_owned(),
                source_id: None,
                owner_kind: "artifact".to_owned(),
                owner_id: "artifact-1".to_owned(),
                role: "metadata".to_owned(),
            },
            ArtifactObjectBatchItem {
                stored: second,
                media_type: "application/json".to_owned(),
                source_id: None,
                owner_kind: "artifact".to_owned(),
                owner_id: "artifact-2".to_owned(),
                role: "metadata".to_owned(),
            },
        ],
    )
    .expect("register batch");
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM artifact_objects", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM artifact_references", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
