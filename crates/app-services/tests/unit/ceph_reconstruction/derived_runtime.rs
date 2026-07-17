use persistence_sqlite::repositories::ceph_rbd_lineage_repo::{
    CephRbdLineageAggregate, CephRbdLineageRecord, CephRbdReplicaRecord,
};

use super::lineage_fingerprint;

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
            expected_replica_count: 1,
        },
        replicas: vec![CephRbdReplicaRecord {
            ordinal: 0,
            source_data_source_id: "source".to_string(),
            inventory_id: "inventory".to_string(),
            osd_id: 7,
        }],
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
