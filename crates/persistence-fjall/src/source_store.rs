use crate::{
    codec::VersionedEnvelope,
    database::{
        DatabaseConfig, PersistMode, SingleWriterTxDatabase, SourceReadSnapshot, WriteAction,
        WriteBatchReceipt,
    },
    errors::Result,
    keyspaces::KeyspaceName,
};
use std::path::Path;

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
}
