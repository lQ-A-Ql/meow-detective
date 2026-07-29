use persistence_sqlite::{
    open_in_memory,
    repositories::ceph_fs_lineage_repo::{
        cephfs_lineage_fingerprint, CephFsDerivedLineageAggregate, CephFsDerivedLineageRecord,
        CephFsDerivedLineageRepo, CephFsDerivedMapProvenanceRecord, CephFsDerivedPoolRecord,
        CephFsDerivedPoolSourceRecord,
    },
    runner,
};
use rusqlite::Connection;

const DERIVED_SOURCE_ID: &str = "cephfs-source";
const CLUSTER_ID: &str = "pve-cluster";

fn setup_case_db() -> Connection {
    let conn = open_in_memory().expect("open case database");
    runner::run_all(&conn).expect("run case migrations");
    conn.execute("INSERT INTO cases (id, name) VALUES ('case-1', 'PVE')", [])
        .expect("insert case");
    conn.execute(
        "INSERT INTO data_source_clusters (
            id, case_id, name, root_path, platform, manifest_rel_path,
            import_state, member_count, ready_count
         ) VALUES (?1, 'case-1', 'PVE', 'E:/pve', 'linux', 'clusters/pve.json',
                   'ready', 4, 4)",
        [CLUSTER_ID],
    )
    .expect("insert cluster");
    for source_id in [DERIVED_SOURCE_ID, "osd-0", "osd-1", "osd-2", "osd-3"] {
        let derived = source_id == DERIVED_SOURCE_ID;
        conn.execute(
            "INSERT INTO data_sources (
                id, case_id, name, kind, source_path, platform, import_state, cluster_id
             ) VALUES (?1, 'case-1', ?1, ?2, '', 'linux', 'ready', ?3)",
            rusqlite::params![
                source_id,
                if derived { "ceph_fs" } else { "e01" },
                if derived { None } else { Some(CLUSTER_ID) },
            ],
        )
        .expect("insert source");
    }
    conn
}

fn aggregate() -> CephFsDerivedLineageAggregate {
    let sources = || {
        (0..3)
            .map(|ordinal| CephFsDerivedPoolSourceRecord {
                ordinal,
                source_data_source_id: format!("osd-{ordinal}"),
                inventory_id: format!("inventory-{ordinal}"),
            })
            .collect()
    };
    let mut aggregate = CephFsDerivedLineageAggregate {
        lineage: CephFsDerivedLineageRecord {
            derived_data_source_id: DERIVED_SOURCE_ID.to_string(),
            parent_cluster_id: CLUSTER_ID.to_string(),
            cluster_identity: "cluster".to_string(),
            filesystem_identity: "ceph-fs:cluster:1:42:7".to_string(),
            filesystem_id: 1,
            filesystem_name: "cephfs".to_string(),
            fsmap_epoch: 42,
            mdsmap_epoch: 41,
            descriptor_state: "present".to_string(),
            metadata_pool_id: 7,
            expected_replica_count: 3,
            namespace_input_sha256: "11".repeat(32),
            namespace_projection_sha256: "22".repeat(32),
            namespace_assembly_sha256: "33".repeat(32),
            source_capability: "bounded-preview".to_string(),
            namespace_schema_version: 1,
            decoder_profile: "cephfs-namespace-v1".to_string(),
            journal_boundary_sha256: Some("33".repeat(32)),
            lineage_fingerprint: String::new(),
        },
        pools: vec![
            CephFsDerivedPoolRecord {
                pool_id: 7,
                role: "metadata".to_string(),
                ordinal: 0,
                sources: sources(),
            },
            CephFsDerivedPoolRecord {
                pool_id: 8,
                role: "data".to_string(),
                ordinal: 0,
                sources: sources(),
            },
        ],
        map_provenance: (0..3)
            .map(|ordinal| CephFsDerivedMapProvenanceRecord {
                ordinal,
                source_data_source_id: format!("osd-{ordinal}"),
                inventory_id: format!("inventory-{ordinal}"),
                captured_at: "2026-07-20T00:00:00+00:00".to_string(),
                raw_fsmap_sha256: "44".repeat(32),
                raw_mdsmap_sha256: "55".repeat(32),
            })
            .collect(),
    };
    aggregate.lineage.lineage_fingerprint = cephfs_lineage_fingerprint(&aggregate);
    aggregate
}

#[test]
fn lineage_round_trips_and_cascades_with_derived_source() {
    let conn = setup_case_db();
    assert_eq!(runner::latest_version(), "0043_bitlocker_restore_intents");
    let expected = aggregate();
    let repo = CephFsDerivedLineageRepo::new(&conn);
    repo.insert(&expected).expect("insert lineage");
    assert_eq!(
        repo.find_by_data_source(DERIVED_SOURCE_ID)
            .expect("load lineage"),
        Some(expected)
    );
    conn.execute(
        "DELETE FROM data_sources WHERE id = ?1",
        [DERIVED_SOURCE_ID],
    )
    .expect("delete source");
    assert!(repo
        .find_by_data_source(DERIVED_SOURCE_ID)
        .expect("load deleted lineage")
        .is_none());
}

#[test]
fn incomplete_replica_coverage_and_foreign_sources_are_rejected() {
    let conn = setup_case_db();
    let repo = CephFsDerivedLineageRepo::new(&conn);
    let mut incomplete = aggregate();
    incomplete.pools[1].sources.pop();
    incomplete.lineage.lineage_fingerprint = cephfs_lineage_fingerprint(&incomplete);
    assert!(repo.insert(&incomplete).is_err());

    let mut foreign = aggregate();
    foreign.pools[1].sources[2].source_data_source_id = "foreign".to_string();
    foreign.lineage.lineage_fingerprint = cephfs_lineage_fingerprint(&foreign);
    assert!(repo.insert(&foreign).is_err());
}

#[test]
fn candidate_source_set_may_exceed_the_replica_count() {
    let conn = setup_case_db();
    let mut expanded = aggregate();
    for pool in &mut expanded.pools {
        pool.sources.push(CephFsDerivedPoolSourceRecord {
            ordinal: 3,
            source_data_source_id: "osd-3".to_string(),
            inventory_id: "inventory-3".to_string(),
        });
    }
    expanded.lineage.lineage_fingerprint = cephfs_lineage_fingerprint(&expanded);

    let repo = CephFsDerivedLineageRepo::new(&conn);
    repo.insert(&expanded)
        .expect("candidate source set larger than replica count is valid");
    assert_eq!(
        repo.find_by_data_source(DERIVED_SOURCE_ID)
            .expect("load expanded lineage"),
        Some(expanded)
    );
}
