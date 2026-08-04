use std::collections::{HashMap, VecDeque};
use std::mem::size_of;
use std::sync::Arc;

use domain::FileEntry;
use evidence_mount::{MountNode, MountPath};

pub(crate) const DEFAULT_DIRECTORY_CACHE_BYTES: usize = 64 * 1024 * 1024;

pub(crate) struct DirectorySnapshot {
    entries: Arc<[MountNode]>,
    catalog_entries: HashMap<MountPath, Arc<FileEntry>>,
    weight: usize,
}

impl DirectorySnapshot {
    pub(crate) fn new(entries: Vec<(MountNode, Arc<FileEntry>)>) -> Self {
        let mut weight = size_of::<Self>();
        let mut nodes = Vec::with_capacity(entries.len());
        let mut catalog_entries = HashMap::with_capacity(entries.len());
        for (node, entry) in entries {
            weight = weight
                .saturating_add(size_of::<MountNode>())
                .saturating_add(node.path.as_str().len())
                .saturating_add(node.name.len())
                .saturating_add(node.source_file_id.as_ref().map_or(0, String::len))
                .saturating_add(file_entry_weight(&entry));
            catalog_entries.insert(node.path.clone(), entry);
            nodes.push(node);
        }
        Self {
            entries: nodes.into(),
            catalog_entries,
            weight,
        }
    }

    pub(crate) fn page(&self, offset: usize, limit: usize) -> &[MountNode] {
        let start = offset.min(self.entries.len());
        let end = start.saturating_add(limit).min(self.entries.len());
        &self.entries[start..end]
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

pub(crate) struct DirectorySnapshotCache {
    capacity_bytes: usize,
    current_bytes: usize,
    snapshots: HashMap<MountPath, Arc<DirectorySnapshot>>,
    entries_by_path: HashMap<MountPath, Arc<FileEntry>>,
    insertion_order: VecDeque<MountPath>,
}

impl DirectorySnapshotCache {
    pub(crate) fn new(capacity_bytes: usize) -> Self {
        Self {
            capacity_bytes: capacity_bytes.max(1),
            current_bytes: 0,
            snapshots: HashMap::new(),
            entries_by_path: HashMap::new(),
            insertion_order: VecDeque::new(),
        }
    }

    pub(crate) fn get(&self, path: &MountPath) -> Option<Arc<DirectorySnapshot>> {
        self.snapshots.get(path).cloned()
    }

    pub(crate) fn find_entry(&self, path: &MountPath) -> Option<Arc<FileEntry>> {
        self.entries_by_path.get(path).cloned()
    }

    pub(crate) fn insert(
        &mut self,
        path: MountPath,
        snapshot: Arc<DirectorySnapshot>,
    ) -> Arc<DirectorySnapshot> {
        if let Some(existing) = self.snapshots.get(&path) {
            return Arc::clone(existing);
        }
        while self.current_bytes.saturating_add(snapshot.weight) > self.capacity_bytes {
            let Some(evicted_path) = self.insertion_order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.snapshots.remove(&evicted_path) {
                self.current_bytes = self.current_bytes.saturating_sub(evicted.weight);
                for (entry_path, entry) in &evicted.catalog_entries {
                    let should_remove = self
                        .entries_by_path
                        .get(entry_path)
                        .is_some_and(|cached| Arc::ptr_eq(cached, entry));
                    if should_remove {
                        self.entries_by_path.remove(entry_path);
                    }
                }
            }
        }
        if snapshot.weight <= self.capacity_bytes {
            self.current_bytes = self.current_bytes.saturating_add(snapshot.weight);
            self.insertion_order.push_back(path.clone());
            for (entry_path, entry) in &snapshot.catalog_entries {
                self.entries_by_path
                    .insert(entry_path.clone(), Arc::clone(entry));
            }
            self.snapshots.insert(path, Arc::clone(&snapshot));
        }
        snapshot
    }
}

fn file_entry_weight(entry: &FileEntry) -> usize {
    size_of::<FileEntry>()
        .saturating_add(entry.id.0.len())
        .saturating_add(entry.parent_id.as_ref().map_or(0, |id| id.0.len()))
        .saturating_add(entry.data_source_id.0.len())
        .saturating_add(entry.path.len())
        .saturating_add(entry.name.len())
        .saturating_add(entry.ext.as_ref().map_or(0, String::len))
        .saturating_add(entry.hash_sha256.as_ref().map_or(0, String::len))
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/directory_cache.rs"]
mod tests;
