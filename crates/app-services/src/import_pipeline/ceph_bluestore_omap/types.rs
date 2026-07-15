use std::collections::BTreeMap;

use ceph_wire::{BlueStoreOmapKey, BlueStoreOmapKeyFamily, BlueStoreOmapPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlueStoreOmapPoolScope {
    PerPool(i64),
    PerPg(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreOmapScope {
    pub family: BlueStoreOmapKeyFamily,
    pub pool: Option<BlueStoreOmapPoolScope>,
    pub hash: Option<u32>,
    pub nid: u64,
}

impl BlueStoreOmapScope {
    pub(super) fn from_key(key: &BlueStoreOmapKey<'_>) -> Self {
        let pool = match key.pool {
            Some(BlueStoreOmapPool::PerPool(value)) => Some(BlueStoreOmapPoolScope::PerPool(value)),
            Some(BlueStoreOmapPool::PerPg(value)) => Some(BlueStoreOmapPoolScope::PerPg(value)),
            None => None,
        };
        Self {
            family: key.family,
            pool,
            hash: key.hash,
            nid: key.nid,
        }
    }
}

impl PartialOrd for BlueStoreOmapScope {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BlueStoreOmapScope {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (family_rank(self.family), self.pool, self.hash, self.nid).cmp(&(
            family_rank(other.family),
            other.pool,
            other.hash,
            other.nid,
        ))
    }
}

fn family_rank(family: BlueStoreOmapKeyFamily) -> u8 {
    match family {
        BlueStoreOmapKeyFamily::Bulk => 0,
        BlueStoreOmapKeyFamily::PgMeta => 1,
        BlueStoreOmapKeyFamily::PerPool => 2,
        BlueStoreOmapKeyFamily::PerPg => 3,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreOmapLimits {
    pub max_scopes: usize,
    pub max_entries_per_scope: usize,
    pub max_owners: usize,
    pub max_retained_text_bytes: usize,
}

impl Default for BlueStoreOmapLimits {
    fn default() -> Self {
        Self {
            max_scopes: 1_000_000,
            max_entries_per_scope: 1_000_000,
            max_owners: 1_000_000,
            max_retained_text_bytes: 128 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStoreOmapOwnerKind {
    RbdDirectory,
    RbdHeader { image_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreOmapOwner {
    pub nid: u64,
    pub family: BlueStoreOmapKeyFamily,
    pub kind: BlueStoreOmapOwnerKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreOmapScopeRecord {
    pub scope: BlueStoreOmapScope,
    pub owner: Option<BlueStoreOmapOwner>,
    pub entry_count: u64,
    pub recognized_entry_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreRbdDirectoryMapping {
    pub scope: BlueStoreOmapScope,
    pub owner_nid: u64,
    pub image_name: String,
    pub image_id: String,
    pub bidirectional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreRbdHeader {
    pub scope: BlueStoreOmapScope,
    pub owner_nid: u64,
    pub image_id: String,
    pub size: Option<u64>,
    pub order: Option<u8>,
    pub features: Option<u64>,
    pub object_prefix: Option<String>,
    pub stripe_unit: Option<u64>,
    pub stripe_count: Option<u64>,
    pub data_pool_id: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlueStoreOmapSnapshot {
    pub scopes: Vec<BlueStoreOmapScopeRecord>,
    pub directory_mappings: Vec<BlueStoreRbdDirectoryMapping>,
    pub rbd_headers: Vec<BlueStoreRbdHeader>,
}

#[derive(Debug, Default)]
pub(super) struct DirectoryAccumulator {
    pub(super) name_to_id: BTreeMap<String, String>,
    pub(super) id_to_name: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub(super) struct HeaderAccumulator {
    pub(super) size: Option<u64>,
    pub(super) order: Option<u8>,
    pub(super) features: Option<u64>,
    pub(super) object_prefix: Option<String>,
    pub(super) stripe_unit: Option<u64>,
    pub(super) stripe_count: Option<u64>,
    pub(super) data_pool_id: Option<i64>,
}
