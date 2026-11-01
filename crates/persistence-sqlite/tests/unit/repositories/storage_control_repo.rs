use super::*;

fn setup() -> rusqlite::Connection {
    let connection = crate::open_in_memory().expect("open database");
    crate::runner::run_all(&connection).expect("run migrations");
    connection
        .execute("INSERT INTO cases (id, name) VALUES ('case-1', 'Case')", [])
        .expect("insert case");
    connection
        .execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path)
             VALUES ('source-1', 'case-1', 'Source', 'e01', '')",
            [],
        )
        .expect("insert source");
    connection
}

#[test]
fn operation_state_and_open_listing_round_trip() {
    let connection = setup();
    let repo = StorageControlRepo::new(&connection);
    repo.insert_operation(&StorageOperationRecord {
        operation_id: "operation-1".to_owned(),
        case_id: "case-1".to_owned(),
        source_id: Some("source-1".to_owned()),
        attempt_id: "attempt-1".to_owned(),
        operation_kind: "source_build".to_owned(),
        state: StorageOperationState::Planned,
        input_digest: Some("a".repeat(64)),
        expected_batch_digest: None,
        created_at: "2026-11-01T00:00:00Z".to_owned(),
        updated_at: "2026-11-01T00:00:00Z".to_owned(),
        last_error_code: None,
    })
    .expect("insert operation");

    assert_eq!(repo.list_open_operations("case-1").unwrap().len(), 1);
    assert!(repo
        .update_operation_state("operation-1", StorageOperationState::Published, "now", None)
        .unwrap());
    assert!(repo.list_open_operations("case-1").unwrap().is_empty());
    assert_eq!(
        repo.find_operation("operation-1").unwrap().unwrap().state,
        StorageOperationState::Published
    );
}

#[test]
fn manifest_artifact_and_repair_records_round_trip() {
    let connection = setup();
    let repo = StorageControlRepo::new(&connection);
    repo.insert_manifest(&StorageManifestRecord {
        manifest_id: "manifest-1".to_owned(),
        case_id: "case-1".to_owned(),
        source_id: Some("source-1".to_owned()),
        relative_path: "sources/source-1/source.fjall".to_owned(),
        format_version: 1,
        schema_version: 1,
        manifest_digest: "b".repeat(64),
        state: "building".to_owned(),
        sealed_at: None,
        published_at: None,
    })
    .expect("insert manifest");
    assert_eq!(
        repo.find_manifest_by_path("case-1", "sources/source-1/source.fjall")
            .unwrap()
            .unwrap()
            .manifest_id,
        "manifest-1"
    );
    assert!(repo
        .update_manifest_state("manifest-1", "published", Some("seal"), Some("publish"))
        .unwrap());

    repo.register_artifact_object(&ArtifactObjectRecord {
        object_id: "object-1".to_owned(),
        case_id: "case-1".to_owned(),
        sha256: "c".repeat(64),
        original_length: 10,
        stored_length: 10,
        media_type: "application/octet-stream".to_owned(),
        compression: "none".to_owned(),
        relative_path: "artifacts/objects/cc/cc/object".to_owned(),
        write_state: "ready".to_owned(),
    })
    .expect("insert artifact");
    assert_eq!(
        repo.find_artifact_by_digest("case-1", &"c".repeat(64))
            .unwrap()
            .unwrap()
            .object_id,
        "object-1"
    );
    assert!(repo.update_artifact_state("object-1", "deleted").unwrap());

    repo.insert_repair_event(&StorageRepairEventRecord {
        event_id: "repair-1".to_owned(),
        case_id: "case-1".to_owned(),
        source_id: Some("source-1".to_owned()),
        operation_id: None,
        event_kind: "manifest_check".to_owned(),
        state: "completed".to_owned(),
        details_json: Some("{}".to_owned()),
        error_code: None,
    })
    .expect("insert repair event");
}
