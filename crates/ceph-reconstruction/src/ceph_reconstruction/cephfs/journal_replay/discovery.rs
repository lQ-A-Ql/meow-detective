use std::collections::{BTreeMap, BTreeSet};

use ceph_wire::{classify_cephfs_metadata_object_name, CephFsMetadataObjectClass};
use thiserror::Error;

use super::super::{
    CephFsDescriptor, CephFsDescriptorState, CephFsMergedMetadataInventory, CephFsObjectLocator,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalRankCandidate {
    pub rank: u32,
    pub gid: u64,
    pub incarnation: i32,
    pub pointer_locator: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CephFsJournalDiscoveryError {
    #[error("CephFS metadata inventory is not bound to the filesystem descriptor")]
    InvalidInventoryBinding,
    #[error("CephFS journal rank binding is duplicated or invalid: {rank}")]
    InvalidRankBinding { rank: u32 },
    #[error("CephFS journal pointer is missing for current rank {rank}")]
    MissingPointer { rank: u32 },
    #[error("CephFS journal pointer is duplicated for current rank {rank}")]
    DuplicatePointer { rank: u32 },
    #[error("CephFS journal pointer locator is invalid")]
    InvalidPointerLocator,
}

impl transport::ServiceErrorCategory for CephFsJournalDiscoveryError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::InvalidRankBinding { .. } => transport::ErrorCategory::Validation,
            Self::InvalidInventoryBinding
            | Self::MissingPointer { .. }
            | Self::DuplicatePointer { .. }
            | Self::InvalidPointerLocator => transport::ErrorCategory::Parser,
        }
    }
}

pub fn discover_cephfs_journal_ranks(
    descriptor: &CephFsDescriptor,
    inventory: &CephFsMergedMetadataInventory,
) -> Result<Vec<CephFsJournalRankCandidate>, CephFsJournalDiscoveryError> {
    validate_inventory_binding(descriptor, inventory)?;
    let bindings = validate_rank_bindings(descriptor)?;
    let mut pointers = BTreeMap::new();
    for object in &inventory.objects {
        let locator = CephFsObjectLocator::parse(&object.locator)
            .map_err(|_| CephFsJournalDiscoveryError::InvalidPointerLocator)?;
        let (class, _) = classify_cephfs_metadata_object_name(locator.object_name());
        let CephFsMetadataObjectClass::JournalPointer { rank } = class else {
            continue;
        };
        if !bindings.contains_key(&rank) {
            continue;
        }
        if pointers.insert(rank, object.locator.clone()).is_some() {
            return Err(CephFsJournalDiscoveryError::DuplicatePointer { rank });
        }
    }
    bindings
        .into_iter()
        .map(|(rank, (gid, incarnation))| {
            let pointer_locator = pointers
                .remove(&rank)
                .ok_or(CephFsJournalDiscoveryError::MissingPointer { rank })?;
            Ok(CephFsJournalRankCandidate {
                rank,
                gid,
                incarnation,
                pointer_locator,
            })
        })
        .collect()
}

fn validate_inventory_binding(
    descriptor: &CephFsDescriptor,
    inventory: &CephFsMergedMetadataInventory,
) -> Result<(), CephFsJournalDiscoveryError> {
    if inventory.filesystem_identity != descriptor.identity
        || inventory.filesystem_id != descriptor.filesystem_id
        || inventory.fsmap_epoch != descriptor.fsmap_epoch
        || inventory.metadata_pool_id != descriptor.metadata_pool.pool_id
    {
        return Err(CephFsJournalDiscoveryError::InvalidInventoryBinding);
    }
    Ok(())
}

fn validate_rank_bindings(
    descriptor: &CephFsDescriptor,
) -> Result<BTreeMap<u32, (u64, i32)>, CephFsJournalDiscoveryError> {
    if descriptor.state != CephFsDescriptorState::Present {
        return Err(CephFsJournalDiscoveryError::InvalidRankBinding { rank: 0 });
    }
    let mut bindings = BTreeMap::new();
    let mut gids = BTreeSet::new();
    for binding in &descriptor.rank_bindings {
        if binding.rank >= 0x100 || binding.incarnation < 0 {
            return Err(CephFsJournalDiscoveryError::InvalidRankBinding { rank: binding.rank });
        }
        let daemon_rank = binding.rank as i32;
        let active_daemon_count = descriptor
            .daemons
            .iter()
            .filter(|daemon| {
                daemon.rank == daemon_rank
                    && daemon.gid == binding.gid
                    && daemon.incarnation == binding.incarnation
                    && daemon.state.is_active()
            })
            .count();
        if active_daemon_count != 1
            || !gids.insert(binding.gid)
            || bindings
                .insert(binding.rank, (binding.gid, binding.incarnation))
                .is_some()
        {
            return Err(CephFsJournalDiscoveryError::InvalidRankBinding { rank: binding.rank });
        }
    }
    if bindings.is_empty() {
        return Err(CephFsJournalDiscoveryError::InvalidRankBinding { rank: 0 });
    }
    Ok(bindings)
}
