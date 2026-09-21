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
const CEPH_SCOPE_ID: &str = "scope:ceph:lineage";

fn setup_case_db() -> Connection {
    let conn = open_in_memory().expect("open case database");
    runner::run_all(&conn).expect("run case migrations");
    conn.execute("INSERT INTO cases (id, name) VALUES ('case-1', 'PVE')", [])
        .expect("insert case");
    conn.execute(
        "INSERT INTO linux_topology_scopes (
            id, case_id, scope_kind, name, identity_state, status,
            evidence_completeness, diagnostics_json
         ) VALUES (?1, 'case-1', 'ceph', 'Ceph', 'unproven', 'ready', 'complete', '[]')",
        [CEPH_SCOPE_ID],
    )
    .expect("insert Ceph scope");
    for source_id in [DERIVED_SOURCE_ID, "osd-0", "osd-1", "osd-2", "osd-3"] {
        let derived = source_id == DERIVED_SOURCE_ID;
        conn.execute(
            "INSERT INTO data_sources (
                id, case_id, name, kind, source_path, platform, import_state
             ) VALUES (?1, 'case-1', ?1, ?2, '', 'linux', 'ready')",
            rusqlite::params![source_id, if derived { "ceph_fs" } else { "e01" },],
        )
        .expect("insert source");
        if !derived {
            conn.execute(
                "INSERT INTO linux_topology_memberships (
                    scope_id, data_source_id, role, member_index, confidence, provenance_json
                 ) VALUES (?1, ?2, 'storage_node', 0, 'candidate', '{}')",
                rusqlite::params![CEPH_SCOPE_ID, source_id],
            )
            .expect("bind source to Ceph scope");
        }
    }
    conn.execute(
        "INSERT INTO storage_objects (id, case_id, object_kind, name, identity_state, status)
         VALUES ('storage:cephfs:lineage:ceph-fs:cluster:1:42:7', 'case-1', 'ceph_fs', 'cephfs', 'candidate', 'ready')",
        [],
    ).expect("insert CephFS storage object");
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
            parent_ceph_scope_id: CEPH_SCOPE_ID.to_string(),
            parent_storage_object_id: "storage:cephfs:lineage:ceph-fs:cluster:1:42:7".to_string(),
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
    assert_eq!(runner::latest_version(), "0054_storage_objects");
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
