use crate::{
    codec::VersionedEnvelope,
    database::{
        DatabaseConfig, PersistMode, SingleWriterTxDatabase, SourceReadSnapshot, WriteAction,
        WriteBatchReceipt,
    },
    errors::Result,
    keyspaces::KeyspaceName,
};
use std::{path::Path, sync::Arc};

mod clone;
mod shared;
pub use clone::SourceCloneProgress;
pub(crate) use clone::{ensure_not_cancelled, read_bounded_page};
pub use shared::{
    close_shared_source_stores_under, open_shared_source_store, SharedSourceStoreCloseGuard,
};

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
