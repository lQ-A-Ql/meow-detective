use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;

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
    let workspace = tempfile::tempdir().unwrap();
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
    register_inputs(&connection, &case.meta.id, &shared, &plans[0]);
    let source = cluster_service::materialize_mdraid_array(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plans[0],
    )
    .unwrap();
    let repeated = cluster_service::materialize_mdraid_array(
        &connection,
        &case.case_root,
        &case.meta.id,
        &plans[0],
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
    let object =
        persistence_sqlite::repositories::storage_object_repo::StorageObjectRepo::new(&connection)
            .find(&version.storage_object_id)
            .unwrap()
            .unwrap();
    assert_eq!(object.object_kind, "md_raid");
    let from_collection = cluster_service::materialize_linux_evidence_set_mdraid(
        &connection,
        &case.case_root,
        &case.meta.id,
        &shared,
    )
    .unwrap();
    assert_eq!(from_collection.len(), 1);
    assert_eq!(source.id, from_collection[0].id);
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
    assert!(cluster_service::materialize_mdraid_source_catalog(
        &connection,
        &case.case_root,
        &source.id,
    )
    .is_err());
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
    let facts = revisions.observations(changed_parent).unwrap();
    revisions
        .bind(
            &case.meta.id.0,
            &changed_parent.source_id,
            &content,
            &facts,
            true,
        )
        .unwrap();
    assert!(revisions
        .require_current_inputs(&case.meta.id.0, &source.id.0)
        .is_err());
    assert!(
        file_service::FileHeaderReadCache::new(case.meta.id.0.clone())
            .read_file_header_by_id(&source_connection, &file.id, 1024)
            .is_err()
    );
    let replacement = cluster_service::materialize_mdraid_array(
        &connection,
        &case.case_root,
        &case.meta.id,
        &updated[0],
    )
    .unwrap();
    assert_ne!(source.id, replacement.id);
    assert_ne!(source.source_path, replacement.source_path);
    assert_eq!(
        std::fs::read(&source.source_path).unwrap(),
        descriptor_bytes
    );
    assert_eq!(
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&connection)
            .find_by_case(&case.meta.id)
            .unwrap()
            .len(),
        4,
    );
}

fn register_inputs(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    shared: &evidence_access::datasource_service::ImageCollectionPlan,
    plan: &evidence_access::mdraid::MdRaidArrayPlan,
) {
    for (index, content) in plan.source_content.iter().enumerate() {
        let source = domain::DataSource {
            id: domain::DataSourceId(format!("physical-{index}")),
            name: format!("Physical {index}"),
            kind: content.source_kind.clone(),
            source_path: PathBuf::from(&content.source_path),
            imported_at: chrono::Utc::now(),
            provenance: Default::default(),
        };
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
            .insert(case_id, &source)
            .unwrap();
        persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(conn)
            .bind(
                &case_id.0,
                &source.id.0,
                &domain::structure::SourceRevisionContent {
                    logical_size: content.logical_size,
                    metadata_identity: None,
                    logical_sha256: Some(content.logical_sha256.clone()),
                },
                shared
                    .native_observations(&source.source_path, &source.kind)
                    .unwrap(),
                true,
            )
            .unwrap();
    }
}
