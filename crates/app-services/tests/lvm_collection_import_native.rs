use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use app_services::{
    case_service, cluster_service, content_index_service, file_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_with_counts, ImportJobOptions},
    search_service, source_db, timeline_service,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo,
    datasource_repo::DataSourceRepo,
    job_repo::JobRepo,
    processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim, ProcessingPhaseState,
    },
    source_revision_repo::SourceRevisionRepo,
    storage_object_repo::StorageObjectRepo,
    structure_view_attempt_repo::StructureViewAttemptRepo,
};

#[test]
fn native_collection_publishes_independent_lvm_source_with_preview_and_lifecycle() {
    let workspace = tempfile::Builder::new()
        .prefix("lvm-source-")
        .tempdir_in(scratch_root())
        .unwrap();
    let input = workspace.path().join("images");
    std::fs::create_dir(&input).unwrap();
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fixtures/public-small");
    for index in 0..2 {
        std::io::copy(
            &mut flate2::read::GzDecoder::new(
                File::open(fixtures.join(format!("lvm-native/shared-ext4/pv-{index}.raw.gz")))
                    .unwrap(),
            ),
            &mut File::create(input.join(format!("pv-{index}.raw"))).unwrap(),
        )
        .unwrap();
    }
    let plan =
        cluster_service::plan_linux_evidence_set_import(&input, Some("native-shared".into()))
            .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let shared = plan.prepare_shared_sources(&cancel).unwrap();
    let configs = plan
        .member_import_configs_from_plan(Arc::clone(&shared))
        .unwrap();
    let case = case_service::create_case(workspace.path(), "LVM logical source", None).unwrap();
    let connection = case.connection().unwrap();
    cluster_service::register_linux_evidence_set_import(&connection, &case.meta.id, &plan).unwrap();
    cluster_service::write_linux_evidence_set_manifest(&case.case_root, &plan).unwrap();
    for config in configs.into_iter().rev() {
        let job = JobRepo::new(&connection)
            .create(&case.meta.id.0, "native PV member")
            .unwrap();
        let (_, counts) = execute_import_job_with_counts(
            &connection,
            &case.meta.id,
            &case.case_root,
            config,
            &job,
            ImportJobOptions {
                projection_policy: Default::default(),
                event_sink: None,
                cancel_token: &cancel,
                max_import_workers: Some(1),
                max_analysis_workers: Some(1),
                analysis_mode: ImportAnalysisMode::MetadataOnly,
                defer_linux_artifacts: false,
            },
        )
        .unwrap();
        assert_eq!(counts.failed_count, 0);
    }
    cluster_service::validate_collection_publication(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plan,
        &shared,
        &cancel,
    )
    .unwrap();
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_by_case(&case.meta.id)
            .unwrap()
            .len(),
        2
    );
    for source in DataSourceRepo::new(&connection)
        .find_by_case(&case.meta.id)
        .unwrap()
    {
        let source_conn = source_db::open_source_db(&case.case_root, &source.id).unwrap();
        let native =
            source_db::open_registered_fjall_source_read_only(&case.case_root, &source.id).unwrap();
        let partitions = native
            .partition_list_by_source(
                &native.snapshot(),
                &source.id.0,
                None,
                persistence_source::fjall::ScanDirection::Forward,
                16,
            )
            .unwrap();
        assert!(partitions
            .records
            .iter()
            .all(|partition| { partition.filesystem.as_deref() != Some("Ext4") }));
        for partition in &partitions.records {
            assert!(FileRepo::new(&source_conn)
                .find_by_partition_and_path(
                    &source.id,
                    usize::try_from(partition.partition_index).unwrap(),
                    "evidence.bin",
                )
                .unwrap()
                .is_none());
        }
    }

    let lv_sources = cluster_service::materialize_linux_evidence_set_lvm(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plan,
        &shared,
    )
    .unwrap();
    assert_eq!(lv_sources.len(), 1);
    let logical = &lv_sources[0];
    assert_eq!(logical.kind, domain::DataSourceKind::LvmLogicalVolume);
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_by_case(&case.meta.id)
            .unwrap()
            .len(),
        3
    );
    let repeated = cluster_service::materialize_linux_evidence_set_lvm(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plan,
        &shared,
    )
    .unwrap();
    assert_eq!(repeated[0].id, logical.id);
    assert!(cluster_service::materialize_lvm_source_catalog(
        &connection,
        &case.case_root,
        &logical.id,
    )
    .is_ok());
    let source_conn = source_db::open_source_db(&case.case_root, &logical.id).unwrap();
    let native =
        source_db::open_registered_fjall_source_read_only(&case.case_root, &logical.id).unwrap();
    let partitions = native
        .partition_list_by_source(
            &native.snapshot(),
            &logical.id.0,
            None,
            persistence_source::fjall::ScanDirection::Forward,
            16,
        )
        .unwrap();
    let ext4 = partitions
        .records
        .iter()
        .find(|partition| partition.filesystem.as_deref() == Some("Ext4"))
        .unwrap();
    let file = FileRepo::new(&source_conn)
        .find_by_partition_and_path(
            &logical.id,
            usize::try_from(ext4.partition_index).unwrap(),
            "evidence.bin",
        )
        .unwrap()
        .unwrap();
    let expected = std::fs::read(fixtures.join("mdraid/oracle.bin")).unwrap();
    let preview = file_service::FileHeaderReadCache::new(case.meta.id.0.clone())
        .read_file_header_by_id(&source_conn, &file.id, 1024)
        .unwrap();
    assert_eq!(preview, expected[..1024]);
    let mut stream = file_service::open_file_content_by_id(&source_conn, &file.id).unwrap();
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut stream, &mut bytes).unwrap();
    assert_eq!(bytes, expected);
    drop(stream);

    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        cluster_service::materialize_linux_evidence_set_lvm_with_cancel(
            &connection,
            &case.case_root,
            &case.meta.id,
            &plan,
            &shared,
            &cancelled,
        ),
        Err(cluster_service::LvmPlanError::Cancelled)
    ));

    let revisions = SourceRevisionRepo::new(&connection);
    let phase_repo = DataSourceProcessingPhaseRepo::new(&connection);
    let catalog_phase = phase_repo
        .find(&logical.id, ProcessingPhase::Catalog)
        .unwrap()
        .unwrap();
    assert_eq!(catalog_phase.state, ProcessingPhaseState::Ready);
    connection
        .execute(
            "UPDATE data_source_processing_phases
             SET state='pending', owner_id=NULL, attempt_id=NULL, stats_json='{}',
                 last_error=NULL, started_at=NULL, completed_at=NULL,
                 heartbeat_at=NULL, lease_expires_at=NULL, updated_at=datetime('now')
             WHERE data_source_id=?1 AND phase='catalog'",
            [&logical.id.0],
        )
        .unwrap();
    let competing_claim = phase_repo
        .claim(
            &logical.id,
            ProcessingPhase::Catalog,
            catalog_phase.version,
            &catalog_phase.input_fingerprint,
            "competing-owner",
        )
        .unwrap();
    assert!(matches!(competing_claim, ProcessingPhaseClaim::Acquired(_)));
    assert!(matches!(
        cluster_service::materialize_lvm_source_catalog(&connection, &case.case_root, &logical.id,),
        Err(cluster_service::LvmPlanError::Busy)
    ));
    connection
        .execute(
            "UPDATE data_source_processing_phases
             SET lease_expires_at='2000-01-01 00:00:00'
             WHERE data_source_id=?1 AND phase='catalog'",
            [&logical.id.0],
        )
        .unwrap();
    assert_eq!(
        phase_repo
            .recover_interrupted("test owner lease expired")
            .unwrap(),
        1
    );
    cluster_service::materialize_lvm_source_catalog(&connection, &case.case_root, &logical.id)
        .unwrap();

    let publication = CatalogPublicationRepo::new(&connection)
        .find(&logical.id)
        .unwrap()
        .unwrap();
    connection
        .execute(
            "UPDATE data_source_catalog_publications
             SET state='prepared', published_at=NULL
             WHERE data_source_id=?1",
            [&logical.id.0],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE data_source_processing_phases
             SET state='running', completed_at=NULL,
                 lease_expires_at='2000-01-01 00:00:00'
             WHERE data_source_id=?1 AND phase='catalog'",
            [&logical.id.0],
        )
        .unwrap();
    DataSourceRepo::new(&connection)
        .update_import_state(&logical.id, "pending", None)
        .unwrap();
    cluster_service::materialize_lvm_source_catalog(&connection, &case.case_root, &logical.id)
        .unwrap();
    assert_eq!(
        CatalogPublicationRepo::new(&connection)
            .find(&logical.id)
            .unwrap()
            .unwrap()
            .state,
        "published"
    );
    let recovered_attempt = StructureViewAttemptRepo::new(&connection)
        .find(&case.meta.id.0, &logical.id.0)
        .unwrap()
        .unwrap();
    assert_eq!(recovered_attempt.attempt_id, publication.attempt_id);

    connection
        .execute(
            "DELETE FROM structure_view_attempts WHERE derived_source_id=?1",
            [&logical.id.0],
        )
        .unwrap();
    let mut tampered_attempt = recovered_attempt.clone();
    tampered_attempt.receipt = format!(
        "{}{}",
        if tampered_attempt.receipt.starts_with('0') {
            "1"
        } else {
            "0"
        },
        &tampered_attempt.receipt[1..]
    );
    StructureViewAttemptRepo::new(&connection)
        .bind(&case.meta.id.0, &tampered_attempt)
        .unwrap();
    assert!(matches!(
        cluster_service::materialize_lvm_source_catalog(&connection, &case.case_root, &logical.id,),
        Err(cluster_service::LvmPlanError::InvalidPublication)
    ));
    connection
        .execute(
            "DELETE FROM structure_view_attempts WHERE derived_source_id=?1",
            [&logical.id.0],
        )
        .unwrap();
    StructureViewAttemptRepo::new(&connection)
        .bind(&case.meta.id.0, &recovered_attempt)
        .unwrap();
    cluster_service::materialize_lvm_source_catalog(&connection, &case.case_root, &logical.id)
        .unwrap();

    let version = StorageObjectRepo::new(&connection)
        .version_for_source(&case.meta.id.0, &logical.id.0)
        .unwrap()
        .unwrap();
    let graph = app_services::structure_analysis::get_structure_graph(
        &connection,
        &case.case_root,
        &app_services::structure_analysis::StructureGraphRequest {
            case_id: case.meta.id.clone(),
            source_ids: vec![logical.id.0.clone()],
        },
    )
    .unwrap();
    let request = app_services::structure_analysis::StructureAnalysisRequest {
        case_id: case.meta.id.clone(),
        source_ids: vec![logical.id.0.clone()],
        intent: domain::structure::ReadIntent::MetadataInspection,
        priority: domain::structure::ReadPriority::Interactive,
        consumers: vec!["lvm-range-test".into()],
        required_features: Default::default(),
        attempt: None,
    };
    let run = app_services::structure_analysis::run_structure_analysis(
        &connection,
        &case.case_root,
        &request,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(run
        .plan
        .work_items
        .iter()
        .any(|item| item.adapter_id == app_services::structure_analysis::LVM_ADAPTER_ID));
    assert!(run.work_results.iter().all(|result| result.bytes_read
        == result.receipt.ranges.covered_bytes()
        && result.receipt.output_sha256.len() == 64));
    assert!(graph
        .mappings
        .iter()
        .any(|mapping| mapping.target_node.0 == format!("version:{}", version.id.0)));
    assert!(graph.edges.iter().any(|edge| {
        edge.kind == domain::structure::StructureRelationKind::ContentDependency
            && version
                .parent_source_revisions
                .iter()
                .any(|parent| edge.to.0.contains(&parent.source_id))
    }));

    let index_summary = content_index_service::build(
        &connection,
        &case.case_root,
        &case.meta.id,
        &logical.id.0,
        &cancel,
        &|_, _| {},
    )
    .unwrap();
    assert!(index_summary.complete);
    let search = search_service::search_files_for_case(
        &connection,
        &case.case_root,
        &case.meta.id,
        &transport::commands::SearchFilesRequest {
            query: "evidence.bin".into(),
            match_path: true,
            entry_type: Default::default(),
            extensions: Vec::new(),
            data_source_ids: vec![logical.id.0.clone()],
            sort_key: Default::default(),
            sort_direction: Default::default(),
            offset: 0,
            limit: 100,
            cursor: None,
        },
    )
    .unwrap();
    assert!(search.total >= 1);
    assert!(search
        .items
        .iter()
        .all(|hit| hit.data_source_id == logical.id.0));

    timeline_service::rebuild_timeline_projection(
        &connection,
        &case.case_root,
        &case.meta.id,
        Some(&logical.id.0),
        Default::default(),
        &cancel,
    )
    .unwrap();
    let timeline = timeline_service::query_timeline_for_case(
        &connection,
        &case.case_root,
        &case.meta.id,
        0,
        100,
    )
    .unwrap();
    assert!(timeline.total >= 1);
    assert!(timeline
        .items
        .iter()
        .all(|event| event.data_source_id.as_deref() == Some(logical.id.0.as_str())));
    cluster_service::materialize_lvm_source_catalog(&connection, &case.case_root, &logical.id)
        .unwrap();

    let old_inputs = revisions.inputs(&case.meta.id.0, &logical.id.0).unwrap();
    let descriptor_before_replacement = std::fs::read(&logical.source_path).unwrap();
    let changed_parent = DataSourceRepo::new(&connection)
        .find_by_case(&case.meta.id)
        .unwrap()
        .into_iter()
        .find(|source| source.id.0 == old_inputs[0].source_id)
        .unwrap();
    let mut changed = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&changed_parent.source_path)
        .unwrap();
    changed.seek(SeekFrom::End(-1)).unwrap();
    let mut byte = [0];
    changed.read_exact(&mut byte).unwrap();
    changed.seek(SeekFrom::End(-1)).unwrap();
    changed.write_all(&[byte[0] ^ 0x80]).unwrap();
    changed.sync_all().unwrap();
    drop(changed);
    let updated_shared = plan.prepare_shared_sources(&cancel).unwrap();
    let mut replacement_inputs = Vec::new();
    for input in &old_inputs {
        let parent = DataSourceRepo::new(&connection)
            .find_by_case(&case.meta.id)
            .unwrap()
            .into_iter()
            .find(|source| source.id.0 == input.source_id)
            .unwrap();
        let content = updated_shared
            .source_content(&parent.source_path, &parent.kind)
            .unwrap();
        let (_, mut revision_content) = revisions
            .current(&case.meta.id.0, &input.source_id)
            .unwrap();
        revision_content.logical_size = content.logical_size;
        revision_content.logical_sha256 = Some(content.logical_sha256.clone());
        replacement_inputs.push(
            revisions
                .bind(&case.meta.id.0, &input.source_id, &revision_content, true)
                .unwrap(),
        );
    }
    assert!(revisions
        .require_current_inputs(&case.meta.id.0, &logical.id.0)
        .is_err());
    assert!(
        file_service::FileHeaderReadCache::new(case.meta.id.0.clone())
            .read_file_header_by_id(&source_conn, &file.id, 1024)
            .is_err()
    );
    let replacement = cluster_service::materialize_linux_evidence_set_lvm_with_cancel(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plan,
        &updated_shared,
        &cancel,
    )
    .unwrap();
    assert_eq!(replacement.len(), 1);
    let replacement = &replacement[0];
    assert_ne!(logical.id, replacement.id);
    assert_ne!(logical.source_path, replacement.source_path);
    assert_eq!(
        std::fs::read(&logical.source_path).unwrap(),
        descriptor_before_replacement
    );
    assert_eq!(
        revisions.inputs(&case.meta.id.0, &logical.id.0).unwrap(),
        old_inputs
    );
    assert_eq!(
        revisions
            .inputs(&case.meta.id.0, &replacement.id.0)
            .unwrap(),
        replacement_inputs
    );
    let old_version = StorageObjectRepo::new(&connection)
        .version_for_source(&case.meta.id.0, &logical.id.0)
        .unwrap()
        .unwrap();
    let replacement_version = StorageObjectRepo::new(&connection)
        .version_for_source(&case.meta.id.0, &replacement.id.0)
        .unwrap()
        .unwrap();
    assert_eq!(
        old_version.storage_object_id,
        replacement_version.storage_object_id
    );
    assert_ne!(old_version.id, replacement_version.id);
    assert_eq!(
        StorageObjectRepo::new(&connection)
            .versions_for_object(&case.meta.id.0, &old_version.storage_object_id)
            .unwrap()
            .len(),
        2
    );
    let (_, output_revision) = revisions
        .current(&case.meta.id.0, &replacement.id.0)
        .unwrap();
    assert_eq!(
        output_revision.metadata_identity.as_deref(),
        Some(replacement_version.id.0.as_str())
    );

    let replacement_conn = source_db::open_source_db(&case.case_root, &replacement.id).unwrap();
    let replacement_native =
        source_db::open_registered_fjall_source_read_only(&case.case_root, &replacement.id)
            .unwrap();
    let replacement_partitions = replacement_native
        .partition_list_by_source(
            &replacement_native.snapshot(),
            &replacement.id.0,
            None,
            persistence_source::fjall::ScanDirection::Forward,
            16,
        )
        .unwrap();
    let replacement_ext4 = replacement_partitions
        .records
        .iter()
        .find(|partition| partition.filesystem.as_deref() == Some("Ext4"))
        .unwrap();
    let replacement_file = FileRepo::new(&replacement_conn)
        .find_by_partition_and_path(
            &replacement.id,
            usize::try_from(replacement_ext4.partition_index).unwrap(),
            "evidence.bin",
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        file_service::FileHeaderReadCache::new(case.meta.id.0.clone())
            .read_file_header_by_id(&replacement_conn, &replacement_file.id, 1024)
            .unwrap(),
        expected[..1024]
    );

    drop(native);
    drop(source_conn);
    drop(replacement_native);
    drop(replacement_conn);
    case_service::delete_data_source_in(&connection, &case.case_root, &logical.id.0).unwrap();
    assert!(!source_db::source_dir(&case.case_root, &logical.id).exists());
    assert!(revisions.inputs(&case.meta.id.0, &logical.id.0).is_err());
    case_service::delete_data_source_in(&connection, &case.case_root, &replacement.id.0).unwrap();
    assert!(!source_db::source_dir(&case.case_root, &replacement.id).exists());
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_by_case(&case.meta.id)
            .unwrap()
            .len(),
        2
    );
    case_service::close_case_drain(&connection, &case.meta.id.0, 1000).unwrap();
    let case_root = case.case_root.clone();
    drop(connection);
    drop(case);
    drop(updated_shared);
    drop(shared);
    drop(plan);
    case_service::delete_case(&case_root).unwrap();
    assert!(!case_root.exists());
}

fn scratch_root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-workspaces");
    std::fs::create_dir_all(&root).unwrap();
    root
}
