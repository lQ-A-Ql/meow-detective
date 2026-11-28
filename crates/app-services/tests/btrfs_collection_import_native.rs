use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use app_services::{
    case_service, cluster_service, file_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_with_counts, ImportJobOptions},
    source_db,
};
use domain::structure::{ReadIntent, ReadPriority};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, job_repo::JobRepo, source_revision_repo::SourceRevisionRepo,
    storage_object_repo::StorageObjectRepo,
};
use sha2::{Digest, Sha256};

fn exercise(profile: &str, members: usize) {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/btrfs-pool");
    let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-workspaces");
    std::fs::create_dir_all(&scratch).unwrap();
    let workspace = tempfile::tempdir_in(scratch).unwrap();
    let images = workspace.path().join("images");
    std::fs::create_dir(&images).unwrap();
    for index in 0..members {
        std::io::copy(
            &mut flate2::read::GzDecoder::new(
                File::open(
                    fixtures
                        .join(profile)
                        .join(format!("member-{index}.raw.gz")),
                )
                .unwrap(),
            ),
            &mut File::create(images.join(format!("member-{index}.raw"))).unwrap(),
        )
        .unwrap();
    }
    let import =
        cluster_service::plan_linux_evidence_set_import(&images, Some(profile.into())).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let collection = import.prepare_shared_sources(&cancel).unwrap();
    assert_eq!(collection.btrfs_pools().len(), 1);
    assert_eq!(collection.btrfs_pools()[0].members.len(), members);
    let configs = import
        .member_import_configs_from_plan(collection.clone())
        .unwrap();
    let case = case_service::create_case(workspace.path(), "Btrfs device pool", None).unwrap();
    let connection = case.connection().unwrap();
    cluster_service::register_linux_evidence_set_import(&connection, &case.meta.id, &import)
        .unwrap();
    cluster_service::write_linux_evidence_set_manifest(&case.case_root, &import).unwrap();
    for config in configs.into_iter().rev() {
        let job = JobRepo::new(&connection)
            .create(&case.meta.id.0, "Btrfs physical member")
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
        &import,
        &collection,
        &cancel,
    )
    .unwrap();
    let physical = DataSourceRepo::new(&connection)
        .find_by_case(&case.meta.id)
        .unwrap();
    assert_eq!(physical.len(), members);
    for source in &physical {
        let (reference, _) = SourceRevisionRepo::new(&connection)
            .current(&case.meta.id.0, &source.id.0)
            .unwrap();
        assert!(persistence_sqlite::repositories::structure_observation_repo::StructureObservationRepo::new(&connection)
            .count_published(&reference).unwrap() > 0);
        let source_connection = source_db::open_source_db(&case.case_root, &source.id).unwrap();
        assert!(FileRepo::new(&source_connection)
            .find_by_partition_and_path(&source.id, 0, "evidence.bin")
            .unwrap()
            .is_none());
    }
    let sources = cluster_service::materialize_linux_evidence_set_btrfs(
        &connection,
        &case.case_root,
        &case.meta.id,
        &import,
        &collection,
        &cancel,
    )
    .unwrap();
    assert_eq!(sources.len(), 1);
    let source = &sources[0];
    assert_eq!(source.kind, domain::DataSourceKind::BtrfsPool);
    let repeated = cluster_service::materialize_linux_evidence_set_btrfs(
        &connection,
        &case.case_root,
        &case.meta.id,
        &import,
        &collection,
        &cancel,
    )
    .unwrap();
    assert_eq!(source.id, repeated[0].id);
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_by_case(&case.meta.id)
            .unwrap()
            .len(),
        members + 1
    );
    let version = StorageObjectRepo::new(&connection)
        .version_for_source(&case.meta.id.0, &source.id.0)
        .unwrap()
        .unwrap();
    assert!(matches!(
        version.native_versions[0],
        domain::structure::NativeVersion::Btrfs { .. }
    ));
    assert_eq!(version.parent_source_revisions.len(), members);
    connection.execute(
        "UPDATE data_source_catalog_publications SET state='prepared', published_at=NULL WHERE data_source_id=?1",
        [&source.id.0]).unwrap();
    connection
        .execute(
            "UPDATE data_source_processing_phases SET state='running', completed_at=NULL,
         lease_expires_at='2000-01-01 00:00:00' WHERE data_source_id=?1 AND phase='catalog'",
            [&source.id.0],
        )
        .unwrap();
    DataSourceRepo::new(&connection)
        .update_import_state(&source.id, "pending", None)
        .unwrap();
    assert_eq!(
        cluster_service::materialize_linux_evidence_set_btrfs(
            &connection,
            &case.case_root,
            &case.meta.id,
            &import,
            &collection,
            &cancel
        )
        .unwrap()[0]
            .id,
        source.id
    );
    let source_connection =
        source_db::open_registered_source_db(&connection, &case.case_root, &source.id).unwrap();
    let file = FileRepo::new(&source_connection)
        .find_by_partition_and_path(&source.id, 0, "evidence.bin")
        .unwrap()
        .unwrap();
    let expected = std::fs::read(fixtures.join("oracle.bin")).unwrap();
    let cache = file_service::FileHeaderReadCache::new(case.meta.id.0.clone());
    assert_eq!(
        cache
            .read_file_header_by_id(&source_connection, &file.id, 257)
            .unwrap(),
        expected[..257]
    );
    let mut context = file_service::SourceReadContext::new(
        &source_connection,
        &connection,
        &case.case_root,
        &case.meta.id,
        &source.id,
    );
    let mut stream = match context.open_file_range_by_id(&file.id).unwrap() {
        file_service::RangeContentReader::Seekable(reader) => reader,
        _ => panic!("Btrfs file should provide bounded seekable content"),
    };
    let mut actual = Vec::new();
    stream.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, expected);
    let request = app_services::structure_analysis::StructureAnalysisRequest {
        case_id: case.meta.id.clone(),
        source_ids: vec![source.id.0.clone()],
        intent: ReadIntent::ContentRead,
        priority: ReadPriority::Interactive,
        consumers: vec!["btrfs-native-acceptance".into()],
        attempt: None,
        required_features: Default::default(),
    };
    let run = app_services::structure_analysis::run_structure_analysis(
        &connection,
        &case.case_root,
        &request,
        &cancel,
    )
    .unwrap();
    let proof = run
        .work_results
        .iter()
        .find(|result| result.receipt.target_fingerprint == format!("btrfs-file-v1:{}", file.id.0))
        .unwrap();
    assert_eq!(proof.bytes_read, expected.len() as u64);
    assert_eq!(
        proof.receipt.output_sha256,
        hex::encode(Sha256::digest(&expected))
    );
    assert_eq!(
        run.verification_stats.verified_bytes,
        members as u64 * 268435456
            + run
                .work_results
                .iter()
                .map(|result| result.bytes_read)
                .sum::<u64>()
    );
    assert!(run
        .plan
        .graph
        .edges
        .iter()
        .any(|edge| edge.kind == domain::structure::StructureRelationKind::ContentDependency));
    let queried = app_services::structure_analysis::get_structure_graph(
        &connection,
        &case.case_root,
        &app_services::structure_analysis::StructureGraphRequest {
            case_id: case.meta.id.clone(),
            source_ids: vec![source.id.0.clone()],
        },
    )
    .unwrap();
    assert!(queried
        .nodes
        .iter()
        .any(|node| node.id.0 == format!("version:{}", version.id.0)));
    let mut unsupported = request.clone();
    unsupported.required_features.insert("raid6".into());
    assert!(app_services::structure_analysis::plan_structure_analysis(
        &connection,
        &case.case_root,
        &unsupported
    )
    .is_err());
    let plan = &collection.btrfs_pools()[0];
    let inputs = SourceRevisionRepo::new(&connection)
        .inputs(&case.meta.id.0, &source.id.0)
        .unwrap();
    assert!(cluster_service::materialize_btrfs_pool(
        &connection,
        &case.case_root,
        &case.meta.id,
        plan,
        &inputs,
        &AtomicBool::new(true)
    )
    .is_err());
    let mut foreign = inputs.clone();
    foreign[0].case_id = uuid::Uuid::new_v4().to_string();
    assert!(cluster_service::materialize_btrfs_pool(
        &connection,
        &case.case_root,
        &case.meta.id,
        plan,
        &foreign,
        &cancel
    )
    .is_err());
    let original_publication =
        persistence_sqlite::repositories::catalog_publication_repo::CatalogPublicationRepo::new(
            &connection,
        )
        .find(&source.id)
        .unwrap()
        .unwrap();
    connection
        .execute(
            "UPDATE data_source_catalog_publications SET catalog_digest=?1 WHERE data_source_id=?2",
            rusqlite::params!["f".repeat(64), source.id.0],
        )
        .unwrap();
    assert!(cluster_service::materialize_btrfs_pool(
        &connection,
        &case.case_root,
        &case.meta.id,
        plan,
        &inputs,
        &cancel
    )
    .is_err());
    connection
        .execute(
            "UPDATE data_source_catalog_publications SET catalog_digest=?1 WHERE data_source_id=?2",
            rusqlite::params![original_publication.catalog_digest, source.id.0],
        )
        .unwrap();
    assert_eq!(stream.read(&mut []).unwrap(), 0);
    let revisions = SourceRevisionRepo::new(&connection);
    let (_, mut content) = revisions
        .current(&case.meta.id.0, &inputs[0].source_id)
        .unwrap();
    content.metadata_identity = Some(uuid::Uuid::new_v4().to_string());
    revisions
        .bind(&case.meta.id.0, &inputs[0].source_id, &content, true)
        .unwrap();
    assert!(stream.read(&mut []).is_err());
    assert!(cache
        .read_file_header_by_id(&source_connection, &file.id, 16)
        .is_err());
    assert!(app_services::structure_analysis::plan_structure_analysis(
        &connection,
        &case.case_root,
        &request
    )
    .is_err());
    drop(stream);
    drop(context);
    drop(cache);
    drop(source_connection);
    case_service::delete_data_source_in(&connection, &case.case_root, &source.id.0).unwrap();
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_by_case(&case.meta.id)
            .unwrap()
            .len(),
        members
    );
    assert!(source.source_path.exists());
    for parent in physical {
        assert!(parent.source_path.exists());
    }
}

#[test]
fn native_single_pool_import_publishes_only_its_logical_catalog() {
    exercise("single", 2);
}

#[test]
fn native_raid0_pool_import_publishes_only_its_logical_catalog() {
    exercise("raid0", 2);
}

#[test]
fn native_raid1_pool_import_publishes_only_its_logical_catalog() {
    exercise("raid1", 2);
}

#[test]
fn native_raid10_pool_import_publishes_only_its_logical_catalog() {
    exercise("raid10", 4);
}
