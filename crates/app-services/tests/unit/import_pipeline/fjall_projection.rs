use chrono::Utc;
use domain::{
    Artifact, ArtifactId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance,
    FileEntryId, TimelineEvent, TimelineEventId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
    timeline_repo::TimelineRepo,
};
use std::collections::BTreeMap;

fn artifact(id: &str, source_id: &str) -> Artifact {
    Artifact {
        id: ArtifactId(id.to_owned()),
        family: "analysis_test".to_owned(),
        title: id.to_owned(),
        summary: "summary".to_owned(),
        source_object_id: Some(FileEntryId(source_id.to_owned())),
        extractor_id: Some("fixture.parser".to_owned()),
        extractor_version: Some("1".to_owned()),
        confidence: Some(0.9),
        source_attribution: None,
        created_at: Utc::now(),
        attrs: BTreeMap::from([(
            "dataSourceId".to_owned(),
            serde_json::Value::String("projection-source".to_owned()),
        )]),
    }
}

fn event(id: &str, source_id: &str) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(id.to_owned()),
        source_object_id: source_id.to_owned(),
        event_type: "TEST_EVENT".to_owned(),
        timestamp: Utc::now(),
        title: id.to_owned(),
        description: "description".to_owned(),
        parser_id: Some("fixture.parser".to_owned()),
        parser_version: Some("1".to_owned()),
        confidence: Some(0.9),
        source_attribution: None,
        attrs: BTreeMap::new(),
    }
}

#[test]
fn partition_projection_replaces_stale_source_rows() {
    let directory = tempfile::tempdir().unwrap();
    let case =
        crate::case_service::create_case(directory.path(), "Partition projection", None).unwrap();
    let source_id = DataSourceId("partition-projection-source".to_owned());
    let source_connection = crate::source_db::open_source_db(&case.case_root, &source_id).unwrap();
    PartitionRepo::new(&source_connection)
        .insert_batch(&[DataSourcePartitionRecord {
            id: "partition-new".to_owned(),
            data_source_id: source_id.0.clone(),
            partition_index: 2,
            name: "volume".to_owned(),
            kind_label: "NTFS".to_owned(),
            status: "ready".to_owned(),
            type_guid: Some("type-guid".to_owned()),
            offset: 4096,
            length: 8192,
            filesystem: Some("NTFS".to_owned()),
            unlock_hint: None,
            lvm_vg_uuid: None,
            lvm_vg_name: None,
            lvm_lv_uuid: None,
            lvm_lv_name: None,
            lvm_pv_offsets_json: None,
            lvm_pv_sources_json: None,
        }])
        .unwrap();

    let fjall =
        crate::source_db::open_fjall_source_read_write(&case.case_root, &source_id.0).unwrap();
    fjall
        .partition_replace_by_source(
            &source_id.0,
            &[persistence_fjall::PartitionRecord {
                id: "partition-old".to_owned(),
                data_source_id: source_id.0.clone(),
                partition_index: 1,
                name: "old-volume".to_owned(),
                kind_label: "NTFS".to_owned(),
                status: "ready".to_owned(),
                type_guid: None,
                offset: 0,
                length: 4096,
                filesystem: Some("NTFS".to_owned()),
                unlock_hint: None,
                lvm_vg_uuid: None,
                lvm_vg_name: None,
                lvm_lv_uuid: None,
                lvm_lv_name: None,
                lvm_pv_offsets_json: None,
                lvm_pv_sources_json: None,
            }],
            persistence_fjall::PersistMode::Buffer,
        )
        .unwrap();
    crate::import_pipeline::fjall_projection::mirror_source_partitions(
        &source_connection,
        &fjall,
        &source_id.0,
    )
    .unwrap();
    drop(fjall);

    let fjall =
        crate::source_db::open_fjall_source_read_only(&case.case_root, &source_id.0).unwrap();
    let snapshot = fjall.snapshot();
    let page = fjall
        .partition_list_by_source(
            &snapshot,
            &source_id.0,
            None,
            persistence_fjall::ScanDirection::Forward,
            10,
        )
        .unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].id, "partition-new");
    assert_eq!(page.records[0].offset, 4096);
}

#[test]
fn analysis_projection_removes_deleted_rows_and_skips_unchanged_replays() {
    let directory = tempfile::tempdir().unwrap();
    let case = crate::case_service::create_case(directory.path(), "Projection", None).unwrap();
    let source_id = DataSourceId("projection-source".to_owned());
    let source = DataSource {
        id: source_id.clone(),
        name: "Projection source".to_owned(),
        kind: DataSourceKind::E01,
        source_path: directory.path().join("source.E01"),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source_id.0, Some("linux"), None);
    storage.import_state = "ready".to_owned();
    case.with_conn(|connection| {
        DataSourceRepo::new(connection).insert_with_storage(&case.meta.id, &source, &storage)
    })
    .unwrap();
    let source_connection = crate::source_db::open_source_db(&case.case_root, &source_id).unwrap();
    DataSourceRepo::new(&source_connection)
        .upsert_source_local_metadata(&case.meta.id, &source)
        .unwrap();
    ArtifactRepo::new(&source_connection)
        .insert_batch(
            &[
                artifact("old-artifact", "file-old"),
                artifact("stable-artifact", "file-stable"),
            ],
            &case.meta.id.0,
            &source_id.0,
        )
        .unwrap();
    TimelineRepo::new(&source_connection)
        .insert_batch_with_case(
            &[
                event("old-event", "file-old"),
                event("stable-event", "file-stable"),
            ],
            &case.meta.id.0,
        )
        .unwrap();

    let project = |source_connection: &rusqlite::Connection| {
        case.with_conn(|case_connection| {
            crate::import_pipeline::mirror_analysis_outputs_from_connections(
                case_connection,
                &case.case_root,
                &case.meta.id.0,
                source_connection,
                &source_id.0,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
    };
    let first_receipt = project(&source_connection).unwrap();
    let first_change_high_water = persistence_sqlite::repositories::
        source_projection_change_repo::SourceProjectionChangeRepo::new(&source_connection)
        .high_water()
        .unwrap();
    assert_eq!(
        persistence_sqlite::repositories::source_projection_change_repo::
            SourceProjectionChangeRepo::new(&source_connection)
            .list_after(0, first_change_high_water, 512)
            .unwrap()
            .len(),
        0,
        "full reconciliation must retire its captured change range"
    );

    let transaction = source_connection.unchecked_transaction().unwrap();
    ArtifactRepo::new(&transaction)
        .delete_analysis_outputs_by_extractor_in_transaction("file-old", "fixture.parser")
        .unwrap();
    TimelineRepo::new(&transaction)
        .delete_analysis_outputs_by_parser_in_transaction("file-old", "fixture.parser")
        .unwrap();
    ArtifactRepo::new(&transaction)
        .insert_batch_in_transaction(
            &[artifact("new-artifact", "file-new")],
            &case.meta.id.0,
            &source_id.0,
        )
        .unwrap();
    TimelineRepo::new(&transaction)
        .insert_batch_with_case_in_transaction(&[event("new-event", "file-new")], &case.meta.id.0)
        .unwrap();
    transaction.commit().unwrap();
    let change_repo = persistence_sqlite::repositories::
        source_projection_change_repo::SourceProjectionChangeRepo::new(&source_connection);
    let changed_records = change_repo
        .list_after(
            first_change_high_water,
            change_repo.high_water().unwrap(),
            512,
        )
        .unwrap();
    assert_eq!(changed_records.len(), 4);
    assert!(changed_records
        .iter()
        .all(|change| !change.entity_id.starts_with("stable-")));
    assert!(ArtifactRepo::new(&source_connection)
        .find_by_id("old-artifact")
        .unwrap()
        .is_none());
    assert!(
        ArtifactRepo::new(&source_connection)
            .cursor_revision()
            .unwrap()
            > 0
    );
    let second_receipt = project(&source_connection).unwrap();
    assert_ne!(first_receipt, second_receipt);

    let fjall =
        crate::source_db::open_registered_fjall_source_read_only(&case.case_root, &source_id)
            .unwrap();
    let snapshot = fjall.snapshot();
    assert!(fjall
        .artifact_get(&snapshot, &source_id.0, "old-artifact")
        .unwrap()
        .is_none());
    assert!(fjall
        .timeline_get(&snapshot, &source_id.0, "old-event")
        .unwrap()
        .is_none());
    assert!(fjall
        .artifact_get(&snapshot, &source_id.0, "new-artifact")
        .unwrap()
        .is_some());
    assert!(fjall
        .timeline_get(&snapshot, &source_id.0, "new-event")
        .unwrap()
        .is_some());
    assert!(fjall
        .artifact_get(&snapshot, &source_id.0, "stable-artifact")
        .unwrap()
        .is_some());
    assert!(fjall
        .timeline_get(&snapshot, &source_id.0, "stable-event")
        .unwrap()
        .is_some());
    let released_references: i64 = case
        .with_conn(|connection| {
            connection
                .query_row(
                    "SELECT COUNT(*) FROM artifact_references
                 WHERE case_id = ?1 AND source_id = ?2 AND owner_id = 'old-artifact'
                   AND role = 'metadata' AND released_at IS NOT NULL",
                    [&case.meta.id.0, &source_id.0],
                    |row| row.get(0),
                )
                .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(released_references, 1);

    let revision = fjall.timeline_cursor_revision(&snapshot).unwrap();
    let batches_before = fjall
        .snapshot()
        .prefix(
            persistence_fjall::KeyspaceName::Batches,
            b"batch/",
            persistence_fjall::ScanDirection::Forward,
        )
        .unwrap()
        .len();
    project(&source_connection).unwrap();
    let batches_after = fjall
        .snapshot()
        .prefix(
            persistence_fjall::KeyspaceName::Batches,
            b"batch/",
            persistence_fjall::ScanDirection::Forward,
        )
        .unwrap()
        .len();
    assert_eq!(batches_after, batches_before);
    assert_eq!(
        fjall.timeline_cursor_revision(&fjall.snapshot()).unwrap(),
        revision
    );
}
