use persistence_sqlite::{
    repositories::{
        ceph_fs_capability_repo::{
            CephFsSourceCapability, CephFsSourceCapabilityRecord, CephFsSourceCapabilityRepo,
        },
        ceph_fs_namespace_assembly_repo::{
            CephFsNamespaceAssemblyRecord, CephFsNamespaceAssemblyRepo,
        },
    },
    runner,
};
use rusqlite::Connection;

const FILESYSTEM_IDENTITY: &str = "ceph-fs:cluster:1:42:7";
const DATA_SOURCE_ID: &str = "cephfs-source";

fn connection() -> Connection {
    let connection = Connection::open_in_memory().expect("open source database");
    runner::run_source_all(&connection).expect("run source migrations");
    connection
        .execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
             VALUES (?1, 'case', 'CephFS', 'ceph_fs', 'cephfs://cluster/1',
                     '2026-07-20T00:00:00Z')",
            [DATA_SOURCE_ID],
        )
        .expect("insert source metadata");
    connection
        .execute(
            "INSERT INTO ceph_fs_namespace_manifests (
                filesystem_identity, data_source_id, filesystem_id, fsmap_epoch,
                root_inode, input_sha256, projection_sha256, schema_version,
                decoder_profile, completeness, published, entry_count,
                inode_count, diagnostic_count
             ) VALUES (?1, ?2, 1, 42, 1, ?3, ?4, 1,
                       'cephfs-namespace-v1', 'closed', 1, 1, 1, 0)",
            rusqlite::params![
                FILESYSTEM_IDENTITY,
                DATA_SOURCE_ID,
                "a".repeat(64),
                "b".repeat(64),
            ],
        )
        .expect("insert namespace manifest");
    connection
}

fn assembly() -> CephFsNamespaceAssemblyRecord {
    CephFsNamespaceAssemblyRecord {
        filesystem_identity: FILESYSTEM_IDENTITY.to_string(),
        data_source_id: DATA_SOURCE_ID.to_string(),
        assembly_sha256: "c".repeat(64),
        assembly_version: 1,
        complete: true,
        frozen: false,
        freeze_reasons_json: "[]".to_string(),
        mutation_state: "complete".to_string(),
        mutation_digest: None,
    }
}

fn capability() -> CephFsSourceCapabilityRecord {
    CephFsSourceCapabilityRecord {
        filesystem_identity: FILESYSTEM_IDENTITY.to_string(),
        data_source_id: DATA_SOURCE_ID.to_string(),
        capability: CephFsSourceCapability::BoundedPreview,
        lineage_fingerprint: "d".repeat(64),
        assembly_sha256: "c".repeat(64),
        namespace_projection_sha256: "b".repeat(64),
        schema_version: 1,
        decoder_profile: "cephfs-namespace-v1".to_string(),
    }
}

#[test]
fn assembly_and_capability_round_trip_with_exact_verification() {
    let connection = connection();
    let assembly_repo = CephFsNamespaceAssemblyRepo::new(&connection);
    let expected_assembly = assembly();
    assembly_repo
        .replace(&expected_assembly)
        .expect("persist namespace assembly");
    assert_eq!(
        assembly_repo
            .verify(&expected_assembly)
            .expect("verify namespace assembly"),
        expected_assembly
    );

    let capability_repo = CephFsSourceCapabilityRepo::new(&connection);
    let expected_capability = capability();
    capability_repo
        .replace(&expected_capability)
        .expect("persist source capability");
    assert_eq!(
        capability_repo
            .verify(&expected_capability)
            .expect("verify source capability"),
        expected_capability
    );
}

#[test]
fn inconsistent_freeze_state_and_stale_capability_are_rejected() {
    let connection = connection();
    let assembly_repo = CephFsNamespaceAssemblyRepo::new(&connection);
    let mut invalid = assembly();
    invalid.frozen = true;
    assert!(assembly_repo.replace(&invalid).is_err());

    let capability_repo = CephFsSourceCapabilityRepo::new(&connection);
    let expected = capability();
    capability_repo
        .replace(&expected)
        .expect("persist source capability");
    let mut stale = expected;
    stale.namespace_projection_sha256 = "e".repeat(64);
    assert!(capability_repo.verify(&stale).is_err());
}
