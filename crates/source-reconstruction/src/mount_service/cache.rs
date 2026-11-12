use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use domain::FileEntry;
use evidence_mount::MountPath;

pub(crate) const DEFAULT_METADATA_CACHE_ENTRIES: usize = 4096;

pub(crate) struct MountMetadataCache {
    capacity: usize,
    entries: HashMap<MountPath, Arc<FileEntry>>,
    insertion_order: VecDeque<MountPath>,
}

impl MountMetadataCache {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::with_capacity(capacity),
            insertion_order: VecDeque::with_capacity(capacity),
        }
    }

    pub(crate) fn get(&self, path: &MountPath) -> Option<Arc<FileEntry>> {
        self.entries.get(path).cloned()
    }

    pub(crate) fn insert(&mut self, path: MountPath, entry: Arc<FileEntry>) {
        if let Some(existing) = self.entries.get_mut(&path) {
            *existing = entry;
            return;
        }
        while self.entries.len() >= self.capacity {
            let Some(evicted) = self.insertion_order.pop_front() else {
                break;
            };
            self.entries.remove(&evicted);
        }
        self.insertion_order.push_back(path.clone());
        self.entries.insert(path, entry);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/cache.rs"]
mod tests;
