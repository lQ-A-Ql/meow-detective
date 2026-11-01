use crate::{
    errors::{FjallStorageError, Result},
    keyspaces::{open_keyspaces, KeyspaceName},
};
use fjall::{Readable, SingleWriterTxKeyspace};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

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
    inner: Arc<fjall::SingleWriterTxDatabase>,
    path: PathBuf,
    keyspaces: Arc<HashMap<KeyspaceName, SingleWriterTxKeyspace>>,
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
        Ok(Self {
            inner: Arc::new(inner),
            path,
            keyspaces: Arc::new(keyspaces),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn keyspace(&self, name: KeyspaceName) -> &SingleWriterTxKeyspace {
        self.keyspaces
            .get(&name)
            .expect("configured keyspace is present")
    }

    pub fn keyspace_names(&self) -> impl Iterator<Item = KeyspaceName> + '_ {
        self.keyspaces.keys().copied()
    }

    pub fn write_batch(
        &self,
        actions: &[WriteAction],
        mode: PersistMode,
    ) -> Result<WriteBatchReceipt> {
        let batch_digest = digest_actions(actions);
        let receipt_key = [b"batch/".as_slice(), batch_digest.as_slice()].concat();
        let mut transaction = self.inner.write_tx().durability(Some(mode.into()));
        for action in actions {
            match action {
                WriteAction::Put {
                    keyspace,
                    key,
                    value,
                } => transaction.insert(self.keyspace(*keyspace), key.clone(), value.clone()),
                WriteAction::Delete { keyspace, key } => {
                    transaction.remove(self.keyspace(*keyspace), key.clone())
                }
            }
        }
        transaction.insert(
            self.keyspace(KeyspaceName::Batches),
            receipt_key,
            actions.len().to_be_bytes().to_vec(),
        );
        transaction.commit()?;
        Ok(WriteBatchReceipt {
            operation_count: actions.len(),
            batch_digest,
        })
    }

    pub fn persist(&self, mode: PersistMode) -> Result<()> {
        self.inner.persist(mode.into()).map_err(Into::into)
    }

    pub fn read_snapshot(&self) -> SourceReadSnapshot {
        SourceReadSnapshot {
            snapshot: self.inner.read_tx(),
            keyspaces: Arc::clone(&self.keyspaces),
        }
    }
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
    pub fn batch_receipt_state(&self, receipt: &WriteBatchReceipt) -> Result<BatchReceiptState> {
        let key = [b"batch/".as_slice(), receipt.batch_digest.as_slice()].concat();
        let snapshot = self.read_snapshot();
        let Some(value) = snapshot.get(KeyspaceName::Batches, &key)? else {
            return Ok(BatchReceiptState::Missing);
        };
        if value.len() != std::mem::size_of::<usize>() {
            return Ok(BatchReceiptState::Corrupt);
        }
        let mut encoded = [0u8; std::mem::size_of::<usize>()];
        encoded.copy_from_slice(&value);
        let operation_count = usize::from_be_bytes(encoded);
        if operation_count != receipt.operation_count {
            return Ok(BatchReceiptState::Corrupt);
        }
        Ok(BatchReceiptState::Present { operation_count })
    }
}

fn digest_actions(actions: &[WriteAction]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for action in actions {
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
    hasher.finalize().into()
}

pub struct SourceReadSnapshot {
    pub(crate) snapshot: fjall::Snapshot,
    pub(crate) keyspaces: Arc<HashMap<KeyspaceName, SingleWriterTxKeyspace>>,
}

impl SourceReadSnapshot {
    pub fn get(&self, keyspace: KeyspaceName, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.snapshot
            .get(
                self.keyspaces
                    .get(&keyspace)
                    .expect("configured keyspace is present"),
                key,
            )
            .map(|value| value.map(|bytes| bytes.to_vec()))
            .map_err(Into::into)
    }
}
