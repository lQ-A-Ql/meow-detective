use crate::{
    errors::{FjallStorageError, Result},
    instrumentation::{
        record_database_open, record_persist, record_point_read, record_write_batch,
    },
    keyspaces::{open_keyspaces, KeyspaceName},
};
use fjall::{Readable, SingleWriterTxKeyspace};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
mod write_stream;
pub(crate) use write_stream::WriteStream;

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
            worker_threads: 2,
            max_journaling_size_bytes: 512 * 1024 * 1024,
            manual_journal_persist: true,
        }
    }
}

#[derive(Clone)]
pub struct SingleWriterTxDatabase {
    pub(crate) inner: Arc<DatabaseState>,
}

pub(crate) struct DatabaseState {
    keyspaces: Arc<HashMap<KeyspaceName, SingleWriterTxKeyspace>>,
    engine: fjall::SingleWriterTxDatabase,
    path: PathBuf,
    immutable_epoch: AtomicU64,
    immutable_gate: Mutex<()>,
    immutable_sealed: AtomicBool,
}

impl SingleWriterTxDatabase {
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let builder = fjall::SingleWriterTxDatabase::builder(&path)
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
                immutable_gate: Mutex::new(()),
                immutable_sealed: AtomicBool::new(false),
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
            .immutable_gate
            .lock()
            .map_err(|_| FjallStorageError::LockPoisoned)?;
        if self.immutable_epoch() != epoch {
            return Err(FjallStorageError::InvalidEnvelope(
                "verified immutable data changed".into(),
            ));
        }
        let value = seal()?;
        self.inner.immutable_sealed.store(true, Ordering::Release);
        Ok(value)
    }

    fn check_immutable_write(&self, changes: bool) -> Result<()> {
        if changes && self.inner.immutable_sealed.load(Ordering::Acquire) {
            return Err(FjallStorageError::InvalidEnvelope(
                "verified generation is immutable".into(),
            ));
        }
        Ok(())
    }

    fn keyspace(&self, name: KeyspaceName) -> &SingleWriterTxKeyspace {
        self.inner
            .keyspaces
            .get(&name)
            .expect("configured keyspace is present")
    }

    pub fn keyspace_names(&self) -> impl Iterator<Item = KeyspaceName> + '_ {
        self.inner.keyspaces.keys().copied()
    }

    pub fn write_batch(
        &self,
        actions: &[WriteAction],
        mode: PersistMode,
    ) -> Result<WriteBatchReceipt> {
        let _guard = self
            .inner
            .immutable_gate
            .lock()
            .map_err(|_| FjallStorageError::LockPoisoned)?;
        self.check_immutable_write(actions.iter().any(immutable_action))?;
        let batch_digest = digest_actions(actions);
        let receipt_key = [b"batch/".as_slice(), batch_digest.as_slice()].concat();
        let database = self.inner.engine.inner().clone();
        let mut batch = database.batch().durability(Some(mode.into()));
        let mut action_bytes = 0usize;
        for action in actions {
            match action {
                WriteAction::Put {
                    keyspace,
                    key,
                    value,
                } => {
                    action_bytes += key.len() + value.len();
                    batch.insert(self.keyspace(*keyspace).inner(), key.clone(), value.clone())
                }
                WriteAction::Delete { keyspace, key } => {
                    action_bytes += key.len();
                    batch.remove(self.keyspace(*keyspace).inner(), key.clone())
                }
            }
        }
        batch.insert(
            self.keyspace(KeyspaceName::Batches).inner(),
            receipt_key,
            actions.len().to_be_bytes().to_vec(),
        );
        batch.commit()?;
        if actions.iter().any(immutable_action) {
            self.inner.immutable_epoch.fetch_add(1, Ordering::Release);
        }
        record_write_batch(
            actions.len(),
            action_bytes,
            matches!(mode, PersistMode::SyncAll),
        );
        Ok(WriteBatchReceipt {
            operation_count: actions.len(),
            batch_digest,
        })
    }

    pub fn persist(&self, mode: PersistMode) -> Result<()> {
        self.inner
            .engine
            .persist(mode.into())
            .map_err(FjallStorageError::from)?;
        record_persist(matches!(mode, PersistMode::SyncAll));
        Ok(())
    }

    pub(crate) fn write_planned(
        &self,
        mode: PersistMode,
        plan: impl FnOnce(&SourceReadSnapshot) -> Result<Vec<WriteAction>>,
    ) -> Result<WriteBatchReceipt> {
        let _guard = self
            .inner
            .immutable_gate
            .lock()
            .map_err(|_| FjallStorageError::LockPoisoned)?;
        let mut transaction = self.inner.engine.write_tx().durability(Some(mode.into()));
        let actions = plan(&self.read_snapshot())?;
        self.check_immutable_write(actions.iter().any(immutable_action))?;
        let batch_digest = digest_actions(&actions);
        if actions.is_empty() {
            return Ok(WriteBatchReceipt {
                operation_count: 0,
                batch_digest,
            });
        }
        let mut action_bytes = 0usize;
        for action in &actions {
            match action {
                WriteAction::Put {
                    keyspace,
                    key,
                    value,
                } => {
                    action_bytes += key.len() + value.len();
                    transaction.insert(self.keyspace(*keyspace), key.clone(), value.clone())
                }
                WriteAction::Delete { keyspace, key } => {
                    action_bytes += key.len();
                    transaction.remove(self.keyspace(*keyspace), key.clone())
                }
            }
        }
        transaction.insert(
            self.keyspace(KeyspaceName::Batches),
            [b"batch/".as_slice(), batch_digest.as_slice()].concat(),
            actions.len().to_be_bytes().to_vec(),
        );
        transaction.commit()?;
        if actions.iter().any(immutable_action) {
            self.inner.immutable_epoch.fetch_add(1, Ordering::Release);
        }
        record_write_batch(
            actions.len(),
            action_bytes,
            matches!(mode, PersistMode::SyncAll),
        );
        Ok(WriteBatchReceipt {
            operation_count: actions.len(),
            batch_digest,
        })
    }

    pub(crate) fn write_streamed<T, E>(
        &self,
        mode: PersistMode,
        plan: impl FnOnce(&SourceReadSnapshot, &mut WriteStream<'_>) -> std::result::Result<T, E>,
    ) -> std::result::Result<(T, WriteBatchReceipt), E>
    where
        E: From<FjallStorageError>,
    {
        let _guard = self
            .inner
            .immutable_gate
            .lock()
            .map_err(|_| E::from(FjallStorageError::LockPoisoned))?;
        let transaction = self.inner.engine.write_tx().durability(Some(mode.into()));
        let snapshot = self.read_snapshot();
        let mut stream = WriteStream::new(
            transaction,
            &self.inner.keyspaces,
            mode,
            &self.inner.immutable_epoch,
            &self.inner.immutable_sealed,
        );
        let value = plan(&snapshot, &mut stream)?;
        let receipt = stream.finish().map_err(E::from)?;
        Ok((value, receipt))
    }

    pub fn read_snapshot(&self) -> SourceReadSnapshot {
        SourceReadSnapshot {
            snapshot: self.inner.engine.read_tx(),
            keyspaces: Arc::clone(&self.inner.keyspaces),
            _owner: self.clone(),
        }
    }
}

fn immutable_action(action: &WriteAction) -> bool {
    let keyspace = match action {
        WriteAction::Put { keyspace, .. } | WriteAction::Delete { keyspace, .. } => *keyspace,
    };
    immutable_keyspace(keyspace)
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

impl SingleWriterTxDatabase {
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

fn digest_actions(actions: &[WriteAction]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for action in actions {
        hash_action(&mut hasher, action);
    }
    hasher.finalize().into()
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
    pub(crate) snapshot: fjall::Snapshot,
    pub(crate) keyspaces: Arc<HashMap<KeyspaceName, SingleWriterTxKeyspace>>,
    _owner: SingleWriterTxDatabase,
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
