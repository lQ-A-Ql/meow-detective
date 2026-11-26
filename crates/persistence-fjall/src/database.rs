use crate::{
    errors::{FjallStorageError, Result},
    instrumentation::{
        record_database_open, record_persist, record_point_read, record_write_batch,
    },
    keyspaces::{open_keyspaces, KeyspaceName},
};
use fjall::Keyspace;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, RwLock,
    },
};
mod memory_metrics;
mod native_write;
mod read_view;
mod transaction;
mod write_session;
mod write_stream;
pub use memory_metrics::{DatabaseMemoryMetrics, KeyspaceMemoryMetrics};
pub use read_view::SourceReadHandle;
pub(crate) use read_view::TransactionReadView;
pub use write_session::{
    NativeWriteInput, NativeWriteReport, SourceWriteSession, WriteSessionConfig,
};
pub(crate) use write_stream::WriteStream;
pub(crate) const NATIVE_WRITE_INCOMPLETE_FILE: &str = "native-write-incomplete";

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub cache_size_bytes: u64,
    pub worker_threads: usize,
    pub max_journaling_size_bytes: u64,
    pub manual_journal_persist: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistMode {
    Buffer,
    SyncData,
    SyncAll,
}

impl From<PersistMode> for fjall::PersistMode {
    fn from(mode: PersistMode) -> Self {
        match mode {
            PersistMode::Buffer => Self::Buffer,
            PersistMode::SyncData => Self::SyncData,
            PersistMode::SyncAll => Self::SyncAll,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            cache_size_bytes: 32 * 1024 * 1024,
            worker_threads: 4,
            max_journaling_size_bytes: 512 * 1024 * 1024,
            manual_journal_persist: true,
        }
    }
}

#[derive(Clone)]
pub struct SourceDatabase {
    pub(crate) inner: Arc<DatabaseState>,
}

pub(crate) struct DatabaseState {
    keyspaces: Arc<HashMap<KeyspaceName, Keyspace>>,
    engine: fjall::OptimisticTxDatabase,
    path: PathBuf,
    immutable_epoch: AtomicU64,
    lifecycle: RwLock<()>,
    immutable_sealed: AtomicBool,
    session_active: AtomicBool,
    session_failed: AtomicBool,
}

impl SourceDatabase {
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self> {
        std::fs::create_dir_all(path.as_ref())?;
        let path = path.as_ref().canonicalize()?;
        if path.join(NATIVE_WRITE_INCOMPLETE_FILE).exists() {
            return Err(FjallStorageError::WriteSessionFailed);
        }
        let builder = fjall::OptimisticTxDatabase::builder(&path)
            .cache_size(config.cache_size_bytes)
            .worker_threads(config.worker_threads)
            .max_journaling_size(config.max_journaling_size_bytes)
            .manual_journal_persist(config.manual_journal_persist);
        let inner = builder.open().map_err(|error| match error {
            fjall::Error::Locked => FjallStorageError::StorageLocked { path: path.clone() },
            other => FjallStorageError::Engine(other),
        })?;
        let keyspaces = open_keyspaces(&inner)?.into_iter().collect();
        record_database_open();
        Ok(Self {
            inner: Arc::new(DatabaseState {
                engine: inner,
                path,
                keyspaces: Arc::new(keyspaces),
                immutable_epoch: AtomicU64::new(0),
                lifecycle: RwLock::new(()),
                immutable_sealed: AtomicBool::new(false),
                session_active: AtomicBool::new(false),
                session_failed: AtomicBool::new(false),
            }),
        })
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub(crate) fn immutable_epoch(&self) -> u64 {
        self.inner.immutable_epoch.load(Ordering::Acquire)
    }

    pub(crate) fn seal_immutable<T>(
        &self,
        epoch: u64,
        seal: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let _guard = self
            .inner
            .lifecycle
            .write()
            .map_err(|_| FjallStorageError::LockPoisoned)?;
        self.ensure_regular_write()?;
        if self.immutable_epoch() != epoch {
            return Err(FjallStorageError::InvalidEnvelope(
                "verified immutable data changed".into(),
            ));
        }
        let value = seal()?;
        self.inner.immutable_sealed.store(true, Ordering::Release);
        Ok(value)
    }

    pub fn keyspace_names(&self) -> impl Iterator<Item = KeyspaceName> + '_ {
        self.inner.keyspaces.keys().copied()
    }

    pub fn write_batch(
        &self,
        actions: &[WriteAction],
        mode: PersistMode,
    ) -> Result<WriteBatchReceipt> {
        self.write_transaction(mode, |_| Ok(actions.to_vec()))
    }

    pub fn persist(&self, mode: PersistMode) -> Result<()> {
        self.inner
            .engine
            .persist(mode.into())
            .map_err(FjallStorageError::from)?;
        record_persist(matches!(mode, PersistMode::SyncAll));
        Ok(())
    }

    pub fn read_snapshot(&self) -> SourceReadSnapshot {
        SourceReadSnapshot {
            snapshot: read_view::ReadView::Snapshot(self.inner.engine.read_tx()),
            keyspaces: Arc::clone(&self.inner.keyspaces),
            _owner: self.clone(),
        }
    }

    pub(crate) fn ensure_regular_write(&self) -> Result<()> {
        if self.inner.session_failed.load(Ordering::Acquire) {
            return Err(FjallStorageError::WriteSessionFailed);
        }
        if self.inner.session_active.load(Ordering::Acquire) {
            return Err(FjallStorageError::WriteSessionBusy);
        }
        Ok(())
    }
}

fn immutable_keyspace(keyspace: KeyspaceName) -> bool {
    matches!(
        keyspace,
        KeyspaceName::CatalogRecords
            | KeyspaceName::CatalogIndexes
            | KeyspaceName::PartitionRecords
            | KeyspaceName::PartitionIndexes
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteAction {
    Put {
        keyspace: KeyspaceName,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        keyspace: KeyspaceName,
        key: Vec<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteBatchReceipt {
    pub operation_count: usize,
    pub batch_digest: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchReceiptState {
    Present { operation_count: usize },
    Missing,
    Corrupt,
}

impl SourceDatabase {
    pub fn batch_receipt_state_for_digest(&self, digest: &[u8; 32]) -> Result<BatchReceiptState> {
        let key = [b"batch/".as_slice(), digest.as_slice()].concat();
        let snapshot = self.read_snapshot();
        let Some(value) = snapshot.get(KeyspaceName::Batches, &key)? else {
            return Ok(BatchReceiptState::Missing);
        };
        if value.len() != std::mem::size_of::<usize>() {
            return Ok(BatchReceiptState::Corrupt);
        }
        let mut encoded = [0u8; std::mem::size_of::<usize>()];
        encoded.copy_from_slice(&value);
        Ok(BatchReceiptState::Present {
            operation_count: usize::from_be_bytes(encoded),
        })
    }

    pub fn batch_receipt_state(&self, receipt: &WriteBatchReceipt) -> Result<BatchReceiptState> {
        match self.batch_receipt_state_for_digest(&receipt.batch_digest)? {
            BatchReceiptState::Present { operation_count }
                if operation_count == receipt.operation_count =>
            {
                Ok(BatchReceiptState::Present { operation_count })
            }
            BatchReceiptState::Present { .. } => Ok(BatchReceiptState::Corrupt),
            other => Ok(other),
        }
    }
}

fn hash_action(hasher: &mut Sha256, action: &WriteAction) {
    match action {
        WriteAction::Put {
            keyspace,
            key,
            value,
        } => {
            hasher.update([0u8, *keyspace as u8]);
            hasher.update((key.len() as u64).to_be_bytes());
            hasher.update(key);
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value);
        }
        WriteAction::Delete { keyspace, key } => {
            hasher.update([1u8, *keyspace as u8]);
            hasher.update((key.len() as u64).to_be_bytes());
            hasher.update(key);
        }
    }
}

pub struct SourceReadSnapshot {
    pub(crate) snapshot: read_view::ReadView,
    pub(crate) keyspaces: Arc<HashMap<KeyspaceName, Keyspace>>,
    _owner: SourceDatabase,
}

impl SourceReadSnapshot {
    pub fn get(&self, keyspace: KeyspaceName, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let value = self
            .snapshot
            .get(
                self.keyspaces
                    .get(&keyspace)
                    .expect("configured keyspace is present"),
                key,
            )
            .map(|value| value.map(|bytes| bytes.to_vec()))
            .map_err(FjallStorageError::from)?;
        if let Some(bytes) = value.as_ref() {
            record_point_read(bytes.len());
        }
        Ok(value)
    }
}

#[cfg(test)]
#[path = "../tests/unit/database_immutable_seal.rs"]
mod tests;
