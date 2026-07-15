use std::path::PathBuf;

use ceph_wire::RbdImageMetadata;
use domain::DataSourceId;

use super::*;

fn replica(inventory_id: &str) -> RadosReplicaSource {
    RadosReplicaSource::new(
        DataSourceId(format!("source-{inventory_id}")),
        inventory_id,
        PathBuf::from(format!("sources/{inventory_id}/source.db")),
    )
    .expect("valid replica")
}

fn descriptor(image_id: &str, image_size: u64) -> RbdImageDescriptor {
    RbdImageDescriptor {
        metadata: RbdImageMetadata {
            name: "vm-100-disk-0".to_string(),
            id: image_id.to_string(),
            object_prefix: format!("rbd_data.{image_id}"),
            image_size,
            order: 22,
            features: 0,
            stripe_unit: 0,
            stripe_count: 0,
            data_pool_id: 8,
        },
        scope_identity: "perPool:i8:-:0000000000000010".to_string(),
        context: super::super::RbdReadContext {
            operation_features: 0,
            has_parent: false,
            snapshot_id: None,
            encrypted: false,
        },
    }
}

#[test]
fn rejects_empty_replica_set_before_opening_source_databases() {
    let error = discover_rbd_images_from_source_dbs(&[]).expect_err("empty set must fail");

    assert!(matches!(
        error,
        RbdReconstructionError::ReplicaCoverageNotClosed {
            expected: 0,
            provided: 0
        }
    ));
}

#[test]
fn rejects_explicit_replica_count_mismatch_before_source_access() {
    let error = detect_rbd_image_from_source_dbs(vec![replica("inventory-a")], "image-1", 2)
        .expect_err("incomplete set must fail");

    assert!(matches!(
        error,
        RbdReconstructionError::ReplicaCoverageNotClosed {
            expected: 2,
            provided: 1
        }
    ));
}

#[test]
fn rejects_duplicate_replica_inventory_before_source_access() {
    let replicas = vec![replica("inventory-a"), replica("inventory-a")];
    let error =
        discover_rbd_images_from_source_dbs(&replicas).expect_err("duplicate set must fail");

    assert!(matches!(
        error,
        RbdReconstructionError::DuplicateReplicaInventory { inventory_id }
            if inventory_id == "inventory-a"
    ));
}

#[test]
fn rejects_duplicate_replica_source_before_source_access() {
    let replicas = vec![
        replica("inventory-a"),
        RadosReplicaSource::new(
            DataSourceId("source-inventory-a".to_string()),
            "inventory-b",
            PathBuf::from("sources/inventory-b/source.db"),
        )
        .expect("valid replica"),
    ];
    let error =
        discover_rbd_images_from_source_dbs(&replicas).expect_err("duplicate source must fail");

    assert!(matches!(
        error,
        RbdReconstructionError::DuplicateReplicaSource { data_source_id }
            if data_source_id == "source-inventory-a"
    ));
}

#[test]
fn source_database_errors_are_inventory_scoped_and_do_not_expose_paths() {
    let source = RadosReplicaSource::new(
        DataSourceId("source-a".to_string()),
        "inventory-a",
        PathBuf::from(r"D:\private\evidence\source.db"),
    )
    .expect("valid replica");
    let error =
        discover_rbd_images_from_source_dbs(&[source]).expect_err("missing source DB must fail");
    let message = error.to_string();

    assert!(matches!(
        error,
        RbdReconstructionError::SourceDb { inventory_id } if inventory_id == "inventory-a"
    ));
    assert!(!message.contains("private"));
    assert!(!message.contains("evidence"));
}

#[test]
fn conflicting_descriptors_fail_closed_instead_of_selecting_one() {
    let mut images = BTreeMap::new();
    merge_descriptor(&mut images, descriptor("image-1", 0x1000)).expect("first descriptor");

    let error = merge_descriptor(&mut images, descriptor("image-1", 0x2000))
        .expect_err("conflict must fail");

    assert!(matches!(
        error,
        RbdReconstructionError::MetadataConflict { image_id } if image_id == "image-1"
    ));
}
