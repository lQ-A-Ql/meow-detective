use app_services::ceph_reconstruction::{
    merge_cephfs_metadata_inventories, CephFsDescriptor, CephFsDescriptorState,
    CephFsInventoryError, CephFsObjectLocator, CephFsPoolBinding, CephFsPoolProvenance,
    CephFsPoolRole,
};
use persistence_sqlite::repositories::ceph_fs_metadata_inventory_repo::{
    cephfs_metadata_inventory_sha256, CephFsMetadataInventory, CephFsMetadataInventoryManifest,
    CephFsMetadataObjectProjection, CEPHFS_METADATA_CLASSIFIER_PROFILE,
    CEPHFS_METADATA_SCHEMA_VERSION,
};

#[path = "support/cephfs_inventory.rs"]
mod support;

fn descriptor() -> CephFsDescriptor {
    CephFsDescriptor {
        identity: "ceph-fs:cluster-a:1:17:7".to_string(),
        cluster_identity: "cluster-a".to_string(),
        filesystem_id: 1,
        name: "cephfs-a".to_string(),
        fsmap_epoch: 17,
        mdsmap_epoch: 17,
        state: CephFsDescriptorState::Present,
        metadata_pool: CephFsPoolBinding {
            pool_id: 7,
            role: CephFsPoolRole::Metadata,
            provenance: vec![
                CephFsPoolProvenance {
                    source_identity: "source-a".to_string(),
                    inventory_identity: "inventory-a".to_string(),
                },
                CephFsPoolProvenance {
                    source_identity: "source-b".to_string(),
                    inventory_identity: "inventory-b".to_string(),
                },
            ],
        },
        data_pools: Vec::new(),
        rank_bindings: Vec::new(),
        daemons: Vec::new(),
        provenance: Vec::new(),
    }
}

fn source_inventory(source: &str, inventory: &str, record_digest: char) -> CephFsMetadataInventory {
    let mut inventory = CephFsMetadataInventory {
        manifest: CephFsMetadataInventoryManifest {
            filesystem_identity: "ceph-fs:cluster-a:1:17:7".to_string(),
            inventory_id: inventory.to_string(),
            data_source_id: source.to_string(),
            filesystem_id: 1,
            fsmap_epoch: 17,
            metadata_pool_id: 7,
            schema_version: CEPHFS_METADATA_SCHEMA_VERSION,
            classifier_profile: CEPHFS_METADATA_CLASSIFIER_PROFILE.to_string(),
            source_semantic_sha256: "a".repeat(64),
            inventory_sha256: String::new(),
            object_count: 1,
            unknown_object_count: 0,
            complete: true,
        },
        objects: vec![CephFsMetadataObjectProjection {
            object_identity_sha256: "b".repeat(64),
            locator: "1:7:hff00:h312e3030303030303030:17".to_string(),
            candidate_mask: 63,
            classification_state: "candidate".to_string(),
            classifier_rule: "dirfrag_candidate".to_string(),
            record_sha256: record_digest.to_string().repeat(64),
        }],
    };
    inventory.manifest.inventory_sha256 =
        cephfs_metadata_inventory_sha256(&inventory.manifest, &inventory.objects);
    inventory
}

#[test]
fn locator_round_trips_binary_names_and_checks_ranges() {
    let locator =
        CephFsObjectLocator::new(1, 7, vec![0xff, 0x00], b"1.00000000".to_vec(), 17).unwrap();
    let canonical = locator.canonical();
    assert_eq!(CephFsObjectLocator::parse(&canonical).unwrap(), locator);
    assert_eq!(locator.checked_range(8, 8, 16).unwrap(), 8..16);
    assert!(matches!(
        locator.checked_range(9, 8, 16),
        Err(CephFsInventoryError::RangeOutOfBounds { .. })
    ));
    assert!(matches!(
        locator.checked_range(u64::MAX, 2, u64::MAX),
        Err(CephFsInventoryError::RangeOverflow { .. })
    ));
    assert!(CephFsObjectLocator::parse("1:7:ff00:h31:17").is_err());
}

#[test]
fn merge_deduplicates_replica_provenance_without_losing_identity() {
    let merged = merge_cephfs_metadata_inventories(
        &descriptor(),
        &[
            source_inventory("source-a", "inventory-a", 'e'),
            source_inventory("source-b", "inventory-b", 'e'),
        ],
    )
    .unwrap();
    assert_eq!(merged.object_count, 1);
    assert_eq!(merged.unknown_object_count, 0);
    assert_eq!(merged.objects[0].provenance.len(), 2);
    assert_eq!(merged.inventory_sha256.len(), 64);
}

#[test]
fn merge_rejects_locator_and_source_snapshot_conflicts() {
    let conflict = merge_cephfs_metadata_inventories(
        &descriptor(),
        &[
            source_inventory("source-a", "inventory-a", 'e'),
            source_inventory("source-b", "inventory-b", 'f'),
        ],
    );
    assert!(matches!(
        conflict,
        Err(CephFsInventoryError::ObjectIdentityConflict { .. })
    ));

    let changed = source_inventory("source-a", "inventory-a", 'f');
    assert!(matches!(
        merge_cephfs_metadata_inventories(
            &descriptor(),
            &[source_inventory("source-a", "inventory-a", 'e'), changed]
        ),
        Err(CephFsInventoryError::SourceSnapshotConflict)
    ));
}

#[test]
fn merge_rejects_invalid_manifests_unbound_sources_and_noncanonical_locators() {
    let mut invalid_manifest = source_inventory("source-a", "inventory-a", 'e');
    invalid_manifest.manifest.schema_version += 1;
    assert!(matches!(
        merge_cephfs_metadata_inventories(&descriptor(), &[invalid_manifest]),
        Err(CephFsInventoryError::InvalidBinding(_))
    ));

    let unbound = source_inventory("source-c", "inventory-c", 'e');
    assert!(matches!(
        merge_cephfs_metadata_inventories(&descriptor(), &[unbound]),
        Err(CephFsInventoryError::InvalidBinding(_))
    ));

    let mut invalid_locator = source_inventory("source-a", "inventory-a", 'e');
    invalid_locator.objects[0].locator = "1:8:hff00:h312e3030303030303030:17".to_string();
    invalid_locator.manifest.inventory_sha256 =
        cephfs_metadata_inventory_sha256(&invalid_locator.manifest, &invalid_locator.objects);
    assert!(matches!(
        merge_cephfs_metadata_inventories(&descriptor(), &[invalid_locator]),
        Err(CephFsInventoryError::InvalidLocator)
    ));
}

#[test]
fn inventory_digest_is_independent_of_projection_order() {
    let mut inventory = source_inventory("source-a", "inventory-a", 'e');
    let mut second = inventory.objects[0].clone();
    second.object_identity_sha256 = "d".repeat(64);
    second.locator = "1:7:hff00:h322e3030303030303030:17".to_string();
    second.record_sha256 = "f".repeat(64);
    inventory.objects.push(second);
    inventory.manifest.object_count = 2;
    let forward = cephfs_metadata_inventory_sha256(&inventory.manifest, &inventory.objects);
    inventory.objects.reverse();
    let reversed = cephfs_metadata_inventory_sha256(&inventory.manifest, &inventory.objects);
    assert_eq!(forward, reversed);
}

#[test]
fn source_inventory_uses_real_semantic_rows_and_does_not_create_a_cephfs_source() {
    let conn = support::source_with_metadata_objects();
    let inventory = app_services::ceph_reconstruction::inventory_cephfs_metadata_pool(
        &conn,
        &descriptor(),
        support::SOURCE,
        support::INVENTORY,
    )
    .unwrap();
    assert_eq!(inventory.manifest.object_count, 2);
    assert_eq!(inventory.manifest.unknown_object_count, 1);
    assert!(inventory
        .objects
        .iter()
        .any(|object| object.classifier_rule == "dirfrag_candidate"));
    assert!(inventory
        .objects
        .iter()
        .any(|object| object.classifier_rule == "unknown_object"));

    let repeated = app_services::ceph_reconstruction::inventory_cephfs_metadata_pool(
        &conn,
        &descriptor(),
        support::SOURCE,
        support::INVENTORY,
    )
    .unwrap();
    assert_eq!(repeated, inventory);
    let source_count: u64 = conn
        .query_row("SELECT COUNT(*) FROM data_sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(source_count, 1);
}
