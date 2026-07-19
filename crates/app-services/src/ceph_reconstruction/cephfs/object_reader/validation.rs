use std::collections::HashSet;

use persistence_sqlite::repositories::{
    ceph_bluestore_semantic_repo::CephBluestoreObjectReadPlan,
    ceph_fs_metadata_inventory_repo::{
        CephFsMetadataInventoryManifest, CephFsMetadataObjectProjection,
        CEPHFS_METADATA_CLASSIFIER_PROFILE, CEPHFS_METADATA_SCHEMA_VERSION,
    },
};

use super::{CephFsObjectReadError, CephFsObjectSource, ResolvedReplica};
use crate::ceph_reconstruction::{CephFsDescriptor, CephFsObjectLocator, CEPHFS_HEAD_SNAP_HEX};

pub(super) fn validate_descriptor(
    descriptor: &CephFsDescriptor,
) -> Result<(), CephFsObjectReadError> {
    let expected_identity = format!(
        "ceph-fs:{}:{}:{}:{}",
        descriptor.cluster_identity,
        descriptor.filesystem_id,
        descriptor.fsmap_epoch,
        descriptor.metadata_pool.pool_id
    );
    if descriptor.identity != expected_identity
        || descriptor.cluster_identity.trim().is_empty()
        || descriptor.cluster_identity.contains('\0')
        || descriptor.filesystem_id < 0
        || descriptor.fsmap_epoch == 0
        || descriptor.metadata_pool.pool_id < 0
    {
        return Err(CephFsObjectReadError::InvalidDescriptor);
    }
    Ok(())
}

pub(super) fn validate_sources(
    descriptor: &CephFsDescriptor,
    sources: &[CephFsObjectSource],
    expected_replica_count: usize,
) -> Result<(), CephFsObjectReadError> {
    if expected_replica_count == 0 || sources.len() < expected_replica_count {
        return Err(CephFsObjectReadError::CoverageNotClosed {
            expected: expected_replica_count,
            supplied: sources.len(),
        });
    }
    let mut source_ids = HashSet::new();
    let mut inventories = HashSet::new();
    for source in sources {
        if !source_ids.insert(source.data_source_id.0.as_str()) {
            return Err(CephFsObjectReadError::DuplicateSource {
                data_source_id: source.data_source_id.0.clone(),
            });
        }
        if !inventories.insert(source.inventory_id.as_str()) {
            return Err(CephFsObjectReadError::DuplicateInventory {
                inventory_id: source.inventory_id.clone(),
            });
        }
        let bound = descriptor.metadata_pool.provenance.iter().any(|bound| {
            bound.source_identity == source.data_source_id.0
                && bound.inventory_identity == source.inventory_id
        });
        if !bound {
            return Err(CephFsObjectReadError::InvalidSourceBinding);
        }
    }
    Ok(())
}

pub(super) fn validate_locator(
    descriptor: &CephFsDescriptor,
    locator: &CephFsObjectLocator,
) -> Result<(), CephFsObjectReadError> {
    if locator.filesystem_id() != descriptor.filesystem_id
        || locator.pool_id() != descriptor.metadata_pool.pool_id
        || locator.fsmap_epoch() != descriptor.fsmap_epoch
    {
        return Err(CephFsObjectReadError::LocatorMismatch {
            filesystem_identity: descriptor.identity.clone(),
        });
    }
    Ok(())
}

pub(super) fn validate_manifest(
    descriptor: &CephFsDescriptor,
    source: &CephFsObjectSource,
    semantic_sha256: &str,
    manifest: &CephFsMetadataInventoryManifest,
) -> Result<(), CephFsObjectReadError> {
    if !manifest.complete
        || manifest.filesystem_identity != descriptor.identity
        || manifest.inventory_id != source.inventory_id
        || manifest.data_source_id != source.data_source_id.0
        || manifest.filesystem_id != descriptor.filesystem_id
        || manifest.fsmap_epoch != descriptor.fsmap_epoch
        || manifest.metadata_pool_id != descriptor.metadata_pool.pool_id
        || manifest.schema_version != CEPHFS_METADATA_SCHEMA_VERSION
        || manifest.classifier_profile != CEPHFS_METADATA_CLASSIFIER_PROFILE
        || manifest.source_semantic_sha256 != semantic_sha256
        || !canonical_sha256(&manifest.source_semantic_sha256)
        || !canonical_sha256(&manifest.inventory_sha256)
        || manifest.unknown_object_count > manifest.object_count
    {
        return Err(CephFsObjectReadError::InventoryUnavailable {
            inventory_id: source.inventory_id.clone(),
        });
    }
    Ok(())
}

pub(super) fn validate_plan(
    locator: &CephFsObjectLocator,
    projection: &CephFsMetadataObjectProjection,
    plan: &CephBluestoreObjectReadPlan,
) -> Result<(), CephFsObjectReadError> {
    let object = &plan.object;
    if plan.object_identity_sha256 != projection.object_identity_sha256
        || object.object_identity_sha256 != projection.object_identity_sha256
        || object.decoded_pool != locator.pool_id()
        || object.object_namespace != locator.namespace()
        || object.object_name != locator.object_name()
        || object.snap_hex != CEPHFS_HEAD_SNAP_HEX
        || object.decode_status != "parsed"
        || object.deferred_reason.is_some()
        || !canonical_sha256(&projection.object_identity_sha256)
        || !canonical_sha256(&projection.record_sha256)
    {
        return Err(CephFsObjectReadError::MetadataConflict {
            locator: locator.canonical(),
        });
    }
    Ok(())
}

fn canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn validate_replica_metadata(
    locator: &str,
    replicas: &[ResolvedReplica],
) -> Result<u64, CephFsObjectReadError> {
    let first = replicas
        .first()
        .ok_or_else(|| CephFsObjectReadError::ObjectNotFound {
            locator: locator.to_string(),
        })?;
    if replicas.iter().skip(1).any(|replica| {
        replica.record_sha256 != first.record_sha256
            || replica.object_size != first.object_size
            || replica.provenance.object_identity_sha256 != first.provenance.object_identity_sha256
    }) {
        return Err(CephFsObjectReadError::MetadataConflict {
            locator: locator.to_string(),
        });
    }
    Ok(first.object_size)
}

pub(super) fn checked_range(
    locator: &CephFsObjectLocator,
    offset: u64,
    length: usize,
    object_size: u64,
) -> Result<(), CephFsObjectReadError> {
    let length = u64::try_from(length).map_err(|_| CephFsObjectReadError::RangeOverflow {
        locator: locator.canonical(),
    })?;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| CephFsObjectReadError::RangeOverflow {
            locator: locator.canonical(),
        })?;
    if end > object_size {
        return Err(CephFsObjectReadError::RangeOutOfBounds {
            locator: locator.canonical(),
            object_size,
        });
    }
    Ok(())
}
