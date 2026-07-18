use super::XfsDirectoryEntry;
use std::collections::{HashMap, VecDeque};
use std::mem::size_of;
use std::sync::Arc;

const MAX_DIRECTORY_ENTRY_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_DIRECTORY_ENTRY_CACHE_ITEMS: usize = 32_768;
const CACHE_QUEUE_COMPACTION_FACTOR: usize = 4;

struct CachedDirectoryEntries {
    entries: Arc<Vec<XfsDirectoryEntry>>,
    bytes: usize,
    generation: u64,
}

#[derive(Default)]
pub(crate) struct BoundedDirectoryEntryCache {
    entries: HashMap<u64, CachedDirectoryEntries>,
    access_order: VecDeque<(u64, u64)>,
    bytes: usize,
    generation: u64,
}

impl BoundedDirectoryEntryCache {
    pub(crate) fn get(&mut self, inode: u64) -> Option<Arc<Vec<XfsDirectoryEntry>>> {
        let generation = self.next_generation();
        let entries = {
            let cached = self.entries.get_mut(&inode)?;
            cached.generation = generation;
            Arc::clone(&cached.entries)
        };
        self.access_order.push_back((inode, generation));
        self.compact_access_order_if_needed();
        Some(entries)
    }

    pub(crate) fn insert(&mut self, inode: u64, entries: Arc<Vec<XfsDirectoryEntry>>) {
        if self.entries.contains_key(&inode) {
            return;
        }
        let bytes = directory_entries_size(&entries);
        if bytes > MAX_DIRECTORY_ENTRY_CACHE_BYTES {
            return;
        }
        self.evict_for(bytes);
        if self.entries.len() >= MAX_DIRECTORY_ENTRY_CACHE_ITEMS
            || self.bytes.saturating_add(bytes) > MAX_DIRECTORY_ENTRY_CACHE_BYTES
        {
            return;
        }

        let generation = self.next_generation();
        self.entries.insert(
            inode,
            CachedDirectoryEntries {
                entries,
                bytes,
                generation,
            },
        );
        self.access_order.push_back((inode, generation));
        self.bytes += bytes;
        self.compact_access_order_if_needed();
    }

    fn evict_for(&mut self, incoming_bytes: usize) {
        while self.entries.len() >= MAX_DIRECTORY_ENTRY_CACHE_ITEMS
            || self.bytes.saturating_add(incoming_bytes) > MAX_DIRECTORY_ENTRY_CACHE_BYTES
        {
            let Some((inode, generation)) = self.access_order.pop_front() else {
                break;
            };
            let is_current = self
                .entries
                .get(&inode)
                .is_some_and(|cached| cached.generation == generation);
            if !is_current {
                continue;
            }
            if let Some(removed) = self.entries.remove(&inode) {
                self.bytes = self.bytes.saturating_sub(removed.bytes);
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
            .map(|(inode, cached)| (cached.generation, *inode))
            .collect::<Vec<_>>();
        current.sort_unstable();
        self.access_order = current
            .into_iter()
            .map(|(generation, inode)| (inode, generation))
            .collect();
    }
}

fn directory_entries_size(entries: &[XfsDirectoryEntry]) -> usize {
    size_of::<Vec<XfsDirectoryEntry>>()
        .saturating_add(entries.len().saturating_mul(size_of::<XfsDirectoryEntry>()))
        .saturating_add(
            entries
                .iter()
                .map(|entry| entry.name.capacity())
                .sum::<usize>(),
        )
}
