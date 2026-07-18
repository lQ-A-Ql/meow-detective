use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

const MAX_INODE_BLOCK_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_INODE_BLOCK_CACHE_ITEMS: usize = 8_192;
const CACHE_QUEUE_COMPACTION_FACTOR: usize = 4;

struct CachedInodeBlock {
    bytes: Arc<[u8]>,
    generation: u64,
}

#[derive(Default)]
pub(crate) struct BoundedInodeBlockCache {
    entries: HashMap<u64, CachedInodeBlock>,
    access_order: VecDeque<(u64, u64)>,
    bytes: usize,
    generation: u64,
}

impl BoundedInodeBlockCache {
    pub(crate) fn get(&mut self, block_offset: u64) -> Option<Arc<[u8]>> {
        let generation = self.next_generation();
        let bytes = {
            let cached = self.entries.get_mut(&block_offset)?;
            cached.generation = generation;
            Arc::clone(&cached.bytes)
        };
        self.access_order.push_back((block_offset, generation));
        self.compact_access_order_if_needed();
        Some(bytes)
    }

    pub(crate) fn insert(&mut self, block_offset: u64, bytes: Arc<[u8]>) {
        if self.entries.contains_key(&block_offset) || bytes.len() > MAX_INODE_BLOCK_CACHE_BYTES {
            return;
        }
        self.evict_for(bytes.len());
        if self.entries.len() >= MAX_INODE_BLOCK_CACHE_ITEMS
            || self.bytes.saturating_add(bytes.len()) > MAX_INODE_BLOCK_CACHE_BYTES
        {
            return;
        }

        let generation = self.next_generation();
        self.bytes = self.bytes.saturating_add(bytes.len());
        self.entries
            .insert(block_offset, CachedInodeBlock { bytes, generation });
        self.access_order.push_back((block_offset, generation));
        self.compact_access_order_if_needed();
    }

    fn evict_for(&mut self, incoming_bytes: usize) {
        while self.entries.len() >= MAX_INODE_BLOCK_CACHE_ITEMS
            || self.bytes.saturating_add(incoming_bytes) > MAX_INODE_BLOCK_CACHE_BYTES
        {
            let Some((block_offset, generation)) = self.access_order.pop_front() else {
                break;
            };
            let is_current = self
                .entries
                .get(&block_offset)
                .is_some_and(|cached| cached.generation == generation);
            if !is_current {
                continue;
            }
            if let Some(removed) = self.entries.remove(&block_offset) {
                self.bytes = self.bytes.saturating_sub(removed.bytes.len());
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
            .map(|(offset, cached)| (cached.generation, *offset))
            .collect::<Vec<_>>();
        current.sort_unstable();
        self.access_order = current
            .into_iter()
            .map(|(generation, offset)| (offset, generation))
            .collect();
    }
}
