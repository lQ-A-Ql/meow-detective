use persistence_sqlite::{
    open_in_memory,
    repositories::ceph_rbd_lineage_repo::{
        CephRbdLineageAggregate, CephRbdLineageRecord, CephRbdLineageRepo, CephRbdReplicaRecord,
    },
    runner,
};
use rusqlite::Connection;

const DERIVED_SOURCE_ID: &str = "derived-vm-100";
const CLUSTER_ID: &str = "pve-cluster";

fn setup_case_db() -> Connection {
    let conn = open_in_memory().expect("open case database");
    runner::run_all(&conn).expect("run case migrations");
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-1', 'PVE Case')",
        [],
    )
    .expect("insert case");
    conn.execute(
        "INSERT INTO data_source_clusters (
            id, case_id, name, root_path, platform, manifest_rel_path,
            import_state, member_count, ready_count
         ) VALUES (?1, 'case-1', 'PVE', 'E:/pve', 'linux', 'clusters/pve.json',
                   'ready', 3, 3)",
        [CLUSTER_ID],
    )
    .expect("insert cluster");
    for source_id in [
        DERIVED_SOURCE_ID,
        "source-osd-0",
        "source-osd-1",
        "source-osd-2",
    ] {
        let is_derived = source_id == DERIVED_SOURCE_ID;
        conn.execute(
            "INSERT INTO data_sources (
                id, case_id, name, kind, source_path, platform, import_state, cluster_id
             ) VALUES (?1, 'case-1', ?1, ?2, '', 'linux', 'ready', ?3)",
            rusqlite::params![
                source_id,
                if is_derived { "ceph_rbd" } else { "e01" },
                if is_derived { None } else { Some(CLUSTER_ID) },
            ],
        )
        .expect("insert data source");
    }
    conn
}

fn aggregate() -> CephRbdLineageAggregate {
    CephRbdLineageAggregate {
        lineage: CephRbdLineageRecord {
            derived_data_source_id: DERIVED_SOURCE_ID.to_string(),
            parent_cluster_id: CLUSTER_ID.to_string(),
            image_name: "vm-100-disk-0".to_string(),
            image_id: "16ecc87af5c9".to_string(),
            object_prefix: "rbd_data.16ecc87af5c9".to_string(),
            image_size: 60 * 1024 * 1024 * 1024,
            object_order: 22,
            features: 0x3d,
            stripe_unit: 0,
            stripe_count: 0,
            data_pool_id: 2,
            scope_identity: "pgmeta:2".to_string(),
            operation_features: 0,
            has_parent: false,
            snapshot_id: None,
            encrypted: false,
            expected_replica_count: 3,
        },
        replicas: (0..3)
            .map(|ordinal| CephRbdReplicaRecord {
                ordinal,
                source_data_source_id: format!("source-osd-{ordinal}"),
                inventory_id: format!("inventory-osd-{ordinal}"),
                osd_id: ordinal,
            })
            .collect(),
    }
}

#[test]
fn migration_and_lineage_round_trip_replace_and_delete() {
    let conn = setup_case_db();
    assert_eq!(runner::latest_version(), "0037_ceph_rbd_derived_sources");
    let repo = CephRbdLineageRepo::new(&conn);
    let original = aggregate();
    repo.insert_aggregate(&original).expect("insert lineage");
    assert_eq!(
        repo.find_by_data_source(DERIVED_SOURCE_ID)
            .expect("find lineage"),
        Some(original)
    );

    let mut replacement = aggregate();
    replacement.lineage.snapshot_id = Some(u64::MAX - 1);
    replacement.lineage.encrypted = true;
    repo.replace_aggregate(&replacement)
        .expect("replace lineage");
    assert_eq!(
        repo.find_by_data_source(DERIVED_SOURCE_ID)
            .expect("find replacement"),
        Some(replacement)
    );

    assert!(repo.delete(DERIVED_SOURCE_ID).expect("delete lineage"));
    assert!(!repo
        .delete(DERIVED_SOURCE_ID)
        .expect("delete missing lineage"));
    assert!(repo
        .find_by_data_source(DERIVED_SOURCE_ID)
        .expect("find deleted lineage")
        .is_none());
}

#[test]
fn invalid_identifiers_ordinals_duplicates_and_counts_are_rejected() {
    let conn = setup_case_db();
    let repo = CephRbdLineageRepo::new(&conn);

    let mut missing = aggregate();
    missing.lineage.image_id.clear();
    assert!(repo.insert_aggregate(&missing).is_err());

    let mut non_contiguous = aggregate();
    non_contiguous.replicas[1].ordinal = 2;
    assert!(repo.insert_aggregate(&non_contiguous).is_err());

    for mutate in [
        |value: &mut CephRbdLineageAggregate| {
            value.replicas[1].source_data_source_id =
                value.replicas[0].source_data_source_id.clone();
        },
        |value: &mut CephRbdLineageAggregate| {
            value.replicas[1].inventory_id = value.replicas[0].inventory_id.clone();
        },
        |value: &mut CephRbdLineageAggregate| {
            value.replicas[1].osd_id = value.replicas[0].osd_id;
        },
    ] {
        let mut duplicate = aggregate();
        mutate(&mut duplicate);
        assert!(repo.insert_aggregate(&duplicate).is_err());
    }

    let mut incomplete = aggregate();
    incomplete.replicas.pop();
    assert!(repo.insert_aggregate(&incomplete).is_err());
    assert!(repo
        .find_by_data_source(DERIVED_SOURCE_ID)
        .expect("find absent lineage")
        .is_none());
}

#[test]
fn lineage_foreign_keys_reject_missing_derived_cluster_and_replica_sources() {
    let conn = setup_case_db();
    let repo = CephRbdLineageRepo::new(&conn);

    let mut missing_derived = aggregate();
    missing_derived.lineage.derived_data_source_id = "missing-derived".to_string();
    assert!(repo.insert_aggregate(&missing_derived).is_err());

    let mut missing_cluster = aggregate();
    missing_cluster.lineage.parent_cluster_id = "missing-cluster".to_string();
    assert!(repo.insert_aggregate(&missing_cluster).is_err());

    let original = aggregate();
    repo.insert_aggregate(&original)
        .expect("insert valid lineage");
    let mut missing_replica = aggregate();
    missing_replica.replicas[2].source_data_source_id = "missing-source".to_string();
    assert!(repo.replace_aggregate(&missing_replica).is_err());

    assert_eq!(
        repo.find_by_data_source(DERIVED_SOURCE_ID)
            .expect("find preserved lineage"),
        Some(original)
    );
}
