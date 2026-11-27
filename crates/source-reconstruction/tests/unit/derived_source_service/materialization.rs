use std::path::PathBuf;

use super::*;
use crate::derived_source_service::catalog_manifest::persist_current_source_manifest;
use domain::structure::{AcquisitionId, SourceRevisionContent};
use domain::{
    DataSourceKind, DataSourcePlatform, DataSourceProvenance, EntryType, FileEntry, FileEntryId,
};
use persistence_sqlite::repositories::catalog_publication_repo::{
    seal_for, CatalogPublicationRepo,
};
use persistence_sqlite::repositories::ceph_rbd_lineage_repo::{
    CephRbdLineageRepo, CephRbdReplicaRecord,
};
use persistence_sqlite::repositories::datasource_repo::DataSourceStorage;
use persistence_sqlite::repositories::processing_phase_repo::{
    DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseState,
};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;

use super::registration::lineage_aggregate;

const SOURCE_ID: &str = "rbd-atomic-publish";
const FINGERPRINT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CEPH_SCOPE_ID: &str = "scope:ceph:atomic-publish";
const PARENT_SOURCE_IDS: [&str; 3] = ["source-osd-0", "source-osd-1", "source-osd-2"];

fn setup_case_db() -> rusqlite::Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open case database");
    persistence_sqlite::runner::run_all(&connection).expect("run case migrations");
    connection
        .execute(
            "INSERT INTO cases (id, name) VALUES ('case-1', 'Atomic Catalog Publish')",
            [],
        )
        .expect("insert case");
    connection
        .execute(
            "INSERT INTO data_sources (
                id, case_id, name, kind, source_path, storage_model,
                source_db_rel_path, platform, import_state
             ) VALUES (
                ?1, 'case-1', 'VM disk', 'ceph_rbd',
                'ceph-rbd://cluster/image', 'source_db',
                'sources/rbd-atomic-publish/source.db', 'linux', 'pending'
             )",
            [SOURCE_ID],
        )
        .expect("insert derived source");
    connection
}

fn data_source() -> DataSource {
    DataSource {
        id: DataSourceId(SOURCE_ID.to_string()),
        name: "VM disk".to_string(),
        kind: DataSourceKind::CephRbd,
        source_path: PathBuf::from("ceph-rbd://cluster/image"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::default(),
    }
}

fn summary(source: DataSource) -> MaterializedRbdSource {
    MaterializedRbdSource {
        data_source: source,
        file_count: 42,
        directory_count: 7,
        total_size: 4096,
        created_count: 40,
        modified_count: 41,
        accessed_count: 39,
        changed_count: 38,
        catalog_digest: "b".repeat(64),
    }
}

fn complete_catalog_for_test(
    connection: &rusqlite::Connection,
    source: &DataSource,
    fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
    summary: &MaterializedRbdSource,
) -> DerivedSourceResult<()> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    crate::derived_source_service::finalizer::complete_catalog_phase(
        &transaction,
        &source.id,
        fingerprint,
        attempt,
        summary,
    )?;
    crate::derived_source_service::finalizer::queue_post_catalog_phases(
        &transaction,
        &source.id,
        fingerprint,
    )?;
    DataSourceRepo::new(&transaction).update_import_state(&source.id, "ready", None)?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(())
}

fn descriptor() -> RbdImageDescriptor {
    RbdImageDescriptor {
        metadata: ceph_wire::RbdImageMetadata {
            name: "VM disk".to_string(),
            id: "atomic-publish-image".to_string(),
            object_prefix: "rbd_data.atomic_publish".to_string(),
            image_size: 4096,
            order: 12,
            features: 0,
            stripe_unit: 0,
            stripe_count: 0,
            data_pool_id: 2,
        },
        scope_identity: "scope-atomic-publish".to_string(),
        context: ceph_reconstruction::ceph_reconstruction::RbdReadContext {
            operation_features: 0,
            has_parent: false,
            snapshot_id: None,
            encrypted: false,
        },
    }
}

fn setup_recoverable_catalog(
    case_root: &std::path::Path,
) -> (
    rusqlite::Connection,
    DataSource,
    String,
    MaterializedRbdSource,
) {
    std::fs::create_dir_all(case_root).expect("create case root");
    let connection =
        persistence_sqlite::open_or_create(&case_root.join("app.db")).expect("open case database");
    persistence_sqlite::runner::run_all(&connection).expect("run case migrations");
    connection
        .execute(
            "INSERT INTO cases (id, name) VALUES ('case-1', 'Atomic Catalog Recovery')",
            [],
        )
        .expect("insert case");
    connection
        .execute(
            "INSERT INTO linux_topology_scopes (
            id, case_id, scope_kind, name, identity_state, status,
            evidence_completeness, diagnostics_json
         ) VALUES (?1, 'case-1', 'ceph', 'Ceph', 'unproven', 'ready', 'complete', '[]')",
            [CEPH_SCOPE_ID],
        )
        .expect("insert Ceph scope");
    for source_id in PARENT_SOURCE_IDS {
        connection
            .execute(
                "INSERT INTO data_sources (
                    id, case_id, name, kind, source_path, platform, import_state
                 ) VALUES (
                    ?1, 'case-1', ?1, 'e01', '', 'linux', 'ready'
                 )",
                [source_id],
            )
            .expect("insert parent source");
        connection
            .execute(
                "INSERT INTO linux_topology_memberships (
                scope_id, data_source_id, role, member_index, confidence, provenance_json
             ) VALUES (?1, ?2, 'storage_node', 0, 'candidate', '{}')",
                [CEPH_SCOPE_ID, source_id],
            )
            .expect("bind parent source to Ceph scope");
    }

    let source = data_source();
    let storage = DataSourceStorage::source_db(
        &source.id.0,
        Some(DataSourcePlatform::Linux.as_storage_str()),
        Some("vm_disk".to_string()),
    );
    DataSourceRepo::new(&connection)
        .insert_with_storage(&CaseId("case-1".to_string()), &source, &storage)
        .expect("insert derived source");
    connection.execute(
        "INSERT INTO storage_objects (id, case_id, object_kind, name, identity_state, status)
         VALUES ('storage:rbd:rbd-atomic-publish', 'case-1', 'ceph_rbd', 'VM disk', 'candidate', 'ready')",
        [],
    ).expect("insert derived storage object");
    let replicas = PARENT_SOURCE_IDS
        .iter()
        .enumerate()
        .map(|(ordinal, source_id)| CephRbdReplicaRecord {
            ordinal: ordinal as u32,
            source_data_source_id: (*source_id).to_string(),
            inventory_id: format!("inventory-osd-{ordinal}"),
            osd_id: ordinal as u32,
        })
        .collect::<Vec<_>>();
    CephRbdLineageRepo::new(&connection)
        .insert_aggregate(&lineage_aggregate(
            &source.id,
            CEPH_SCOPE_ID,
            &descriptor(),
            &replicas,
            &ceph_reconstruction::ceph_reconstruction::RbdReplicaPolicy::strict_legacy(),
        ))
        .expect("insert derived lineage");
    let revisions = SourceRevisionRepo::new(&connection);
    let inputs = PARENT_SOURCE_IDS
        .iter()
        .enumerate()
        .map(|(index, source_id)| {
            revisions
                .bind(
                    "case-1",
                    source_id,
                    &SourceRevisionContent::captured(
                        AcquisitionId(format!("fixture:{source_id}")),
                        1,
                        None,
                        format!("{:x}", index + 1).repeat(64),
                    ),
                    true,
                )
                .expect("bind parent revision")
        })
        .collect::<Vec<_>>();
    revisions
        .bind_inputs("case-1", &source.id.0, &inputs)
        .expect("bind RBD parent inputs");
    let fingerprint =
        load_lineage_fingerprint(&connection, &source.id).expect("load lineage fingerprint");
    revisions
        .bind(
            "case-1",
            &source.id.0,
            &SourceRevisionContent::derived(
                AcquisitionId("fixture:rbd".to_string()),
                4096,
                super::super::catalog_manifest::catalog_fingerprint_for_source(&fingerprint),
            ),
            true,
        )
        .expect("bind RBD derived revision");
    let attempt_id = "66666666-6666-4666-8666-666666666666";
    let source_connection =
        source_kernel::source_db::open_fresh_source_build_db(case_root, &source.id, attempt_id)
            .expect("open source database");
    DataSourceRepo::new(&source_connection)
        .upsert_source_local_metadata(&CaseId("case-1".to_string()), &source)
        .expect("insert source-local metadata");
    persistence_source::repositories::file_repo::FileRepo::new(&source_connection)
        .insert_batch(&catalog_rows(&source))
        .expect("insert source catalog");
    persistence_source::detail::set_partition_index(&source_connection, "root", 0).unwrap();
    persistence_source::detail::set_partition_index(&source_connection, "file", 0).unwrap();
    let fjall_source =
        source_kernel::source_db::open_fjall_source_for_build(&source_connection, &source.id)
            .expect("open Fjall source storage");
    fjall_source
        .partition_replace_by_source(
            &source.id.0,
            &[persistence_source::fjall::PartitionRecord {
                id: "partition-0".to_owned(),
                data_source_id: source.id.0.clone(),
                partition_index: 0,
                name: "root".to_owned(),
                kind_label: "Linux filesystem".to_owned(),
                status: "supported".to_owned(),
                type_guid: None,
                offset: 0,
                length: 4096,
                filesystem: Some("XFS".to_owned()),
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
            persistence_source::fjall::PersistMode::Buffer,
        )
        .expect("persist Fjall partition");
    fjall_source
        .partition_projection_publish(&source.id.0)
        .expect("publish Fjall partition projection");
    let persisted_summary = super::super::catalog_manifest::summarize_source_connection(
        &source_connection,
        source.clone(),
    )
    .expect("summarize source catalog");
    persist_current_source_manifest(
        &source_connection,
        &fjall_source,
        &fingerprint,
        &persisted_summary,
    )
    .expect("persist catalog manifest");
    drop(fjall_source);
    let catalog_fingerprint =
        super::super::catalog_manifest::catalog_fingerprint_for_source(&fingerprint);
    let publication_digest =
        persistence_source::publication::catalog_publication::seal_catalog_generation(
            &source_connection,
            &source.id,
            attempt_id,
            &catalog_fingerprint,
            persistence_source::publication::catalog_publication::CatalogProjection::Partition,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .expect("seal fixture generation");
    source_kernel::source_db::finalize_source_build_db(&source_connection)
        .expect("finalize fixture source database");
    drop(source_connection);
    source_kernel::source_db::publish_source_build_db(case_root, &source.id, attempt_id)
        .expect("publish fixture source database");
    let source_db_rel_path = source_kernel::source_db::canonical_source_db_rel_path(&source.id);
    let seal = seal_for(
        &source.id.0,
        attempt_id,
        &catalog_fingerprint,
        &source_db_rel_path,
        &publication_digest,
    );
    connection
        .execute(
            "INSERT INTO data_source_catalog_publications (
                data_source_id, attempt_id, input_fingerprint, source_db_rel_path,
                catalog_digest, seal, state, published_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'published', datetime('now'))",
            rusqlite::params![
                source.id.0,
                attempt_id,
                catalog_fingerprint,
                source_db_rel_path,
                publication_digest,
                seal,
            ],
        )
        .expect("insert catalog publication seal");
    persistence_sqlite::repositories::structure_view_attempt_repo::StructureViewAttemptRepo::new(
        &connection,
    )
    .bind(
        "case-1",
        &domain::structure::StructureViewAttempt {
            derived_source_id: source.id.0.clone(),
            input_fingerprint: catalog_fingerprint,
            attempt_id: attempt_id.to_string(),
            owner_id: "fixture-owner".to_string(),
            generation: "source.fjall".to_string(),
            receipt: publication_digest,
        },
    )
    .expect("bind fixture view attempt");
    (connection, source, fingerprint, persisted_summary)
}

#[test]
fn catalog_phase_queue_and_ready_state_publish_atomically() {
    let connection = setup_case_db();
    let source = data_source();
    let attempt = start_catalog(&connection, &source.id, FINGERPRINT).expect("claim catalog");
    let catalog_fingerprint =
        super::super::catalog_manifest::catalog_fingerprint_for_source(FINGERPRINT);
    let publication = CatalogPublicationRepo::new(&connection)
        .prepare(
            &source.id,
            attempt.attempt_id(),
            &catalog_fingerprint,
            &source_kernel::source_db::canonical_source_db_rel_path(&source.id),
            &summary(source.clone()).catalog_digest,
        )
        .expect("prepare publication seal");
    CatalogPublicationRepo::new(&connection)
        .mark_published(&source.id, attempt.attempt_id(), &publication.seal)
        .expect("publish publication seal");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_ready_publish
             BEFORE UPDATE OF import_state ON data_sources
             WHEN NEW.import_state = 'ready'
             BEGIN
                 SELECT RAISE(ABORT, 'injected ready publication failure');
             END;",
        )
        .expect("install failure injection");

    let error = complete_catalog_for_test(
        &connection,
        &source,
        FINGERPRINT,
        &attempt,
        &summary(source.clone()),
    )
    .expect_err("ready publication should fail");
    assert!(matches!(error, DerivedSourceError::Database(_)));

    let phases = DataSourceProcessingPhaseRepo::new(&connection)
        .list_for_data_source(&source.id)
        .expect("list phases after rollback");
    assert_eq!(phases.len(), 1);
    assert_eq!(phases[0].phase, ProcessingPhase::Catalog);
    assert_eq!(phases[0].state, ProcessingPhaseState::Running);
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_storage(&source.id)
            .expect("query storage after rollback")
            .expect("storage exists")
            .import_state,
        "pending"
    );

    connection
        .execute_batch("DROP TRIGGER reject_ready_publish;")
        .expect("remove failure injection");
    complete_catalog_for_test(
        &connection,
        &source,
        FINGERPRINT,
        &attempt,
        &summary(source.clone()),
    )
    .expect("publish catalog readiness");

    let phases = DataSourceProcessingPhaseRepo::new(&connection)
        .list_for_data_source(&source.id)
        .expect("list published phases");
    assert_eq!(phases.len(), ProcessingPhase::ALL.len());
    assert_eq!(phases[0].state, ProcessingPhaseState::Ready);
    assert!(phases[1..]
        .iter()
        .all(|phase| phase.state == ProcessingPhaseState::Pending));
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_storage(&source.id)
            .expect("query published storage")
            .expect("storage exists")
            .import_state,
        "ready"
    );
}

#[test]
fn first_rbd_publication_recovers_its_sealed_build_after_control_commit_failure() {
    let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-workspaces");
    std::fs::create_dir_all(&scratch).unwrap();
    let workspace = tempfile::tempdir_in(scratch).unwrap();
    let (connection, original, _, _) = setup_recoverable_catalog(workspace.path());
    let replicas = CephRbdLineageRepo::new(&connection)
        .find_by_data_source(&original.id.0)
        .unwrap()
        .unwrap()
        .replicas;
    let source = DataSource {
        id: DataSourceId("rbd-first-publication".into()),
        ..original
    };
    let case_id = CaseId("case-1".into());
    let fingerprint = registration::register_derived_source(
        &connection,
        &case_id,
        CEPH_SCOPE_ID,
        &source,
        &descriptor(),
        &replicas,
        &RbdReplicaPolicy::strict_legacy(),
    )
    .unwrap();
    let attempt = start_catalog(&connection, &source.id, &fingerprint).unwrap();
    let build = source_kernel::source_db::open_fresh_source_build_db(
        workspace.path(),
        &source.id,
        attempt.attempt_id(),
    )
    .unwrap();
    DataSourceRepo::new(&build)
        .upsert_source_local_metadata(&case_id, &source)
        .unwrap();
    persistence_source::repositories::file_repo::FileRepo::new(&build)
        .insert_batch(&catalog_rows(&source))
        .unwrap();
    let storage =
        source_kernel::source_db::open_fjall_source_for_build(&build, &source.id).unwrap();
    let expected =
        super::super::catalog_manifest::summarize_source_connection(&build, source.clone())
            .unwrap();
    persist_current_source_manifest(&build, &storage, &fingerprint, &expected).unwrap();
    drop(storage);
    let receipt = persistence_source::publication::catalog_publication::seal_catalog_generation(
        &build,
        &source.id,
        attempt.attempt_id(),
        &super::super::catalog_manifest::catalog_fingerprint_for_source(&fingerprint),
        persistence_source::publication::catalog_publication::CatalogProjection::Partition,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    source_kernel::source_db::finalize_source_build_db(&build).unwrap();
    drop(build);
    connection
        .execute_batch(
            "CREATE TRIGGER reject_first_ready BEFORE UPDATE OF import_state ON data_sources
         WHEN NEW.id='rbd-first-publication' AND NEW.import_state='ready'
         BEGIN SELECT RAISE(ABORT, 'injected first publication failure'); END;",
        )
        .unwrap();
    let error = finish_materialization(
        &connection,
        workspace.path(),
        &case_id,
        source.clone(),
        expected.clone(),
        &receipt,
        &fingerprint,
        &attempt,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected first publication failure"),
        "{error:?}"
    );
    let publication = CatalogPublicationRepo::new(&connection)
        .find(&source.id)
        .unwrap()
        .unwrap();
    assert_eq!(publication.state, "prepared");
    assert_eq!(publication.catalog_digest, receipt);
    let view = persistence_sqlite::repositories::structure_view_attempt_repo::StructureViewAttemptRepo
        ::new(&connection).find("case-1", &source.id.0).unwrap().unwrap();
    assert_eq!(view.owner_id, attempt.owner_id());
    assert_eq!(view.attempt_id, attempt.attempt_id());
    record_catalog_failure(&connection, &source.id, &fingerprint, &attempt, &error);
    connection
        .execute_batch("DROP TRIGGER reject_first_ready")
        .unwrap();
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        workspace.path(),
        &source.id.0,
        attempt.attempt_id(),
    )
    .unwrap();
    let close =
        source_kernel::storage_runtime::close_source_store_at_path(&layout.published_path())
            .unwrap()
            .unwrap();
    std::fs::rename(layout.published_path(), layout.build_path()).unwrap();
    std::fs::rename(
        source_kernel::source_db::source_db_path(workspace.path(), &source.id),
        persistence_source::publication::source_build_db_path(
            workspace.path(),
            &source.id,
            attempt.attempt_id(),
        )
        .unwrap(),
    )
    .unwrap();
    drop(close);
    let recovered =
        recovery::reuse_existing_catalog(&connection, workspace.path(), &case_id, &source.id)
            .unwrap()
            .unwrap();
    assert_eq!(recovered.file_count, expected.file_count);
    assert_eq!(recovered.catalog_digest, expected.catalog_digest);
    assert_eq!(
        CatalogPublicationRepo::new(&connection)
            .find(&source.id)
            .unwrap()
            .unwrap()
            .state,
        "published"
    );
    assert!(DataSourceProcessingPhaseRepo::new(&connection)
        .list_for_data_source(&source.id)
        .unwrap()
        .iter()
        .any(|phase| phase.phase == ProcessingPhase::Search));
}

#[test]
fn failed_catalog_publication_reuses_persisted_manifest_without_reenumeration() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, fingerprint, persisted_summary) =
        setup_recoverable_catalog(temp.path());
    let attempt = start_catalog(&connection, &source.id, &fingerprint).expect("claim catalog");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_ready_publish
             BEFORE UPDATE OF import_state ON data_sources
             WHEN NEW.import_state = 'ready'
             BEGIN
                 SELECT RAISE(ABORT, 'injected ready publication failure');
             END;",
        )
        .expect("install failure injection");
    let publication_error = complete_catalog_for_test(
        &connection,
        &source,
        &fingerprint,
        &attempt,
        &persisted_summary,
    )
    .expect_err("ready publication should fail");
    record_catalog_failure(
        &connection,
        &source.id,
        &fingerprint,
        &attempt,
        &publication_error,
    );
    connection
        .execute_batch("DROP TRIGGER reject_ready_publish;")
        .expect("remove failure injection");

    let recovered = super::recovery::recover_persisted_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        source.clone(),
    )
    .expect("recover persisted catalog")
    .expect("catalog manifest is reusable");

    assert_eq!(recovered.file_count, persisted_summary.file_count);
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_storage(&source.id)
            .expect("query recovered storage")
            .expect("storage exists")
            .import_state,
        "ready"
    );
    let phases = DataSourceProcessingPhaseRepo::new(&connection)
        .list_for_data_source(&source.id)
        .expect("list recovered phases");
    assert_eq!(phases.len(), ProcessingPhase::ALL.len());
    assert_eq!(phases[0].state, ProcessingPhaseState::Ready);
    assert!(phases[1..]
        .iter()
        .all(|phase| phase.state == ProcessingPhaseState::Pending));
    assert!(source_kernel::source_db::source_db_path(temp.path(), &source.id).is_file());
}

#[test]
fn catalog_recovery_finalizes_a_prepared_publication_after_rename() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, persisted_summary) = setup_recoverable_catalog(temp.path());
    connection
        .execute(
            "UPDATE data_source_catalog_publications
             SET state = 'prepared', published_at = NULL
             WHERE data_source_id = ?1",
            [&source.id.0],
        )
        .expect("simulate crash after source database rename");

    let recovered = super::recovery::recover_persisted_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        source.clone(),
    )
    .expect("recover prepared publication")
    .expect("catalog manifest is reusable");

    assert_eq!(recovered.catalog_digest, persisted_summary.catalog_digest);
    assert_eq!(
        CatalogPublicationRepo::new(&connection)
            .find(&source.id)
            .expect("read publication")
            .expect("publication exists")
            .state,
        "published"
    );
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_storage(&source.id)
            .expect("query recovered storage")
            .expect("storage exists")
            .import_state,
        "ready"
    );
}

#[test]
fn catalog_recovery_rejects_seal_tampering_without_deleting_source() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_path = source_kernel::source_db::source_db_path(temp.path(), &source.id);
    connection
        .execute(
            "UPDATE data_source_catalog_publications
             SET seal = ?1
             WHERE data_source_id = ?2",
            rusqlite::params!["0".repeat(64), source.id.0],
        )
        .expect("tamper publication seal");

    let error = super::recovery::recover_persisted_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        source,
    )
    .expect_err("tampered seal must fail closed");
    assert!(matches!(error, DerivedSourceError::InconsistentState(_)));
    assert!(source_path.is_file());
}

#[test]
fn catalog_recovery_rejects_view_receipt_tampering_without_deleting_source() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_path = source_kernel::source_db::source_db_path(temp.path(), &source.id);
    let repo =
        persistence_sqlite::repositories::structure_view_attempt_repo::StructureViewAttemptRepo::new(
            &connection,
        );
    let attempt = repo
        .find("case-1", &source.id.0)
        .expect("read view attempt")
        .expect("view attempt exists");
    connection
        .execute(
            "DELETE FROM structure_view_attempts WHERE derived_source_id=?1",
            [&source.id.0],
        )
        .expect("remove view attempt");
    let tampered = domain::structure::StructureViewAttempt {
        receipt: if attempt.receipt.starts_with('0') {
            format!("1{}", &attempt.receipt[1..])
        } else {
            format!("0{}", &attempt.receipt[1..])
        },
        ..attempt
    };
    repo.bind("case-1", &tampered)
        .expect("insert tampered receipt");

    let error = super::recovery::recover_persisted_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        source,
    )
    .expect_err("tampered view receipt must fail closed");
    assert!(matches!(error, DerivedSourceError::InconsistentState(_)));
    assert!(source_path.is_file());
}

#[test]
fn catalog_recovery_rejects_view_input_lineage_mismatch() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_path = source_kernel::source_db::source_db_path(temp.path(), &source.id);
    connection
        .execute(
            "DELETE FROM structure_view_inputs
             WHERE derived_source_id=?1
               AND parent_source_id=(
                   SELECT parent_source_id FROM structure_view_inputs
                   WHERE derived_source_id=?1 ORDER BY parent_source_id LIMIT 1
               )",
            [&source.id.0],
        )
        .expect("remove one RBD view input");

    let error = super::recovery::recover_persisted_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        source,
    )
    .expect_err("missing RBD view input must fail closed");
    assert!(matches!(error, DerivedSourceError::InconsistentState(_)));
    assert!(source_path.is_file());
}

#[test]
fn catalog_recovery_rejects_path_tampering_without_deleting_source() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_path = source_kernel::source_db::source_db_path(temp.path(), &source.id);
    connection
        .execute(
            "UPDATE data_source_catalog_publications
             SET source_db_rel_path = ?1
             WHERE data_source_id = ?2",
            rusqlite::params!["sources/other/source.db", source.id.0],
        )
        .expect("tamper publication path");

    let error = super::recovery::recover_persisted_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        source,
    )
    .expect_err("tampered path must fail closed");
    assert!(matches!(error, DerivedSourceError::InconsistentState(_)));
    assert!(source_path.is_file());
}

#[test]
fn catalog_recovery_rejects_digest_tampering_without_deleting_source() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_path = source_kernel::source_db::source_db_path(temp.path(), &source.id);
    connection
        .execute(
            "UPDATE data_source_catalog_publications
             SET catalog_digest = ?1
             WHERE data_source_id = ?2",
            rusqlite::params!["c".repeat(64), source.id.0],
        )
        .expect("tamper publication digest");

    let error = super::recovery::recover_persisted_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        source,
    )
    .expect_err("tampered digest must fail closed");
    assert!(matches!(error, DerivedSourceError::InconsistentState(_)));
    assert!(source_path.is_file());
}

#[test]
fn catalog_recovery_rejects_file_tree_drift_without_deleting_source() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_path = source_kernel::source_db::source_db_path(temp.path(), &source.id);
    let source_connection = source_kernel::source_db::open_source_db(temp.path(), &source.id)
        .expect("open source database");
    let store = persistence_source::detail::open_store_for_write(&source_connection)
        .unwrap()
        .unwrap();
    persistence_source::fjall::CatalogRecordStore::from_source_store((*store).clone())
        .delete(
            &source.id.0,
            "file",
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .unwrap();
    source_kernel::source_db::checkpoint_source_db(&source_connection)
        .expect("checkpoint source database");
    drop(source_connection);

    let error = reuse_existing_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        &source.id,
    )
    .expect_err("drifted catalog must not be republished or reset");
    assert!(matches!(error, DerivedSourceError::InconsistentState(_)));
    assert!(source_path.is_file());
    assert!(DataSourceRepo::new(&connection)
        .find_storage(&source.id)
        .expect("query source registration")
        .is_some());
}

fn catalog_rows(source: &DataSource) -> Vec<FileEntry> {
    vec![
        FileEntry {
            id: FileEntryId("root".into()),
            parent_id: None,
            data_source_id: source.id.clone(),
            path: String::new(),
            name: "root".into(),
            entry_type: EntryType::Directory,
            size: None,
            ext: None,
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        },
        FileEntry {
            id: FileEntryId("file".into()),
            parent_id: Some(FileEntryId("root".into())),
            data_source_id: source.id.clone(),
            path: "etc/passwd".into(),
            name: "passwd".into(),
            entry_type: EntryType::File,
            size: Some(42),
            ext: None,
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        },
    ]
}

#[test]
fn catalog_recovery_rejects_partition_drift_without_deleting_source() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_path = source_kernel::source_db::source_db_path(temp.path(), &source.id);
    let source_connection = source_kernel::source_db::open_source_db(temp.path(), &source.id)
        .expect("open source database");
    let storage =
        source_kernel::source_db::open_fjall_source_for_build(&source_connection, &source.id)
            .expect("open Fjall source storage");
    storage
        .partition_replace_by_source(
            &source.id.0,
            &[],
            persistence_source::fjall::PersistMode::Buffer,
        )
        .expect("delete partition inventory");
    storage
        .partition_projection_publish(&source.id.0)
        .expect("publish empty partition inventory");
    drop(storage);

    let error = reuse_existing_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        &source.id,
    )
    .expect_err("drifted partition inventory must not be republished or reset");
    assert!(matches!(error, DerivedSourceError::InconsistentState(_)));
    assert!(source_path.is_file());
}

#[test]
fn active_catalog_claim_and_attempt_build_are_not_reset_when_final_db_is_missing() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let connection = setup_case_db();
    let source = data_source();
    let attempt = start_catalog(&connection, &source.id, FINGERPRINT).expect("claim catalog");
    let build_path = temp
        .path()
        .join("sources")
        .join(&source.id.0)
        .join(format!("source.db.build.{}", attempt.attempt_id()));
    let build = source_kernel::source_db::open_fresh_source_build_db(
        temp.path(),
        &source.id,
        attempt.attempt_id(),
    )
    .expect("open attempt build");
    drop(build);

    assert!(reuse_existing_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".to_string()),
        &source.id,
    )
    .expect("inspect active Catalog")
    .is_none());
    assert!(build_path.is_file());
    assert!(DataSourceRepo::new(&connection)
        .find_storage(&source.id)
        .expect("query source registration")
        .is_some());
    assert!(matches!(
        start_catalog(&connection, &source.id, FINGERPRINT),
        Err(DerivedSourceError::ProcessingBusy { phase: "catalog" })
    ));
}

#[test]
fn stale_catalog_attempt_cannot_publish_after_a_new_claim_takes_over() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let connection = setup_case_db();
    let source = data_source();
    let stale_attempt =
        start_catalog(&connection, &source.id, FINGERPRINT).expect("claim stale attempt");
    let stale_build = source_kernel::source_db::open_fresh_source_build_db(
        temp.path(),
        &source.id,
        stale_attempt.attempt_id(),
    )
    .expect("open stale attempt build");
    source_kernel::source_db::finalize_source_build_db(&stale_build).expect("finalize stale build");
    drop(stale_build);
    persist_catalog_failure(
        &connection,
        &source.id,
        FINGERPRINT,
        &stale_attempt,
        &DerivedSourceError::InconsistentState("injected takeover".to_string()),
    )
    .expect("fail stale attempt");
    let active_attempt =
        start_catalog(&connection, &source.id, FINGERPRINT).expect("claim active attempt");

    let catalog_fingerprint =
        super::super::catalog_manifest::catalog_fingerprint_for_source(FINGERPRINT);
    CatalogPublicationRepo::new(&connection)
        .prepare(
            &source.id,
            stale_attempt.attempt_id(),
            &catalog_fingerprint,
            &source_kernel::source_db::canonical_source_db_rel_path(&source.id),
            &"b".repeat(64),
        )
        .expect_err("stale attempt must fail before publication");
    assert!(!source_kernel::source_db::source_db_path(temp.path(), &source.id).exists());
    assert!(temp
        .path()
        .join("sources")
        .join(&source.id.0)
        .join(format!("source.db.build.{}", stale_attempt.attempt_id()))
        .is_file());
    crate::derived_source_service::finalizer::refresh_catalog_claim(
        &connection,
        &source.id,
        FINGERPRINT,
        &active_attempt,
    )
    .expect("active attempt remains valid");
}

#[test]
fn catalog_failure_phase_and_source_state_roll_back_together() {
    let connection = setup_case_db();
    let source = data_source();
    let attempt = start_catalog(&connection, &source.id, FINGERPRINT).expect("claim catalog");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_failed_source_state
             BEFORE UPDATE OF import_state ON data_sources
             WHEN NEW.import_state = 'failed'
             BEGIN
                 SELECT RAISE(ABORT, 'injected failed-state publication failure');
             END;",
        )
        .expect("install failure injection");

    let error = persist_catalog_failure(
        &connection,
        &source.id,
        FINGERPRINT,
        &attempt,
        &DerivedSourceError::InconsistentState("injected failure".to_string()),
    )
    .expect_err("failure publication should roll back");
    assert!(matches!(error, DerivedSourceError::Database(_)));

    let phase = DataSourceProcessingPhaseRepo::new(&connection)
        .find(&source.id, ProcessingPhase::Catalog)
        .expect("query catalog phase")
        .expect("catalog phase");
    assert_eq!(phase.state, ProcessingPhaseState::Running);
    assert_eq!(
        DataSourceRepo::new(&connection)
            .find_storage(&source.id)
            .expect("query source storage")
            .expect("source storage")
            .import_state,
        "pending"
    );
}

#[test]
fn cancelled_catalog_is_deferred_and_remains_retryable() {
    let connection = setup_case_db();
    let source = data_source();
    let attempt = start_catalog(&connection, &source.id, FINGERPRINT).expect("claim catalog");

    persist_catalog_deferred(
        &connection,
        &source.id,
        FINGERPRINT,
        &attempt,
        "cancelled by test",
    )
    .expect("defer catalog");

    let phase = DataSourceProcessingPhaseRepo::new(&connection)
        .find(&source.id, ProcessingPhase::Catalog)
        .expect("query catalog phase")
        .expect("catalog phase");
    assert_eq!(phase.state, ProcessingPhaseState::Deferred);
    assert_eq!(phase.last_error.as_deref(), Some("cancelled by test"));

    let storage = DataSourceRepo::new(&connection)
        .find_storage(&source.id)
        .expect("query source storage")
        .expect("source storage");
    assert_eq!(storage.import_state, "pending");
    assert_eq!(storage.last_error.as_deref(), Some("cancelled by test"));

    start_catalog(&connection, &source.id, FINGERPRINT).expect("reclaim catalog");
    let reclaimed = DataSourceProcessingPhaseRepo::new(&connection)
        .find(&source.id, ProcessingPhase::Catalog)
        .expect("query reclaimed catalog phase")
        .expect("reclaimed catalog phase");
    assert_eq!(reclaimed.state, ProcessingPhaseState::Running);
}

#[test]
fn catalog_recovery_rejects_changed_partition_uuid_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_connection = source_kernel::source_db::open_source_db(temp.path(), &source.id)
        .expect("open source database");
    let storage =
        source_kernel::source_db::open_fjall_source_for_build(&source_connection, &source.id)
            .unwrap();
    let snapshot = storage.snapshot();
    let mut records = storage
        .partition_list_by_source(
            &snapshot,
            &source.id.0,
            None,
            persistence_source::fjall::ScanDirection::Forward,
            256,
        )
        .unwrap();
    let mut record = records.records.pop().expect("partition record");
    record.gpt_partition_uuid = Some("00112233-4455-6677-8899-aabbccddeeff".into());
    storage
        .partition_replace_by_source(
            &source.id.0,
            &[record],
            persistence_source::fjall::PersistMode::Buffer,
        )
        .unwrap();
    storage.partition_projection_publish(&source.id.0).unwrap();
    drop(storage);
    let error = reuse_existing_catalog(
        &connection,
        temp.path(),
        &CaseId("case-1".into()),
        &source.id,
    )
    .unwrap_err();
    assert!(matches!(error, DerivedSourceError::InconsistentState(_)));
}
