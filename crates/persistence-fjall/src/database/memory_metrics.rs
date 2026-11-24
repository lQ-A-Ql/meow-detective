use super::SingleWriterTxDatabase;
use crate::KeyspaceName;
use fjall::AbstractTree;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyspaceMemoryMetrics {
    pub keyspace: KeyspaceName,
    pub active_memtable_bytes: u64,
    pub sealed_memtables: usize,
    pub retained_versions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseMemoryMetrics {
    /// Engine accounting for current active and queued memtables.
    pub write_buffer_bytes: u64,
    pub outstanding_flushes: usize,
    pub active_compactions: usize,
    pub keyspaces: Vec<KeyspaceMemoryMetrics>,
}

impl SingleWriterTxDatabase {
    pub fn memory_metrics(&self) -> DatabaseMemoryMetrics {
        let engine = self.inner.engine.inner();
        let mut keyspaces = self
            .inner
            .keyspaces
            .iter()
            .map(|(name, keyspace)| {
                let tree = &keyspace.inner().tree;
                KeyspaceMemoryMetrics {
                    keyspace: *name,
                    active_memtable_bytes: tree.active_memtable().size(),
                    sealed_memtables: tree.sealed_memtable_count(),
                    retained_versions: tree.get_version_history_lock().len(),
                }
            })
            .collect::<Vec<_>>();
        keyspaces.sort_unstable_by_key(|metrics| metrics.keyspace.as_str());
        DatabaseMemoryMetrics {
            write_buffer_bytes: engine.write_buffer_size(),
            outstanding_flushes: engine.outstanding_flushes(),
            active_compactions: engine.active_compactions(),
            keyspaces,
        }
    }
}
