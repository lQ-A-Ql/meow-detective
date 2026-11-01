use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::{DatabaseConfig, PersistMode, SourceReadSnapshot, WriteAction},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
    source_store::SourceStore,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

const RECORD_KIND: &str = "file_entry";
const SCHEMA_VERSION: u16 = 1;
const RECORD_FAMILY: &str = "catalog.record";
const PARENT_FAMILY: &str = "catalog.parent";
const PATH_FAMILY: &str = "catalog.path";
const HASH_FAMILY: &str = "catalog.hash";
const TOMBSTONE_FAMILY: &str = "catalog.tombstone";
const ROOT_PARENT: &str = "";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CatalogEntryType {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntryRecord {
    pub id: String,
    pub parent_id: Option<String>,
    pub data_source_id: String,
    pub path: String,
    pub name: String,
    pub entry_type: CatalogEntryType,
    pub size: Option<u64>,
    pub ext: Option<String>,
    pub deleted: bool,
    pub hidden: bool,
    pub system: bool,
    pub read_only: bool,
    pub archive: bool,
    pub encrypted: bool,
    pub created: Option<DateTime<Utc>>,
    pub modified: Option<DateTime<Utc>>,
    pub accessed: Option<DateTime<Utc>>,
    pub changed: Option<DateTime<Utc>>,
    pub hash_sha256: Option<String>,
    pub partition_index: Option<u32>,
    pub unix_mode: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPage {
    pub records: Vec<FileEntryRecord>,
    pub next_cursor: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct CatalogRecordStore {
    source_store: SourceStore,
}

impl CatalogRecordStore {
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self> {
        Ok(Self {
            source_store: SourceStore::open(path, config)?,
        })
    }

    pub fn from_source_store(source_store: SourceStore) -> Self {
        Self { source_store }
    }

    pub fn source_store(&self) -> &SourceStore {
        &self.source_store
    }

    pub fn read_snapshot(&self) -> SourceReadSnapshot {
        self.source_store.database().read_snapshot()
    }

    pub fn get(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        record_id: &str,
    ) -> Result<Option<FileEntryRecord>> {
        let key = record_key(source_id, record_id)?;
        self.read_record(snapshot, &key, source_id, record_id)
    }

    pub fn upsert(&self, record: &FileEntryRecord, mode: PersistMode) -> Result<()> {
        let key = record_key(&record.data_source_id, &record.id)?;
        let snapshot = self.read_snapshot();
        let old = self.read_record(&snapshot, &key, &record.data_source_id, &record.id)?;
        let mut actions = Vec::with_capacity(8);
        if let Some(previous) = old {
            self.index_deletes(&mut actions, &previous)?;
        }
        let envelope = VersionedEnvelope::from_serializable(
            RECORD_KIND,
            SCHEMA_VERSION,
            &record.id,
            &record.data_source_id,
            1,
            record,
        )?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::CatalogRecords,
            key,
            value: envelope.encode()?,
        });
        self.index_writes(&mut actions, record)?;
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::Tombstones,
            key: tombstone_key(&record.data_source_id, &record.id)?,
        });
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }

    pub fn delete(&self, source_id: &str, record_id: &str, mode: PersistMode) -> Result<bool> {
        let key = record_key(source_id, record_id)?;
        let snapshot = self.read_snapshot();
        let Some(previous) = self.read_record(&snapshot, &key, source_id, record_id)? else {
            return Ok(false);
        };
        let mut actions = Vec::with_capacity(5);
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::CatalogRecords,
            key: key.clone(),
        });
        self.index_deletes(&mut actions, &previous)?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::Tombstones,
            key: tombstone_key(source_id, record_id)?,
            value: key.clone(),
        });
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(true)
    }

    pub fn list_by_parent(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        parent_id: Option<&str>,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage> {
        let parent = parent_id.unwrap_or(ROOT_PARENT);
        let prefix = encode_key(
            PARENT_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(parent.to_owned()),
            ],
        )?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }

    pub fn find_by_path(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        path: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage> {
        let prefix = encode_key(
            PATH_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(path.to_owned()),
            ],
        )?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }

    pub fn find_by_hash(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        hash_sha256: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage> {
        let prefix = encode_key(
            HASH_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(hash_sha256.to_owned()),
            ],
        )?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }

    fn page_from_index(
        &self,
        snapshot: &SourceReadSnapshot,
        prefix: &[u8],
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage> {
        if limit == 0 {
            return Ok(CatalogPage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let entries = snapshot.keyset(
            KeyspaceName::CatalogIndexes,
            prefix,
            cursor,
            direction,
            limit.saturating_add(1),
        )?;
        let has_more = entries.len() > limit;
        let entries = entries.into_iter().take(limit).collect::<Vec<_>>();
        let mut records = Vec::with_capacity(entries.len());
        for entry in &entries {
            let envelope = self
                .source_store
                .read_envelope(snapshot, KeyspaceName::CatalogRecords, &entry.value)?
                .ok_or(FjallStorageError::CatalogIndexCorrupt)?;
            records.push(decode_record(envelope)?);
        }
        let next_cursor = has_more
            .then(|| entries.last().map(|entry| entry.key.clone()))
            .flatten();
        Ok(CatalogPage {
            records,
            next_cursor,
        })
    }

    fn read_record(
        &self,
        snapshot: &SourceReadSnapshot,
        key: &[u8],
        source_id: &str,
        record_id: &str,
    ) -> Result<Option<FileEntryRecord>> {
        let Some(envelope) =
            self.source_store
                .read_envelope(snapshot, KeyspaceName::CatalogRecords, key)?
        else {
            return Ok(None);
        };
        if envelope.record_kind != RECORD_KIND
            || envelope.record_id != record_id
            || envelope.source_id != source_id
        {
            return Err(FjallStorageError::CatalogRecordIdentityMismatch);
        }
        Ok(Some(decode_record(envelope)?))
    }

    fn index_writes(&self, actions: &mut Vec<WriteAction>, record: &FileEntryRecord) -> Result<()> {
        let record_key = record_key(&record.data_source_id, &record.id)?;
        actions.push(index_write(parent_key(record)?, record_key.clone()));
        actions.push(index_write(path_key(record)?, record_key.clone()));
        if record
            .hash_sha256
            .as_deref()
            .is_some_and(|hash| !hash.is_empty())
        {
            actions.push(index_write(hash_key(record)?, record_key));
        }
        Ok(())
    }

    fn index_deletes(
        &self,
        actions: &mut Vec<WriteAction>,
        record: &FileEntryRecord,
    ) -> Result<()> {
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::CatalogIndexes,
            key: parent_key(record)?,
        });
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::CatalogIndexes,
            key: path_key(record)?,
        });
        if record
            .hash_sha256
            .as_deref()
            .is_some_and(|hash| !hash.is_empty())
        {
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::CatalogIndexes,
                key: hash_key(record)?,
            });
        }
        Ok(())
    }
}

fn decode_record(envelope: VersionedEnvelope) -> Result<FileEntryRecord> {
    if envelope.record_kind != RECORD_KIND || envelope.schema_version != SCHEMA_VERSION {
        return Err(FjallStorageError::CatalogRecordIdentityMismatch);
    }
    let record: FileEntryRecord = envelope.decode_payload()?;
    if record.id != envelope.record_id || record.data_source_id != envelope.source_id {
        return Err(FjallStorageError::CatalogRecordIdentityMismatch);
    }
    Ok(record)
}

fn record_key(source_id: &str, record_id: &str) -> Result<Vec<u8>> {
    encode_key(
        RECORD_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(record_id.to_owned()),
        ],
    )
}

fn parent_key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    encode_key(
        PARENT_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::Text(
                record
                    .parent_id
                    .as_deref()
                    .unwrap_or(ROOT_PARENT)
                    .to_owned(),
            ),
            KeyPart::Text(record.name.clone()),
            KeyPart::Text(record.id.clone()),
        ],
    )
}

fn path_key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    encode_key(
        PATH_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::Text(record.path.clone()),
            KeyPart::Text(record.id.clone()),
        ],
    )
}

fn hash_key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    encode_key(
        HASH_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::Text(record.hash_sha256.clone().unwrap_or_default()),
            KeyPart::Text(record.id.clone()),
        ],
    )
}

fn tombstone_key(source_id: &str, record_id: &str) -> Result<Vec<u8>> {
    encode_key(
        TOMBSTONE_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(record_id.to_owned()),
        ],
    )
}

fn index_write(key: Vec<u8>, record_key: Vec<u8>) -> WriteAction {
    WriteAction::Put {
        keyspace: KeyspaceName::CatalogIndexes,
        key,
        value: record_key,
    }
}
