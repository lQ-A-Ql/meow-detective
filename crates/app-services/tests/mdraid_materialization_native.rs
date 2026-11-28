use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use app_services::{case_service, cluster_service, file_service, source_db};
use domain::DataSourceKind;
use evidence_access::mdraid::MdRaidDiscoveryMember;
use persistence_source::repositories::file_repo::FileRepo;

#[test]
fn native_md_ext4_catalog_publishes_and_previews_through_file_identity() {
    exercise_native_catalog("mdraid/raid0-ext4", "member");
}

#[test]
fn native_multi_pv_lvm_md_ext4_catalog_publishes_and_previews_through_file_identity() {
    exercise_native_catalog("lvm-native/mdraid-ext4", "pv");
}

fn exercise_native_catalog(fixture: &str, member_prefix: &str) {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fixtures/public-small");
    let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-workspaces");
    std::fs::create_dir_all(&scratch).unwrap();
    let workspace = tempfile::tempdir_in(scratch).unwrap();
    let mut paths = Vec::new();
    for index in 0..2 {
        let path = workspace.path().join(format!("member-{index}.raw"));
        let compressed = File::open(
            fixtures
                .join(fixture)
                .join(format!("{member_prefix}-{index}.raw.gz")),
        )
        .unwrap();
        std::io::copy(
            &mut flate2::read::GzDecoder::new(compressed),
            &mut File::create(&path).unwrap(),
        )
        .unwrap();
        paths.push(path);
    }
    let kind = DataSourceKind::Raw;
    let members = paths
        .iter()
        .map(|path| MdRaidDiscoveryMember {
            source_path: path,
            source_kind: &kind,
        })
        .collect::<Vec<_>>();
    let plans = cluster_service::discover_linux_evidence_set_mdraid(&members).unwrap();
    let shared = evidence_access::datasource_service::ImageCollectionPlan::prepare(
        &paths
            .iter()
            .map(|path| {
                evidence_access::datasource_service::LvmDiscoverySource::new(path, kind.clone())
            })
            .collect::<Vec<_>>(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let case = case_service::create_case(workspace.path(), "Native MD filesystem", None).unwrap();
    let connection = case.connection().unwrap();
    let inputs = register_inputs(&connection, &case.meta.id, &plans[0], "physical");
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&connection)
        .insert(
            &case.meta.id,
            &domain::DataSource {
                id: domain::DataSourceId("unrelated-same-path".into()),
                name: "Independent acquisition".into(),
                kind: plans[0].source_content[0].source_kind.clone(),
                source_path: PathBuf::from(&plans[0].source_content[0].source_path),
                imported_at: chrono::Utc::now(),
                provenance: Default::default(),
            },
        )
        .unwrap();
    assert!(matches!(
        cluster_service::materialize_mdraid_array(
            &connection,
            &case.case_root,
            &case.meta.id,
            &plans[0],
            &[],
        ),
        Err(cluster_service::MdRaidPlanError::InvalidContentIdentity)
    ));
    let mut reversed_inputs = inputs.clone();
    reversed_inputs.reverse();
    assert!(matches!(
        cluster_service::materialize_mdraid_array(
            &connection,
            &case.case_root,
            &case.meta.id,
            &plans[0],
            &reversed_inputs,
        ),
        Err(cluster_service::MdRaidPlanError::InvalidContentIdentity)
    ));
    let repeated_inputs = vec![inputs[0].clone(); inputs.len()];
    assert!(matches!(
        cluster_service::materialize_mdraid_array(
            &connection,
            &case.case_root,
            &case.meta.id,
            &plans[0],
            &repeated_inputs,
        ),
        Err(cluster_service::MdRaidPlanError::InvalidContentIdentity)
    ));
    let mut foreign_inputs = inputs.clone();
    foreign_inputs[0].case_id = "foreign-case".into();
    assert!(matches!(
        cluster_service::materialize_mdraid_array(
            &connection,
            &case.case_root,
            &case.meta.id,
            &plans[0],
            &foreign_inputs,
        ),
        Err(cluster_service::MdRaidPlanError::InvalidContentIdentity)
    ));
    let source = cluster_service::materialize_mdraid_array(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plans[0],
        &inputs,
    )
    .unwrap();
    let repeated = cluster_service::materialize_mdraid_array(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plans[0],
        &inputs,
    )
    .unwrap();
    assert_eq!(source.id, repeated.id);
    let version =
        persistence_sqlite::repositories::storage_object_repo::StorageObjectRepo::new(&connection)
            .version_for_source(&case.meta.id.0, &source.id.0)
            .unwrap()
            .unwrap();
    assert_eq!(
        version.mapping_sha256,
        evidence_access::mdraid::mdraid_array_fingerprint(&plans[0]).unwrap()
    );
    assert_eq!(
        version.native_versions,
        vec![domain::structure::NativeVersion::Md {
            metadata_version: domain::structure::MdMetadataVersion::V1_2,
            events: plans[0].members[0].superblock.events,
        }]
    );
    let object =
        persistence_sqlite::repositories::storage_object_repo::StorageObjectRepo::new(&connection)
            .find(&version.storage_object_id)
            .unwrap()
            .unwrap();
    assert_eq!(object.object_kind, "md_raid");
    assert_eq!(
        version.acquisition_ids,
        persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(
            &connection
        )
        .acquisition_ids(&version.parent_source_revisions)
        .unwrap()
    );
    let view_attempt =
        persistence_sqlite::repositories::structure_view_attempt_repo::StructureViewAttemptRepo::new(
            &connection,
        )
        .find(&case.meta.id.0, &source.id.0)
        .unwrap()
        .unwrap();
    assert_eq!(view_attempt.input_fingerprint, version.mapping_sha256);
    assert_eq!(view_attempt.generation, "source.fjall");
    assert!(!view_attempt.attempt_id.is_empty());
    assert!(!view_attempt.owner_id.is_empty());
    assert_eq!(view_attempt.receipt.len(), 64);
    let graph = app_services::structure_analysis::get_structure_graph(
        &connection,
        &case.case_root,
        &app_services::structure_analysis::StructureGraphRequest {
            case_id: case.meta.id.clone(),
            source_ids: vec![source.id.0.clone()],
        },
    )
    .unwrap();
    assert!(graph
        .mappings
        .iter()
        .any(|mapping| { mapping.target_node.0 == format!("version:{}", version.id.0) }));
    assert!(!graph.recovery_relations.is_empty());
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.kind == domain::structure::StructureRelationKind::ContentDependency));
    let analysis_request = app_services::structure_analysis::StructureAnalysisRequest {
        case_id: case.meta.id.clone(),
        source_ids: vec![source.id.0.clone()],
        intent: domain::structure::ReadIntent::MetadataInspection,
        priority: domain::structure::ReadPriority::Interactive,
        consumers: vec!["native-structure-test".into()],
        attempt: Some(domain::structure::StructureAttemptRef {
            attempt_id: "structure-attempt".into(),
            owner: "native-structure-test".into(),
        }),
        required_features: Default::default(),
    };
    let mut unsupported = analysis_request.clone();
    unsupported
        .required_features
        .insert("unknown-feature".into());
    assert!(matches!(
        app_services::structure_analysis::plan_structure_analysis(
            &connection,
            &case.case_root,
            &unsupported,
        ),
        Err(app_services::structure_analysis::StructureAnalysisError::UnsupportedFeature { .. })
    ));
    let analysis = app_services::structure_analysis::plan_structure_analysis(
        &connection,
        &case.case_root,
        &analysis_request,
    )
    .unwrap();
    assert!(analysis
        .adapters
        .iter()
        .any(|adapter| adapter.id == app_services::structure_analysis::MDRAID_ADAPTER_ID));
    assert!(analysis.work_items.iter().any(|item| {
        item.adapter_id == app_services::structure_analysis::MDRAID_ADAPTER_ID
            && item
                .demand
                .as_ref()
                .and_then(|demand| demand.attempt.as_ref())
                .is_some_and(|attempt| attempt.attempt_id == "structure-attempt")
    }));
    assert!(analysis.work_items.iter().any(|item| {
        item.adapter_id == app_services::structure_analysis::FILESYSTEM_ADAPTER_ID
    }));
    let run = app_services::structure_analysis::run_structure_analysis(
        &connection,
        &case.case_root,
        &analysis_request,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(run.work_results.len(), run.plan.work_items.len());
    assert!(run.work_results.iter().all(|result| result.bytes_read > 0));
    let mut deep_request = analysis_request.clone();
    deep_request.intent = domain::structure::ReadIntent::ContentRead;
    let deep = app_services::structure_analysis::run_structure_analysis(
        &connection,
        &case.case_root,
        &deep_request,
        &AtomicBool::new(false),
    )
    .unwrap();
    let input_bytes = plans[0]
        .source_content
        .iter()
        .map(|content| content.logical_size)
        .sum::<u64>();
    let output_bytes = deep
        .work_results
        .iter()
        .map(|result| result.bytes_read)
        .sum::<u64>();
    assert_eq!(
        deep.verification_stats.verified_bytes,
        input_bytes + output_bytes
    );
    assert!(deep
        .work_results
        .iter()
        .all(|result| result.bytes_read == result.receipt.ranges.covered_bytes()));
    let import = registered_collection(&connection, &case.meta.id, &plans[0], "physical", &inputs);
    let from_collection = cluster_service::materialize_linux_evidence_set_mdraid(
        &connection,
        &case.case_root,
        &case.meta.id,
        &import,
        &shared,
    )
    .unwrap();
    assert_eq!(from_collection.len(), 1);
    assert_eq!(source.id, from_collection[0].id);
    let mut reordered_import = import.clone();
    reordered_import.members.reverse();
    assert_eq!(
        cluster_service::materialize_linux_evidence_set_mdraid(
            &connection,
            &case.case_root,
            &case.meta.id,
            &reordered_import,
            &shared,
        )
        .unwrap()[0]
            .id,
        source.id,
    );
    let independent_inputs = register_inputs(&connection, &case.meta.id, &plans[0], "independent");
    let independent_import = registered_collection(
        &connection,
        &case.meta.id,
        &plans[0],
        "independent",
        &independent_inputs,
    );
    let independent = cluster_service::materialize_linux_evidence_set_mdraid(
        &connection,
        &case.case_root,
        &case.meta.id,
        &independent_import,
        &shared,
    )
    .unwrap()
    .remove(0);
    assert_ne!(source.id, independent.id);
    assert_eq!(source.source_path, independent.source_path);
    let revisions = persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(
        &connection,
    );
    assert_eq!(
        revisions.inputs(&case.meta.id.0, &source.id.0).unwrap(),
        inputs
    );
    assert_eq!(
        revisions
            .inputs(&case.meta.id.0, &independent.id.0)
            .unwrap(),
        independent_inputs
    );
    assert_eq!(
        cluster_service::materialize_mdraid_array(
            &connection,
            &case.case_root,
            &case.meta.id,
            &plans[0],
            &independent_inputs,
        )
        .unwrap()
        .id,
        independent.id,
    );
    let mut invalid_import = import.clone();
    invalid_import.import_set_id = "unregistered-collection".into();
    assert!(matches!(
        cluster_service::materialize_linux_evidence_set_mdraid(
            &connection,
            &case.case_root,
            &case.meta.id,
            &invalid_import,
            &shared,
        ),
        Err(cluster_service::MdRaidPlanError::InvalidContentIdentity)
    ));
    assert!(matches!(
        cluster_service::materialize_linux_evidence_set_mdraid(
            &connection,
            &case.case_root,
            &domain::CaseId("foreign-case".into()),
            &import,
            &shared,
        ),
        Err(cluster_service::MdRaidPlanError::InvalidContentIdentity)
    ));
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
    assert_eq!(partitions.records.len(), 1);
    assert_eq!(partitions.records[0].partition_index, 1);
    assert_eq!(partitions.records[0].filesystem.as_deref(), Some("Ext4"));
    let file = FileRepo::new(&source_connection)
        .find_by_partition_and_path(&source.id, 1, "evidence.bin")
        .unwrap()
        .unwrap();
    let expected = std::fs::read(fixtures.join("mdraid/oracle.bin")).unwrap();
    let preview = file_service::FileHeaderReadCache::new(case.meta.id.0.clone())
        .read_file_header_by_id(&source_connection, &file.id, 1024)
        .unwrap();
    assert_eq!(preview, expected[..1024]);
    let independent_connection =
        source_db::open_source_db(&case.case_root, &independent.id).unwrap();
    let independent_file = FileRepo::new(&independent_connection)
        .find_by_partition_and_path(&independent.id, 1, "evidence.bin")
        .unwrap()
        .unwrap();
    assert_ne!(file.data_source_id, independent_file.data_source_id);
    assert_eq!(
        file_service::FileHeaderReadCache::new(case.meta.id.0.clone())
            .read_file_header_by_id(&independent_connection, &independent_file.id, 1024)
            .unwrap(),
        expected[..1024],
    );
    connection
        .execute(
            "UPDATE data_source_catalog_publications SET state='prepared', published_at=NULL
             WHERE data_source_id=?1",
            [&source.id.0],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE data_source_processing_phases SET state='running', completed_at=NULL,
         lease_expires_at='2000-01-01 00:00:00' WHERE data_source_id=?1 AND phase='catalog'",
            [&source.id.0],
        )
        .unwrap();
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&connection)
        .update_import_state(&source.id, "pending", None)
        .unwrap();
    cluster_service::materialize_mdraid_source_catalog(&connection, &case.case_root, &source.id)
        .unwrap();
    assert_eq!(
        persistence_sqlite::repositories::catalog_publication_repo::CatalogPublicationRepo::new(
            &connection,
        )
        .find(&source.id)
        .unwrap()
        .unwrap()
        .state,
        "published",
    );
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    assert!(matches!(
        cluster_service::materialize_mdraid_source_catalog_with_cancel(
            &connection,
            &case.case_root,
            &source.id,
            &cancelled,
        ),
        Err(cluster_service::MdRaidPlanError::Cancelled),
    ));
    app_services::source_processing::finalize_source_processing_with_cancel(
        &connection,
        &case.case_root,
        &case.meta.id,
        &source.id,
        std::sync::Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let processing = persistence_sqlite::repositories::processing_phase_repo::DataSourceProcessingPhaseRepo::new(&connection)
        .list_for_data_source(&source.id).unwrap();
    assert_eq!(processing.len(), 6);
    assert!(processing.iter().all(|phase| phase.state
        == persistence_sqlite::repositories::processing_phase_repo::ProcessingPhaseState::Ready));
    cluster_service::materialize_mdraid_source_catalog(&connection, &case.case_root, &source.id)
        .unwrap();
    let descriptor_bytes = std::fs::read(&source.source_path).unwrap();
    let original_modified = std::fs::metadata(&paths[0]).unwrap().modified().unwrap();
    let mut changed = std::fs::OpenOptions::new()
        .write(true)
        .open(&paths[0])
        .unwrap();
    changed.seek(SeekFrom::End(-1)).unwrap();
    changed.write_all(&[0x19]).unwrap();
    changed.sync_all().unwrap();
    changed
        .set_times(std::fs::FileTimes::new().set_modified(original_modified))
        .unwrap();
    drop(changed);
    assert_eq!(
        std::fs::metadata(&paths[0]).unwrap().modified().unwrap(),
        original_modified
    );
    plans[0].source_snapshot.validate().unwrap();
    assert!(matches!(
        evidence_access::mdraid::verify_mdraid_source_content(
            &plans[0],
            &std::sync::atomic::AtomicBool::new(false),
        ),
        Err(evidence_access::mdraid::MdRaidPlanError::SourceContentChanged),
    ));
    let updated = cluster_service::discover_linux_evidence_set_mdraid(&members).unwrap();
    let revisions = persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(
        &connection,
    );
    let old_inputs = revisions.inputs(&case.meta.id.0, &source.id.0).unwrap();
    let changed_parent = old_inputs
        .iter()
        .find(|input| {
            persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&connection)
                .find_by_case(&case.meta.id)
                .unwrap()
                .iter()
                .any(|parent| {
                    parent.id.0 == input.source_id
                        && parent.source_path.canonicalize().unwrap()
                            == paths[0].canonicalize().unwrap()
                })
        })
        .unwrap();
    let (_, mut content) = revisions
        .current(&case.meta.id.0, &changed_parent.source_id)
        .unwrap();
    content.logical_sha256 = Some(
        updated[0]
            .source_content
            .iter()
            .find(|content| {
                std::path::Path::new(&content.source_path) == paths[0].canonicalize().unwrap()
            })
            .unwrap()
            .logical_sha256
            .clone(),
    );
    revisions
        .bind(&case.meta.id.0, &changed_parent.source_id, &content, true)
        .unwrap();
    assert!(revisions
        .require_current_inputs(&case.meta.id.0, &source.id.0)
        .is_err());
    assert!(app_services::structure_analysis::plan_structure_analysis(
        &connection,
        &case.case_root,
        &analysis_request,
    )
    .is_err());
    revisions
        .require_current_inputs(&case.meta.id.0, &independent.id.0)
        .unwrap();
    assert!(
        file_service::FileHeaderReadCache::new(case.meta.id.0.clone())
            .read_file_header_by_id(&source_connection, &file.id, 1024)
            .is_err()
    );
    assert!(cluster_service::materialize_mdraid_array(
        &connection,
        &case.case_root,
        &case.meta.id,
        &updated[0],
        &inputs,
    )
    .is_err());
    let replacement_inputs = inputs
        .iter()
        .map(|input| {
            revisions
                .current(&case.meta.id.0, &input.source_id)
                .unwrap()
                .0
        })
        .collect::<Vec<_>>();
    let replacement = cluster_service::materialize_mdraid_array(
        &connection,
        &case.case_root,
        &case.meta.id,
        &updated[0],
        &replacement_inputs,
    )
    .unwrap();
    assert_ne!(source.id, replacement.id);
    assert_ne!(source.source_path, replacement.source_path);
    assert_eq!(
        std::fs::read(&source.source_path).unwrap(),
        descriptor_bytes
    );
    let object_repo =
        persistence_sqlite::repositories::storage_object_repo::StorageObjectRepo::new(&connection);
    let original_version = object_repo
        .version_for_source(&case.meta.id.0, &source.id.0)
        .unwrap()
        .unwrap();
    let replacement_version = object_repo
        .version_for_source(&case.meta.id.0, &replacement.id.0)
        .unwrap()
        .unwrap();
    assert_eq!(
        original_version.storage_object_id,
        replacement_version.storage_object_id
    );
    assert_ne!(original_version.id, replacement_version.id);
    assert_eq!(
        object_repo
            .versions_for_object(&case.meta.id.0, &original_version.storage_object_id)
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
    assert_eq!(
        output_revision.metadata_identity_status,
        domain::structure::DigestVerification::Recorded
    );
    assert_eq!(
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&connection)
            .find_by_case(&case.meta.id)
            .unwrap()
            .len(),
        8,
    );
}

fn register_inputs(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    plan: &evidence_access::mdraid::MdRaidArrayPlan,
    acquisition: &str,
) -> Vec<domain::structure::SourceRevisionRef> {
    let mut inputs = Vec::new();
    for (index, content) in plan.source_content.iter().enumerate() {
        let source = domain::DataSource {
            id: domain::DataSourceId(format!("{acquisition}-{index}")),
            name: format!("Physical {index}"),
            kind: content.source_kind.clone(),
            source_path: PathBuf::from(&content.source_path),
            imported_at: chrono::Utc::now(),
            provenance: Default::default(),
        };
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
            .insert(case_id, &source)
            .unwrap();
        let reference =
            persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(conn)
                .bind(
                    &case_id.0,
                    &source.id.0,
                    &domain::structure::SourceRevisionContent::captured(
                        domain::structure::AcquisitionId(acquisition.to_owned()),
                        content.logical_size,
                        None,
                        content.logical_sha256.clone(),
                    ),
                    true,
                )
                .unwrap();
        let root = std::path::PathBuf::from(conn.path().unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        let control = source_db::open_source_db(&root, &source.id).unwrap();
        evidence_access::source_runtime::content::persist_registered_content(
            &control, &reference, content,
        )
        .unwrap();
        inputs.push(reference);
    }
    inputs
}

fn registered_collection(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    plan: &evidence_access::mdraid::MdRaidArrayPlan,
    acquisition: &str,
    inputs: &[domain::structure::SourceRevisionRef],
) -> cluster_service::LinuxEvidenceSetImportPlan {
    let import = cluster_service::LinuxEvidenceSetImportPlan {
        import_set_id: format!("registered-native-md-{acquisition}"),
        import_set_name: "Native MD".into(),
        root_path: PathBuf::new(),
        manifest_rel_path: String::new(),
        members: plan
            .source_content
            .iter()
            .enumerate()
            .map(
                |(index, content)| cluster_service::LinuxEvidenceSetMemberPlan {
                    member_index: index as u32,
                    source_path: PathBuf::from(&content.source_path),
                    source_name: format!("Physical {index}"),
                    source_kind: content.source_kind.clone(),
                },
            )
            .collect(),
        skipped_entries: Vec::new(),
    };
    cluster_service::register_linux_evidence_set_import(conn, case_id, &import).unwrap();
    let repo =
        persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetRepo::new(conn);
    for member in &import.members {
        repo.bind_member_source(
            &import.import_set_id,
            member.member_index,
            &inputs[member.member_index as usize].source_id,
        )
        .unwrap();
    }
    import
}
