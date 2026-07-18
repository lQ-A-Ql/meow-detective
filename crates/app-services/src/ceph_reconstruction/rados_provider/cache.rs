use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::{RbdObjectProviderError, RbdObjectReadOutcome, RbdObjectReadRequest};
use crate::ceph_reconstruction::rbd_reader::RBD_READ_GRANULARITY_BYTES;

pub(super) const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 1024;
pub(super) const PAGE_BYTES: usize = RBD_READ_GRANULARITY_BYTES;
const CACHE_QUEUE_COMPACTION_FACTOR: usize = 4;

#[derive(Clone)]
pub(super) enum VerifiedObject {
    Present(Arc<[u8]>),
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VerifiedRangeKey {
    object_identity: String,
    page_offset: u64,
}

struct CachedVerifiedObject {
    value: VerifiedObject,
    generation: u64,
}

pub(super) struct VerifiedObjectCache {
    entries: HashMap<VerifiedRangeKey, CachedVerifiedObject>,
    access_order: VecDeque<(VerifiedRangeKey, u64)>,
    total_bytes: usize,
    max_bytes: usize,
    max_entries: usize,
    generation: u64,
}

impl VerifiedObjectCache {
    pub(super) fn for_rbd() -> Self {
        Self::new(MAX_BYTES, MAX_ENTRIES)
    }

    pub(super) fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            access_order: VecDeque::new(),
            total_bytes: 0,
            max_bytes,
            max_entries,
            generation: 0,
        }
    }

    pub(super) fn get(
        &mut self,
        object_identity: &str,
        page_offset: u64,
    ) -> Option<VerifiedObject> {
        let key = VerifiedRangeKey {
            object_identity: object_identity.to_string(),
            page_offset,
        };
        let generation = self.next_generation();
        let value = {
            let cached = self.entries.get_mut(&key)?;
            cached.generation = generation;
            cached.value.clone()
        };
        self.access_order.push_back((key, generation));
        self.compact_access_order_if_needed();
        Some(value)
    }

    pub(super) fn contains(&self, object_identity: &str, page_offset: u64) -> bool {
        self.entries.contains_key(&VerifiedRangeKey {
            object_identity: object_identity.to_string(),
            page_offset,
        })
    }

    pub(super) fn insert(
        &mut self,
        object_identity: &str,
        page_offset: u64,
        value: VerifiedObject,
    ) {
        let key = VerifiedRangeKey {
            object_identity: object_identity.to_string(),
            page_offset,
        };
        let value_size = verified_object_size(&value);
        if value_size > self.max_bytes || self.max_entries == 0 {
            return;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.total_bytes = self
                .total_bytes
                .saturating_sub(verified_object_size(&previous.value));
        }
        self.total_bytes = self.total_bytes.saturating_add(value_size);
        let generation = self.next_generation();
        self.access_order.push_back((key.clone(), generation));
        self.entries
            .insert(key, CachedVerifiedObject { value, generation });
        self.evict();
        self.compact_access_order_if_needed();
    }

    fn evict(&mut self) {
        while self.total_bytes > self.max_bytes || self.entries.len() > self.max_entries {
            let Some((oldest, generation)) = self.access_order.pop_front() else {
                break;
            };
            let is_current = self
                .entries
                .get(&oldest)
                .is_some_and(|cached| cached.generation == generation);
            if !is_current {
                continue;
            }
            if let Some(removed) = self.entries.remove(&oldest) {
                self.total_bytes = self
                    .total_bytes
                    .saturating_sub(verified_object_size(&removed.value));
            }
        }
    }

    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }

    fn compact_access_order_if_needed(&mut self) {
        let limit = self
            .entries
            .len()
            .saturating_mul(CACHE_QUEUE_COMPACTION_FACTOR)
            .max(128);
        if self.access_order.len() <= limit {
            return;
        }
        let mut current = self
            .entries
            .iter()
            .map(|(key, cached)| (cached.generation, key.clone()))
            .collect::<Vec<_>>();
        current.sort_by_key(|(generation, _)| *generation);
        self.access_order = current
            .into_iter()
            .map(|(generation, key)| (key, generation))
            .collect();
    }
}

pub(super) fn copy_verified_segment(
    request: &RbdObjectReadRequest,
    page_offset: u64,
    output: &mut [u8],
    verified: &VerifiedObject,
) -> Result<RbdObjectReadOutcome, RbdObjectProviderError> {
    let VerifiedObject::Present(bytes) = verified else {
        return Ok(RbdObjectReadOutcome::Missing);
    };
    let segment_start = usize::try_from(request.object_offset.saturating_sub(page_offset))
        .map_err(|_| RbdObjectProviderError::ReadFailed {
            object_identity: request.object_identity.clone(),
            reason: "RBD page offset does not fit in memory".to_string(),
        })?;
    let segment_end = segment_start.checked_add(output.len()).ok_or_else(|| {
        RbdObjectProviderError::ReadFailed {
            object_identity: request.object_identity.clone(),
            reason: "RBD page range overflow".to_string(),
        }
    })?;
    let source = bytes.get(segment_start..segment_end).ok_or_else(|| {
        RbdObjectProviderError::ReadFailed {
            object_identity: request.object_identity.clone(),
            reason: "RBD request exceeds the verified page".to_string(),
        }
    })?;
    if source.len() != output.len() {
        return Err(RbdObjectProviderError::ReadFailed {
            object_identity: request.object_identity.clone(),
            reason: "verified RBD page returned a short range".to_string(),
        });
    }
    output.copy_from_slice(source);
    Ok(RbdObjectReadOutcome::Present {
        object_identity: request.object_identity.clone(),
        bytes_read: output.len(),
    })
}

fn verified_object_size(value: &VerifiedObject) -> usize {
    match value {
        VerifiedObject::Present(bytes) => bytes.len(),
        VerifiedObject::Missing => 0,
    }
}
