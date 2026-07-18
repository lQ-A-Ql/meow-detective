use std::path::PathBuf;

use super::*;
use crate::ceph_reconstruction::derived_source::catalog_manifest::persist_current_source_manifest;
use domain::{DataSourceKind, DataSourcePlatform, DataSourceProvenance};
use persistence_sqlite::repositories::ceph_rbd_lineage_repo::{
    CephRbdLineageRepo, CephRbdReplicaRecord,
};
use persistence_sqlite::repositories::datasource_repo::DataSourceStorage;
use persistence_sqlite::repositories::processing_phase_repo::{
    DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseState,
};

use super::registration::lineage_aggregate;

const SOURCE_ID: &str = "rbd-atomic-publish";
const FINGERPRINT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CLUSTER_ID: &str = "cluster-atomic-publish";
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
        context: crate::ceph_reconstruction::RbdReadContext {
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
            "INSERT INTO data_source_clusters (
                id, case_id, name, root_path, platform, manifest_rel_path,
                import_state, member_count, ready_count
             ) VALUES (
                ?1, 'case-1', 'PVE', 'E:/pve', 'linux', 'clusters/pve.json',
                'ready', 3, 3
             )",
            [CLUSTER_ID],
        )
        .expect("insert cluster");
    for source_id in PARENT_SOURCE_IDS {
        connection
            .execute(
                "INSERT INTO data_sources (
                    id, case_id, name, kind, source_path, platform, import_state, cluster_id
                 ) VALUES (
                    ?1, 'case-1', ?1, 'e01', '', 'linux', 'ready', ?2
                 )",
                [source_id, CLUSTER_ID],
            )
            .expect("insert parent source");
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
            CLUSTER_ID,
            &descriptor(),
            &replicas,
        ))
        .expect("insert derived lineage");
    let fingerprint =
        load_lineage_fingerprint(&connection, &source.id).expect("load lineage fingerprint");
    let source_connection =
        crate::source_db::open_source_db(case_root, &source.id).expect("open source database");
    DataSourceRepo::new(&source_connection)
        .upsert_source_local_metadata(&CaseId("case-1".to_string()), &source)
        .expect("insert source-local metadata");
    source_connection
        .execute(
            "INSERT INTO data_source_partitions (
                id, data_source_id, partition_index, name, kind_label, status,
                offset, length, filesystem
             ) VALUES (
                'partition-0', ?1, 0, 'root', 'Linux filesystem', 'supported',
                0, 4096, 'XFS'
             )",
            [&source.id.0],
        )
        .expect("insert source partition");
    source_connection
        .execute(
            "INSERT INTO file_entries (
                id, parent_id, data_source_id, path, name, entry_type, size,
                deleted, hidden, system, partition_index
             ) VALUES
             ('root', NULL, ?1, '', 'root', 'directory', NULL, 0, 0, 0, 0),
             ('file', 'root', ?1, 'etc/passwd', 'passwd', 'file', 42, 0, 0, 0, 0)",
            [&source.id.0],
        )
        .expect("insert source catalog");
    let persisted_summary = super::super::catalog_manifest::summarize_source_connection(
        &source_connection,
        source.clone(),
    )
    .expect("summarize source catalog");
    persist_current_source_manifest(&source_connection, &fingerprint, &persisted_summary)
        .expect("persist catalog manifest");
    crate::source_db::checkpoint_source_db(&source_connection).expect("checkpoint source database");
    drop(source_connection);
    (connection, source, fingerprint, persisted_summary)
}

#[test]
fn catalog_phase_queue_and_ready_state_publish_atomically() {
    let connection = setup_case_db();
    let source = data_source();
    let attempt = start_catalog(&connection, &source.id, FINGERPRINT).expect("claim catalog");
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

    let error = publish_catalog_readiness(
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
    publish_catalog_readiness(
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
    let publication_error = publish_catalog_readiness(
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
    assert!(crate::source_db::source_db_path(temp.path(), &source.id).is_file());
}

#[test]
fn catalog_recovery_rejects_file_tree_drift_without_deleting_source() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_path = crate::source_db::source_db_path(temp.path(), &source.id);
    let source_connection =
        crate::source_db::open_source_db(temp.path(), &source.id).expect("open source database");
    source_connection
        .execute("DELETE FROM file_entries WHERE id = 'file'", [])
        .expect("delete catalog row");
    crate::source_db::checkpoint_source_db(&source_connection).expect("checkpoint source database");
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

#[test]
fn catalog_recovery_rejects_partition_drift_without_deleting_source() {
    let temp = tempfile::tempdir().expect("create temporary case root");
    let (connection, source, _, _) = setup_recoverable_catalog(temp.path());
    let source_path = crate::source_db::source_db_path(temp.path(), &source.id);
    let source_connection =
        crate::source_db::open_source_db(temp.path(), &source.id).expect("open source database");
    source_connection
        .execute("DELETE FROM data_source_partitions", [])
        .expect("delete partition inventory");
    crate::source_db::checkpoint_source_db(&source_connection).expect("checkpoint source database");
    drop(source_connection);

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
    let build =
        crate::source_db::open_fresh_source_build_db(temp.path(), &source.id, attempt.attempt_id())
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
    let stale_build = crate::source_db::open_fresh_source_build_db(
        temp.path(),
        &source.id,
        stale_attempt.attempt_id(),
    )
    .expect("open stale attempt build");
    crate::source_db::finalize_source_build_db(&stale_build).expect("finalize stale build");
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

    let error = super::super::catalog_build::publish_claimed_source_build(
        &connection,
        temp.path(),
        &source.id,
        FINGERPRINT,
        &stale_attempt,
    )
    .expect_err("stale attempt must fail before rename");

    assert!(matches!(error, DerivedSourceError::Database(_)));
    assert!(!crate::source_db::source_db_path(temp.path(), &source.id).exists());
    assert!(temp
        .path()
        .join("sources")
        .join(&source.id.0)
        .join(format!("source.db.build.{}", stale_attempt.attempt_id()))
        .is_file());
    crate::ceph_reconstruction::derived_finalizer::refresh_catalog_claim(
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
