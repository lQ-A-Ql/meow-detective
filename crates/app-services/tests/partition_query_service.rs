use app_services::{case_service, partition_query_service::PartitionQueryRepo, source_db};
use chrono::Utc;
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_source::fjall::{
    encode_key, KeyPart, PartitionRecord, PersistMode, VersionedEnvelope,
};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    storage_control_repo::{StorageControlRepo, StorageOperationRecord, StorageOperationState},
};

#[test]
fn published_partition_reads_use_fjall_and_unpublished_sources_fail() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "Partition query", None).unwrap();
    let source_id = DataSourceId("partition-query-source".to_owned());
    let source_connection = source_db::open_source_db(&active.case_root, &source_id).unwrap();

    assert!(PartitionQueryRepo::new(&source_connection)
        .find_by_data_source(&source_id.0)
        .is_err());

    let source = source_db::open_fjall_source_for_build(&source_connection, &source_id).unwrap();
    source
        .partition_replace_by_source(
            &source_id.0,
            &[PartitionRecord {
                id: "partition-0".to_owned(),
                data_source_id: source_id.0.clone(),
                partition_index: 0,
                name: "volume-0".to_owned(),
                kind_label: "NTFS".to_owned(),
                status: "fjall".to_owned(),
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
                gpt_disk_guid: None,
                gpt_partition_uuid: None,
                filesystem_uuid: None,
            }],
            PersistMode::Buffer,
        )
        .unwrap();
    let marker = VersionedEnvelope::new(
        "source_projection_complete",
        1,
        "projection:partition-query-source",
        &source_id.0,
        1,
        source_id.0.as_bytes().to_vec(),
    );
    let receipt = source
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            encode_key("projection", &[KeyPart::Text(source_id.0.clone())]).unwrap(),
            &marker,
            PersistMode::SyncAll,
        )
        .unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        source.source_store(),
        &source_id.0,
        "partition-query-fixture",
        &hex::encode(receipt.batch_digest),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    drop(source);

    let published = PartitionQueryRepo::new(&source_connection)
        .find_by_data_source(&source_id.0)
        .unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].status, "fjall");
    assert_eq!(
        PartitionQueryRepo::new(&source_connection)
            .find_by_data_source_and_index(&source_id.0, 0)
            .unwrap()
            .unwrap()
            .status,
        "fjall"
    );
}

#[test]
fn active_import_partition_queries_read_the_attempt_build() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "Partition build query", None).unwrap();
    let source_id = DataSourceId("partition-build-source".to_owned());
    let attempt_id = "00000000-0000-4000-8000-000000000001";
    let evidence_path = active.case_root.join("partition-build.E01");
    let data_source = DataSource {
        id: source_id.clone(),
        name: "partition-build-source".to_owned(),
        kind: DataSourceKind::E01,
        source_path: evidence_path,
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source_id.0, Some("windows"), None);
    storage.import_state = "importing".to_owned();
    active
        .with_conn(|connection| {
            DataSourceRepo::new(connection).insert_with_storage(
                &active.meta.id,
                &data_source,
                &storage,
            )?;
            StorageControlRepo::new(connection).insert_operation(&StorageOperationRecord {
                operation_id: format!("source-import:{attempt_id}"),
                case_id: active.meta.id.0.clone(),
                source_id: Some(source_id.0.clone()),
                attempt_id: attempt_id.to_owned(),
                operation_kind: "source_import".to_owned(),
                state: StorageOperationState::Running,
                input_digest: None,
                expected_batch_digest: None,
                created_at: Utc::now().to_rfc3339(),
                updated_at: Utc::now().to_rfc3339(),
                last_error_code: None,
            })
        })
        .unwrap();

    let source_connection = source_db::open_source_db(&active.case_root, &source_id).unwrap();
    DataSourceRepo::new(&source_connection)
        .upsert_source_local_metadata(&active.meta.id, &data_source)
        .unwrap();
    let published = source_db::open_fjall_source_for_build(&source_connection, &source_id).unwrap();
    published
        .partition_replace_by_source(
            &source_id.0,
            &[partition_record(&source_id.0, "partition-published")],
            PersistMode::SyncAll,
        )
        .unwrap();
    let (keyspace, key, marker) =
        persistence_source::runtime::partition_marker::partition_projection_marker(&source_id.0)
            .unwrap();
    let receipt = published
        .write_envelope(keyspace, key, &marker, PersistMode::SyncAll)
        .unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        published.source_store(),
        &source_id.0,
        "partition-build-fixture",
        &hex::encode(receipt.batch_digest),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    drop(published);
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        &active.case_root,
        &source_id.0,
        attempt_id,
    )
    .unwrap();
    let runtime =
        app_services::storage_runtime::ReadWriteStorageRuntime::open(layout.build_path()).unwrap();
    runtime
        .source()
        .partition_replace_by_source(
            &source_id.0,
            &[partition_record(&source_id.0, "partition-build")],
            PersistMode::SyncAll,
        )
        .unwrap();
    drop(runtime);

    let partitions = PartitionQueryRepo::new(&source_connection)
        .find_by_data_source(&source_id.0)
        .unwrap();
    assert_eq!(partitions.len(), 1);
    assert_eq!(partitions[0].id, "partition-build");
}

fn partition_record(source_id: &str, id: &str) -> PartitionRecord {
    PartitionRecord {
        id: id.to_owned(),
        data_source_id: source_id.to_owned(),
        partition_index: 0,
        name: "volume-0".to_owned(),
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
        gpt_disk_guid: None,
        gpt_partition_uuid: None,
        filesystem_uuid: None,
    }
}
