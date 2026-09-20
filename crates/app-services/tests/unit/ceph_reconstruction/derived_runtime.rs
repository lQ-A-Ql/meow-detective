use persistence_sqlite::repositories::ceph_rbd_lineage_repo::{
    CephRbdLineageAggregate, CephRbdLineageRecord, CephRbdReplicaRecord,
};

use super::{lineage_fingerprint, load_proven_rbd_policy, RbdReplicaPolicy};

fn aggregate() -> CephRbdLineageAggregate {
    CephRbdLineageAggregate {
        lineage: CephRbdLineageRecord {
            derived_data_source_id: "derived".to_string(),
            parent_cluster_id: "cluster".to_string(),
            image_name: "vm-100-disk-0".to_string(),
            image_id: "image-id".to_string(),
            object_prefix: "rbd_data.prefix".to_string(),
            image_size: 1024 * 1024,
            object_order: 22,
            features: 0,
            stripe_unit: 0,
            stripe_count: 0,
            data_pool_id: 1,
            scope_identity: "scope".to_string(),
            operation_features: 0,
            has_parent: false,
            snapshot_id: None,
            encrypted: false,
            expected_replica_count: 3,
            replica_policy_fingerprint: RbdReplicaPolicy::strict_legacy().fingerprint(),
        },
        replicas: (0..3)
            .map(|ordinal| CephRbdReplicaRecord {
                ordinal,
                source_data_source_id: format!("source-{ordinal}"),
                inventory_id: format!("inventory-{ordinal}"),
                osd_id: ordinal + 7,
            })
            .collect(),
    }
}

#[test]
fn fingerprint_is_deterministic_and_covers_lineage() {
    let original = aggregate();
    let mut changed = original.clone();
    changed.lineage.image_size += 1;

    assert_eq!(
        lineage_fingerprint(&original),
        lineage_fingerprint(&original)
    );
    assert_ne!(
        lineage_fingerprint(&original),
        lineage_fingerprint(&changed)
    );
}

#[test]
fn fingerprint_covers_replica_inventory() {
    let original = aggregate();
    let mut changed = original.clone();
    changed.replicas[0].inventory_id = "replacement-inventory".to_string();

    assert_ne!(
        lineage_fingerprint(&original),
        lineage_fingerprint(&changed)
    );
}

#[test]
fn fingerprint_covers_replica_policy_provenance() {
    let original = aggregate();
    let mut changed = original.clone();
    changed.lineage.replica_policy_fingerprint =
        RbdReplicaPolicy::trusted_pool(8, 3, 2, "osdmap:epoch-17", Some(17), Some("a".repeat(64)))
            .expect("trusted policy")
            .fingerprint();

    assert_ne!(
        lineage_fingerprint(&original),
        lineage_fingerprint(&changed)
    );
}

#[test]
fn runtime_requires_a_complete_coverage_report() {
    let case_root = tempfile::tempdir().expect("case root");
    let error = load_proven_rbd_policy(case_root.path(), "cluster-1")
        .expect_err("missing coverage report must fail closed");
    assert!(matches!(
        error,
        super::DerivedRbdReaderError::CoverageNotProven { state } if state == "missing"
    ));

    let report = super::super::assess_inventory_coverage(&[], &RbdReplicaPolicy::strict_legacy());
    crate::cluster_service::write_linux_cluster_coverage_report(
        case_root.path(),
        "cluster-1",
        &report,
    )
    .expect("write incomplete coverage report");
    let error = load_proven_rbd_policy(case_root.path(), "cluster-1")
        .expect_err("incomplete coverage report must fail closed");
    assert!(matches!(
        error,
        super::DerivedRbdReaderError::CoverageNotProven { state } if state == "incomplete"
    ));
}
