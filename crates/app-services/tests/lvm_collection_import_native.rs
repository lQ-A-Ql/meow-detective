use std::{
    fs::File,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use app_services::{
    case_service, cluster_service, file_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_with_counts, ImportJobOptions},
    source_db,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};

#[test]
fn native_collection_import_publishes_one_cross_pv_catalog_and_previews_the_file() {
    let workspace = tempfile::tempdir().unwrap();
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
    let configs = plan.member_import_configs(&cancel).unwrap();
    assert!(Arc::ptr_eq(
        configs[0].collection_plan.as_ref().unwrap(),
        configs[1].collection_plan.as_ref().unwrap()
    ));
    let shared = configs[0].collection_plan.as_ref().unwrap().clone();
    let case = case_service::create_case(workspace.path(), "Shared native LVM", None).unwrap();
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
    assert!(cluster_service::validate_collection_publication(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plan,
        &shared,
        &AtomicBool::new(true)
    )
    .is_err());
    let sources = DataSourceRepo::new(&connection)
        .find_by_case(&case.meta.id)
        .unwrap();
    assert_eq!(sources.len(), 2);
    let mut evidence_files = 0;
    let mut logical_views = 0;
    let mut committed_metadata_sources = 0;
    let mut analysis_target = None;
    for source in sources {
        let revisions =
            persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(
                &connection,
            );
        let reference = domain::structure::SourceRevisionRef {
            case_id: case.meta.id.0.clone(),
            source_id: source.id.0.clone(),
            revision: 1,
        };
        revisions.require_current(&reference).unwrap();
        let facts = evidence_access::structure_observation::read_source_observations(
            &connection,
            &case.case_root,
            &reference,
        )
        .unwrap();
        assert!(facts
            .iter()
            .any(|fact| matches!(fact.version, domain::structure::NativeVersion::LvmLabel)));
        if facts.iter().any(|fact| {
            matches!(
                fact.version,
                domain::structure::NativeVersion::LvmCommitted { .. }
            )
        }) {
            committed_metadata_sources += 1;
        }
        let source_connection = source_db::open_source_db(&case.case_root, &source.id).unwrap();
        let native =
            source_db::open_registered_fjall_source_read_only(&case.case_root, &source.id).unwrap();
        let partitions = native
            .partition_list_by_source(
                &native.snapshot(),
                &source.id.0,
                None,
                persistence_source::fjall::ScanDirection::Forward,
                10,
            )
            .unwrap();
        for partition in &partitions.records {
            if partition.filesystem.as_deref() == Some("Ext4") {
                logical_views += 1;
                let file = FileRepo::new(&source_connection)
                    .find_by_partition_and_path(
                        &source.id,
                        usize::try_from(partition.partition_index).unwrap(),
                        "evidence.bin",
                    )
                    .unwrap()
                    .unwrap();
                evidence_files += 1;
                let preview = file_service::FileHeaderReadCache::new(case.meta.id.0.clone())
                    .read_file_header_by_id(&source_connection, &file.id, 1024)
                    .unwrap();
                assert_eq!(
                    preview,
                    std::fs::read(fixtures.join("mdraid/oracle.bin")).unwrap()[..1024]
                );
                let verified_plan = shared.clone();
                let resolver = file_service::BoundEvidenceReaderResolver::new(move |path, kind| {
                    let kind = match kind {
                        "raw" => domain::DataSourceKind::Raw,
                        "e01" => domain::DataSourceKind::E01,
                        _ => return Err(std::io::Error::other("Unexpected source kind")),
                    };
                    verified_plan
                        .verified_factory(path, &kind, &AtomicBool::new(false))
                        .map_err(std::io::Error::other)?
                        .open_reader()
                });
                let bound = file_service::FileHeaderReadCache::with_bound_readers(
                    case.meta.id.0.clone(),
                    source.id.clone(),
                    resolver,
                );
                assert_eq!(
                    bound
                        .read_file_header_by_id(&source_connection, &file.id, 1024)
                        .unwrap(),
                    preview
                );
                analysis_target = Some((source.clone(), file.clone()));
                assert_eq!(
                    file_service::read_file_header_by_id(&source_connection, &file.id, 1024)
                        .unwrap(),
                    preview,
                );
                let mut stream =
                    file_service::open_file_content_by_id(&source_connection, &file.id).unwrap();
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut stream, &mut bytes).unwrap();
                assert_eq!(
                    bytes,
                    std::fs::read(fixtures.join("mdraid/oracle.bin")).unwrap()
                );
                let original = file_service::preview_partition_candidate_from_record(partition);
                for changed_vg in [true, false] {
                    let mut candidate = original.clone();
                    let identity = candidate.lvm_identity.as_mut().unwrap();
                    if changed_vg {
                        identity.vg_uuid = "different-vg-uuid".into();
                    } else {
                        identity.lv_uuid = "different-lv-uuid".into();
                    }
                    assert!(file_service::open_candidate_block_reader_with_lvm_cache(
                        &source.source_path,
                        &candidate,
                        &mut |path| evidence_access::datasource_service::open_evidence_reader(
                            path,
                            &domain::DataSourceKind::Raw
                        ),
                        &mut file_service::LvmPoolRequestCache::new(),
                    )
                    .is_err());
                }
            }
        }
        assert_eq!(
            DataSourceRepo::new(&connection)
                .find_storage(&source.id)
                .unwrap()
                .unwrap()
                .import_state,
            "ready"
        );
    }
    let objects =
        persistence_sqlite::repositories::storage_object_repo::StorageObjectRepo::new(&connection);
    let vg_id: String = connection
        .query_row(
            "SELECT id FROM storage_objects
             WHERE case_id=?1 AND object_kind='volume_group'",
            [&case.meta.id.0],
            |row| row.get(0),
        )
        .unwrap();
    let vg_versions = objects
        .versions_for_object(&case.meta.id.0, &vg_id)
        .unwrap();
    assert_eq!(vg_versions.len(), 1);
    assert!(vg_versions.iter().all(|version| {
        version.version_kind == domain::structure::StorageObjectVersionKind::NativeObservation
            && version.consistency == domain::structure::RevisionConsistency::Coherent
    }));
    assert_eq!(
        objects.find(&vg_id).unwrap().unwrap().identity_state,
        "proven"
    );
    assert_eq!(logical_views, 1);
    assert_eq!(evidence_files, 1);
    assert_eq!(committed_metadata_sources, 1);
    let source = DataSourceRepo::new(&connection)
        .find_by_case(&case.meta.id)
        .unwrap()
        .remove(0);
    let source_conn = source_db::open_source_db(&case.case_root, &source.id).unwrap();
    let key =
        persistence_sqlite::repositories::source_meta_repo::COLLECTION_MEMBER_VERIFICATION_KEY;
    let metadata =
        persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(&source_conn);
    let original = metadata.read(key).unwrap().unwrap();
    let mut record: domain::structure::CollectionMemberVerification =
        serde_json::from_str(&original).unwrap();
    record.source.revision += 1;
    metadata
        .write(key, &serde_json::to_string(&record).unwrap())
        .unwrap();
    assert!(cluster_service::validate_collection_publication(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plan,
        &shared,
        &cancel
    )
    .is_err());
    metadata.write(key, &original).unwrap();
    cluster_service::validate_collection_publication(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plan,
        &shared,
        &cancel,
    )
    .unwrap();
    let payload = std::fs::read(fixtures.join("mdraid/oracle.bin")).unwrap();
    let mut changed_payload = false;
    for member in &plan.members {
        let bytes = std::fs::read(&member.source_path).unwrap();
        if let Some(offset) = bytes.windows(64).position(|bytes| bytes == &payload[..64]) {
            use std::io::{Seek, SeekFrom, Write};
            let modified = member.source_path.metadata().unwrap().modified().unwrap();
            let mut writer = std::fs::OpenOptions::new()
                .write(true)
                .open(&member.source_path)
                .unwrap();
            writer.seek(SeekFrom::Start(offset as u64)).unwrap();
            writer.write_all(&[payload[0] ^ 0x80]).unwrap();
            writer.sync_all().unwrap();
            writer
                .set_times(std::fs::FileTimes::new().set_modified(modified))
                .unwrap();
            changed_payload = true;
            break;
        }
    }
    assert!(changed_payload);
    let (target_source, target_file) = analysis_target.unwrap();
    let target_conn = source_db::open_source_db(&case.case_root, &target_source.id).unwrap();
    let verified_plan = shared.clone();
    let resolver = file_service::BoundEvidenceReaderResolver::new(move |path, _| {
        verified_plan
            .verified_factory(path, &domain::DataSourceKind::Raw, &AtomicBool::new(false))
            .map_err(std::io::Error::other)?
            .open_reader()
    });
    let bound = file_service::FileHeaderReadCache::with_bound_readers(
        case.meta.id.0.clone(),
        target_source.id,
        resolver,
    );
    assert!(bound
        .read_file_header_by_id(&target_conn, &target_file.id, 1024)
        .is_err());
    assert!(cluster_service::validate_collection_publication(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plan,
        &shared,
        &cancel
    )
    .is_err());
}
