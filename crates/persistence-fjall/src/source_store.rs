use crate::{
    codec::VersionedEnvelope,
    database::{
        DatabaseConfig, PersistMode, SingleWriterTxDatabase, SourceReadSnapshot, WriteAction,
        WriteBatchReceipt,
    },
    errors::Result,
    keyspaces::KeyspaceName,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
};

static SHARED_STORES: OnceLock<Mutex<HashMap<PathBuf, Weak<crate::database::DatabaseState>>>> =
    OnceLock::new();

mod clone;
pub use clone::SourceCloneProgress;
pub(crate) use clone::{ensure_not_cancelled, read_bounded_page};

#[derive(Clone)]
pub struct SourceStore {
    database: SingleWriterTxDatabase,
}

impl SourceStore {
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self> {
        Ok(Self {
            database: SingleWriterTxDatabase::open(path, config)?,
        })
    }
    pub fn database(&self) -> &SingleWriterTxDatabase {
        &self.database
    }

    pub fn is_exclusively_owned(&self) -> bool {
        Arc::strong_count(&self.database.inner) == 1
    }

    pub fn write_envelope(
        &self,
        keyspace: KeyspaceName,
        key: Vec<u8>,
        envelope: &VersionedEnvelope,
        mode: PersistMode,
    ) -> Result<WriteBatchReceipt> {
        self.database.write_batch(
            &[WriteAction::Put {
                keyspace,
                key,
                value: envelope.encode()?,
            }],
            mode,
        )
    }
    pub fn read_envelope(
        &self,
        snapshot: &SourceReadSnapshot,
        keyspace: KeyspaceName,
        key: &[u8],
    ) -> Result<Option<VersionedEnvelope>> {
        snapshot
            .get(keyspace, key)?
            .map(|value| VersionedEnvelope::decode(&value))
            .transpose()
    }

    pub fn read_raw(
        &self,
        snapshot: &SourceReadSnapshot,
        keyspace: KeyspaceName,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        snapshot.get(keyspace, key)
    }

    pub fn write_raw(
        &self,
        keyspace: KeyspaceName,
        key: Vec<u8>,
        value: Vec<u8>,
        mode: PersistMode,
    ) -> Result<WriteBatchReceipt> {
        self.database.write_batch(
            &[WriteAction::Put {
                keyspace,
                key,
                value,
            }],
            mode,
        )
    }
}

/// Opens one process-wide SourceStore for a Fjall directory.
///
/// Fjall permits a single owner for a database directory. All source readers
/// and writers therefore share this registry instead of opening independent
/// handles for the same source.
pub fn open_shared_source_store(
    path: impl AsRef<Path>,
    config: DatabaseConfig,
) -> Result<Arc<SourceStore>> {
    let path = std::fs::canonicalize(path.as_ref()).or_else(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            let path = path.as_ref();
            std::fs::create_dir_all(path)?;
            std::fs::canonicalize(path)
        } else {
            Err(error)
        }
    })?;
    let registry = SHARED_STORES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry
        .lock()
        .map_err(|_| crate::errors::FjallStorageError::LockPoisoned)?;
    registry.retain(|_, store| store.strong_count() > 0);
    if let Some(store) = registry.get(&path).and_then(Weak::upgrade) {
        return Ok(Arc::new(SourceStore {
            database: SingleWriterTxDatabase { inner: store },
        }));
    }
    let store = Arc::new(SourceStore::open(&path, config)?);
    registry.insert(path, Arc::downgrade(&store.database.inner));
    Ok(store)
}
