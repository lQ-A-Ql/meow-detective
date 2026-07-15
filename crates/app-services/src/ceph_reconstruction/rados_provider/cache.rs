use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::{RbdObjectProviderError, RbdObjectReadOutcome, RbdObjectReadRequest};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 1024;
pub(super) const PAGE_BYTES: usize = 64 * 1024;

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

pub(super) struct VerifiedObjectCache {
    entries: HashMap<VerifiedRangeKey, VerifiedObject>,
    lru: VecDeque<VerifiedRangeKey>,
    total_bytes: usize,
    max_bytes: usize,
    max_entries: usize,
}

impl VerifiedObjectCache {
    pub(super) fn for_rbd() -> Self {
        Self::new(MAX_BYTES, MAX_ENTRIES)
    }

    pub(super) fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            lru: VecDeque::new(),
            total_bytes: 0,
            max_bytes,
            max_entries,
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
        let value = self.entries.get(&key)?.clone();
        self.touch(&key);
        Some(value)
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
                .saturating_sub(verified_object_size(&previous));
        }
        self.lru.retain(|entry| entry != &key);
        self.total_bytes = self.total_bytes.saturating_add(value_size);
        self.lru.push_back(key.clone());
        self.entries.insert(key, value);
        self.evict();
    }

    fn touch(&mut self, key: &VerifiedRangeKey) {
        self.lru.retain(|entry| entry != key);
        self.lru.push_back(key.clone());
    }

    fn evict(&mut self) {
        while self.total_bytes > self.max_bytes || self.entries.len() > self.max_entries {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.total_bytes = self
                    .total_bytes
                    .saturating_sub(verified_object_size(&removed));
            }
        }
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
