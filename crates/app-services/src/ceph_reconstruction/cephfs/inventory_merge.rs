use std::collections::{BTreeMap, HashMap};

use persistence_sqlite::repositories::ceph_fs_metadata_inventory_repo::{
    validate_cephfs_metadata_inventory, CephFsMetadataInventory,
};

use super::{inventory_digest, CephFsDescriptor, CephFsInventoryError, CephFsObjectLocator};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CephFsObjectProvenance {
    pub data_source_id: String,
    pub inventory_id: String,
    pub object_identity_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsMergedMetadataObject {
    pub locator: String,
    pub candidate_mask: u8,
    pub classification_state: String,
    pub classifier_rule: String,
    pub record_sha256: String,
    pub provenance: Vec<CephFsObjectProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsMergedMetadataInventory {
    pub filesystem_identity: String,
    pub filesystem_id: i64,
    pub fsmap_epoch: u32,
    pub metadata_pool_id: i64,
    pub object_count: u64,
    pub unknown_object_count: u64,
    pub inventory_sha256: String,
    pub objects: Vec<CephFsMergedMetadataObject>,
}

pub fn merge_cephfs_metadata_inventories(
    descriptor: &CephFsDescriptor,
    source_inventories: &[CephFsMetadataInventory],
) -> Result<CephFsMergedMetadataInventory, CephFsInventoryError> {
    if source_inventories.is_empty() {
        return Err(CephFsInventoryError::InvalidBinding(
            "metadata inventory source set is empty",
        ));
    }
    let mut source_snapshots = HashMap::new();
    let mut merged = BTreeMap::<String, CephFsMergedMetadataObject>::new();
    for inventory in source_inventories {
        validate_manifest(descriptor, inventory)?;
        let source_key = inventory.manifest.data_source_id.as_str();
        if let Some(previous) = source_snapshots.insert(
            source_key,
            (
                inventory.manifest.inventory_id.as_str(),
                inventory.manifest.inventory_sha256.as_str(),
            ),
        ) {
            let current = (
                inventory.manifest.inventory_id.as_str(),
                inventory.manifest.inventory_sha256.as_str(),
            );
            if previous != current {
                return Err(CephFsInventoryError::SourceSnapshotConflict);
            }
            continue;
        }
        merge_source(&mut merged, inventory)?;
    }
    let mut objects = merged.into_values().collect::<Vec<_>>();
    for object in &mut objects {
        object.provenance.sort();
        object.provenance.dedup();
    }
    let unknown_object_count = objects
        .iter()
        .filter(|object| object.classification_state == "metadata_only")
        .count() as u64;
    let inventory_sha256 = inventory_digest::merged_inventory_sha256(
        &descriptor.identity,
        objects
            .iter()
            .map(|object| (object.locator.as_str(), object.record_sha256.as_str())),
    );
    Ok(CephFsMergedMetadataInventory {
        filesystem_identity: descriptor.identity.clone(),
        filesystem_id: descriptor.filesystem_id,
        fsmap_epoch: descriptor.fsmap_epoch,
        metadata_pool_id: descriptor.metadata_pool.pool_id,
        object_count: objects.len() as u64,
        unknown_object_count,
        inventory_sha256,
        objects,
    })
}

fn validate_manifest(
    descriptor: &CephFsDescriptor,
    inventory: &CephFsMetadataInventory,
) -> Result<(), CephFsInventoryError> {
    let manifest = &inventory.manifest;
    if validate_cephfs_metadata_inventory(inventory).is_err()
        || manifest.filesystem_identity != descriptor.identity
        || manifest.filesystem_id != descriptor.filesystem_id
        || manifest.fsmap_epoch != descriptor.fsmap_epoch
        || manifest.metadata_pool_id != descriptor.metadata_pool.pool_id
        || !descriptor.metadata_pool.provenance.iter().any(|binding| {
            binding.source_identity == manifest.data_source_id
                && binding.inventory_identity == manifest.inventory_id
        })
    {
        return Err(CephFsInventoryError::InvalidBinding(
            "source inventory is not bound to the filesystem descriptor",
        ));
    }
    for object in &inventory.objects {
        let locator = CephFsObjectLocator::parse(&object.locator)?;
        if locator.filesystem_id() != descriptor.filesystem_id
            || locator.pool_id() != descriptor.metadata_pool.pool_id
            || locator.fsmap_epoch() != descriptor.fsmap_epoch
        {
            return Err(CephFsInventoryError::InvalidLocator);
        }
    }
    Ok(())
}

fn merge_source(
    merged: &mut BTreeMap<String, CephFsMergedMetadataObject>,
    inventory: &CephFsMetadataInventory,
) -> Result<(), CephFsInventoryError> {
    for object in &inventory.objects {
        let provenance = CephFsObjectProvenance {
            data_source_id: inventory.manifest.data_source_id.clone(),
            inventory_id: inventory.manifest.inventory_id.clone(),
            object_identity_sha256: object.object_identity_sha256.clone(),
        };
        match merged.get_mut(&object.locator) {
            Some(existing) => {
                if existing.record_sha256 != object.record_sha256
                    || existing.candidate_mask != object.candidate_mask
                    || existing.classification_state != object.classification_state
                    || existing.classifier_rule != object.classifier_rule
                {
                    return Err(CephFsInventoryError::ObjectIdentityConflict {
                        locator: object.locator.clone(),
                    });
                }
                existing.provenance.push(provenance);
            }
            None => {
                merged.insert(
                    object.locator.clone(),
                    CephFsMergedMetadataObject {
                        locator: object.locator.clone(),
                        candidate_mask: object.candidate_mask,
                        classification_state: object.classification_state.clone(),
                        classifier_rule: object.classifier_rule.clone(),
                        record_sha256: object.record_sha256.clone(),
                        provenance: vec![provenance],
                    },
                );
            }
        }
    }
    Ok(())
}
