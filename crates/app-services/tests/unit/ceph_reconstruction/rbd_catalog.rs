use persistence_sqlite::repositories::ceph_bluestore_omap_repo::{
    CephBluestoreOmapAggregate, CephBluestoreOmapScanRecord, CephBluestoreOmapScopeRecord,
    CephBluestoreRbdDirectoryRecord, CephBluestoreRbdHeaderRecord,
};

use super::*;

const INVENTORY_ID: &str = "ceph-osd:test";

fn aggregate() -> CephBluestoreOmapAggregate {
    CephBluestoreOmapAggregate {
        scan: CephBluestoreOmapScanRecord {
            inventory_id: INVENTORY_ID.to_string(),
            data_source_id: "source-test".to_string(),
            schema_version: 1,
            decode_profile: "omap-rbd-v1".to_string(),
            sharding_sha256: "a".repeat(64),
            latest_state_sha256: "b".repeat(64),
            semantic_sha256: "c".repeat(64),
            omap_sha256: "d".repeat(64),
            scope_count: 1,
            directory_mapping_count: 1,
            rbd_header_count: 1,
            profile_complete: true,
        },
        scopes: vec![CephBluestoreOmapScopeRecord {
            inventory_id: INVENTORY_ID.to_string(),
            scope_identity: "perPool:i8:-:0000000000000010".to_string(),
            key_family: "perPool".to_string(),
            pool_kind: "perPool".to_string(),
            pool_value_i64: Some(8),
            pool_value_hex: None,
            hash: None,
            nid_hex: "0000000000000010".to_string(),
            owner_nid_hex: Some("0000000000000010".to_string()),
            owner_family: Some("perPool".to_string()),
            owner_kind: Some("rbdHeader".to_string()),
            owner_image_id: Some("image-1".to_string()),
            entry_count: 4,
            recognized_entry_count: 4,
        }],
        directory_mappings: vec![CephBluestoreRbdDirectoryRecord {
            inventory_id: INVENTORY_ID.to_string(),
            scope_identity: "perPool:i8:-:0000000000000010".to_string(),
            owner_nid_hex: "0000000000000010".to_string(),
            image_name: "vm-100-disk-0".to_string(),
            image_id: "image-1".to_string(),
            bidirectional: true,
        }],
        rbd_headers: vec![CephBluestoreRbdHeaderRecord {
            inventory_id: INVENTORY_ID.to_string(),
            scope_identity: "perPool:i8:-:0000000000000010".to_string(),
            owner_nid_hex: "0000000000000010".to_string(),
            image_id: "image-1".to_string(),
            size_hex: Some("0000000001000000".to_string()),
            object_order: Some(22),
            features_hex: Some("0000000000000000".to_string()),
            operation_features_hex: None,
            parent_key_present: false,
            object_prefix: Some("rbd_data.image-1".to_string()),
            stripe_unit_hex: None,
            stripe_count_hex: None,
            data_pool_id: None,
        }],
    }
}

#[test]
fn discovers_head_image_from_directory_header_and_scope() {
    let images = discover_rbd_images(&aggregate()).expect("discover RBD image");

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].metadata.name, "vm-100-disk-0");
    assert_eq!(images[0].metadata.id, "image-1");
    assert_eq!(images[0].metadata.image_size, 0x1000000);
    assert_eq!(images[0].metadata.order, 22);
    assert_eq!(images[0].metadata.data_pool_id, 8);
    assert_eq!(images[0].metadata.stripe_unit, 0);
    assert_eq!(images[0].metadata.stripe_count, 0);
    assert_eq!(images[0].context.operation_features, 0);
    assert!(!images[0].context.has_parent);
}

#[test]
fn preserves_parent_and_operation_context_for_fail_closed_reads() {
    let mut value = aggregate();
    value.rbd_headers[0].features_hex = Some("0000000000000100".to_string());
    value.rbd_headers[0].operation_features_hex = Some("0000000000000004".to_string());
    value.rbd_headers[0].parent_key_present = true;

    let images = discover_rbd_images(&value).expect("discover guarded RBD image");

    assert_eq!(images[0].context.operation_features, 4);
    assert!(images[0].context.has_parent);
}

#[test]
fn rejects_inconsistent_operation_feature_metadata() {
    let mut value = aggregate();
    value.rbd_headers[0].operation_features_hex = Some("0000000000000004".to_string());

    assert!(matches!(
        discover_rbd_images(&value),
        Err(RbdCatalogError::InvalidField {
            field: "operation_features",
            ..
        })
    ));
}

#[test]
fn discovers_data_pool_from_per_pg_scope_identity() {
    let mut value = aggregate();
    value.scopes[0].scope_identity =
        "perPg:u0000000000000002:3831950c:0000000000000010".to_string();
    value.scopes[0].key_family = "perPg".to_string();
    value.scopes[0].pool_kind = "perPg".to_string();
    value.scopes[0].pool_value_i64 = None;
    value.scopes[0].pool_value_hex = Some("0000000000000002".to_string());
    value.rbd_headers[0].scope_identity = value.scopes[0].scope_identity.clone();

    let images = discover_rbd_images(&value).expect("discover per-PG RBD image");

    assert_eq!(images[0].metadata.data_pool_id, 2);
}

#[test]
fn rejects_header_without_directory_mapping() {
    let mut value = aggregate();
    value.directory_mappings.clear();

    assert!(matches!(
        discover_rbd_images(&value),
        Err(RbdCatalogError::MissingDirectoryMapping { .. })
    ));
}

#[test]
fn rejects_noncanonical_hex_metadata() {
    let mut value = aggregate();
    value.rbd_headers[0].features_hex = Some("0".to_string());

    assert!(matches!(
        discover_rbd_images(&value),
        Err(RbdCatalogError::InvalidField {
            field: "features",
            ..
        })
    ));
}

#[test]
fn rejects_header_without_pool_identity() {
    let mut value = aggregate();
    value.scopes[0].pool_value_i64 = None;
    value.rbd_headers[0].data_pool_id = None;

    assert!(matches!(
        discover_rbd_images(&value),
        Err(RbdCatalogError::MissingDataPool { .. })
    ));
}

#[test]
fn rejects_noncanonical_per_pg_pool_identity() {
    let mut value = aggregate();
    value.scopes[0].scope_identity =
        "perPg:u0000000000000002:3831950c:0000000000000010".to_string();
    value.scopes[0].key_family = "perPg".to_string();
    value.scopes[0].pool_kind = "perPg".to_string();
    value.scopes[0].pool_value_i64 = None;
    value.scopes[0].pool_value_hex = Some("2".to_string());
    value.rbd_headers[0].scope_identity = value.scopes[0].scope_identity.clone();

    assert!(matches!(
        discover_rbd_images(&value),
        Err(RbdCatalogError::MissingDataPool { .. })
    ));
}
