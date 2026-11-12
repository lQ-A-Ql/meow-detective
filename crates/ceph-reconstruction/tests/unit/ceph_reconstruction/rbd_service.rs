use std::path::PathBuf;

use ceph_wire::RbdImageMetadata;
use domain::DataSourceId;
use persistence_source::fjall::{ParserRecord, PersistMode};

use super::*;
use crate::ceph_reconstruction::ReplicaIdentity;

fn replica(inventory_id: &str) -> RadosReplicaSource {
    RadosReplicaSource::with_identity(
        DataSourceId(format!("source-{inventory_id}")),
        inventory_id,
        PathBuf::from(format!("sources/{inventory_id}/source.db")),
        ReplicaIdentity::from_inventory(
            Some(inventory_id.bytes().map(u32::from).sum()),
            format!("uuid-{inventory_id}"),
            Some("fsid-test".to_string()),
        ),
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
            expected: 3,
            provided: 0
        }
    ));
}

#[test]
fn unbound_discovery_rejects_empty_replica_set_before_source_access() {
    let error = discover_rbd_images_from_source_dbs_unbound(&[])
        .expect_err("empty unbound set must fail closed");

    assert!(matches!(
        error,
        RbdReconstructionError::CoverageNotProven { .. }
    ));
}

#[test]
fn rejects_incomplete_replica_count_before_source_access() {
    let error = detect_rbd_image_from_source_dbs(vec![replica("inventory-a")], "image-1")
        .expect_err("incomplete set must fail");

    assert!(matches!(
        error,
        RbdReconstructionError::ReplicaCoverageNotClosed {
            expected: 3,
            provided: 1
        }
    ));
}

#[test]
fn rejects_indeterminate_replica_identity_before_source_access() {
    let incomplete = RadosReplicaSource::new(
        DataSourceId("source-incomplete".to_string()),
        "inventory-incomplete",
        PathBuf::from("sources/incomplete/source.db"),
    )
    .expect("valid replica binding");
    let error = discover_rbd_images_from_source_dbs(&[
        incomplete,
        replica("inventory-b"),
        replica("inventory-c"),
    ])
    .expect_err("incomplete identity must fail before source access");

    assert!(matches!(
        error,
        RbdReconstructionError::CoverageNotProven { .. }
    ));
}

#[test]
fn rejects_duplicate_replica_inventory_before_source_access() {
    let replicas = vec![
        replica("inventory-a"),
        replica("inventory-a"),
        replica("inventory-c"),
    ];
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
        replica("inventory-c"),
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
    let source = RadosReplicaSource::with_identity(
        DataSourceId("source-a".to_string()),
        "inventory-a",
        PathBuf::from(r"D:\private\evidence\source.db"),
        ReplicaIdentity::from_inventory(Some(1), "uuid-inventory-a", Some("fsid-test".into())),
    )
    .expect("valid replica");
    let replicas = vec![source, replica("inventory-b"), replica("inventory-c")];
    let error =
        discover_rbd_images_from_source_dbs(&replicas).expect_err("missing source DB must fail");
    let message = error.to_string();

    assert!(matches!(
        error,
        RbdReconstructionError::SourceDb { inventory_id, .. } if inventory_id == "inventory-a"
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

#[test]
fn source_local_scope_identity_does_not_create_a_metadata_conflict() {
    let mut images = BTreeMap::new();
    merge_descriptor(&mut images, descriptor("image-1", 0x1000)).expect("first descriptor");
    let mut replica_descriptor = descriptor("image-1", 0x1000);
    replica_descriptor.scope_identity =
        "perPg:u0000000000000008:12345678:0000000000000020".to_string();

    merge_descriptor(&mut images, replica_descriptor).expect("matching replica metadata");

    assert_eq!(images.len(), 1);
}

#[test]
fn conflicting_replica_identity_fails_before_source_database_access() {
    let mut replicas = vec![
        replica("inventory-a"),
        replica("inventory-b"),
        replica("inventory-c"),
    ];
    for (index, item) in replicas.iter_mut().enumerate() {
        item.identity = ReplicaIdentity::from_inventory(
            Some(index as u32),
            format!("osd-{index}"),
            Some(
                if index == 2 {
                    "different-fsid"
                } else {
                    "shared-fsid"
                }
                .to_string(),
            ),
        );
    }

    let error = discover_rbd_images_from_source_dbs(&replicas)
        .expect_err("conflicting FSIDs must fail before opening source databases");
    assert!(matches!(
        error,
        RbdReconstructionError::IdentityConflict { detail }
            if detail.contains("FSIDs conflict")
    ));
}

#[test]
fn rbd_header_lookup_prefers_bounded_fjall_parser_projection() {
    let root = tempfile::tempdir().expect("temporary case root");
    let source_id = DataSourceId("source-fjall".to_owned());
    let source =
        source_kernel::source_db::open_registered_fjall_source_read_write(root.path(), &source_id)
            .expect("open source storage");
    let payload = serde_json::json!({
        "inventory_id": "inventory-fjall",
        "scope_identity": "perPool:i8",
        "owner_nid_hex": "0000000000000001",
        "image_id": "image-1",
        "size_hex": "0000000000001000",
        "object_order": 22,
        "features_hex": "0000000000000000",
        "operation_features_hex": null,
        "parent_key_present": 0,
        "object_prefix": "rbd_data.image-1",
        "stripe_unit_hex": null,
        "stripe_count_hex": null,
        "data_pool_id": 8
    });
    source
        .parser_upsert(
            &ParserRecord {
                id: "table:ceph_bluestore_rbd_headers:1".to_owned(),
                data_source_id: source_id.0.clone(),
                family: "ceph".to_owned(),
                kind: "ceph_bluestore_rbd_headers".to_owned(),
                status: "ready".to_owned(),
                summary_sha256: infrastructure::hashing::sha256_bytes(
                    payload.to_string().as_bytes(),
                ),
                payload,
            },
            PersistMode::SyncAll,
        )
        .expect("persist parser projection");
    let marker_key = persistence_source::fjall::encode_key(
        "projection",
        &[persistence_source::fjall::KeyPart::Text(
            source_id.0.clone(),
        )],
    )
    .expect("encode projection marker key");
    let marker = persistence_source::fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", source_id.0),
        source_id.0.clone(),
        1,
        source_id.0.as_bytes().to_vec(),
    );
    source
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            marker_key,
            &marker,
            PersistMode::SyncAll,
        )
        .expect("persist source projection marker");
    drop(source);
    let replica = RadosReplicaSource::new(
        source_id,
        "inventory-fjall",
        root.path().join("missing-source.db"),
    )
    .expect("valid replica");
    let header = find_rbd_header_from_case_storage(root.path(), &replica, "image-1")
        .expect("lookup header")
        .expect("header exists");
    assert_eq!(header.image_id, "image-1");
    assert_eq!(header.data_pool_id, Some(8));
}
