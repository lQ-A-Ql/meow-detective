use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::{DatabaseConfig, PersistMode, SourceReadSnapshot, WriteAction},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
    source_store::SourceStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

const RECORD_KIND: &str = "parser_record";
const SCHEMA_VERSION: u16 = 1;
const RECORD_FAMILY: &str = "parser.record";
const FAMILY_INDEX: &str = "parser.family";
const KIND_INDEX: &str = "parser.kind";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParserRecord {
    pub id: String,
    pub data_source_id: String,
    pub family: String,
    pub kind: String,
    pub status: String,
    pub summary_sha256: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParserPage {
    pub records: Vec<ParserRecord>,
    pub next_cursor: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct ParserRecordStore {
    source_store: SourceStore,
}

impl ParserRecordStore {
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self> {
        Ok(Self {
            source_store: SourceStore::open(path, config)?,
        })
    }

    pub fn from_source_store(source_store: SourceStore) -> Self {
        Self { source_store }
    }

    pub fn read_snapshot(&self) -> SourceReadSnapshot {
        self.source_store.database().read_snapshot()
    }

    pub fn get(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        id: &str,
    ) -> Result<Option<ParserRecord>> {
        let key = record_key(source_id, id)?;
        let Some(envelope) =
            self.source_store
                .read_envelope(snapshot, KeyspaceName::ParserRecords, &key)?
        else {
            return Ok(None);
        };
        Ok(Some(decode(envelope)?))
    }

    pub fn upsert(&self, record: &ParserRecord, mode: PersistMode) -> Result<()> {
        validate(record)?;
        let key = record_key(&record.data_source_id, &record.id)?;
        let snapshot = self.read_snapshot();
        let mut actions = Vec::new();
        if let Some(previous) = self.get(&snapshot, &record.data_source_id, &record.id)? {
            actions.extend(index_deletes(&previous)?);
        }
        let payload = serde_json::to_vec(record)
            .map_err(|error| FjallStorageError::Serialization(error.to_string()))?;
        let envelope = VersionedEnvelope::new(
            RECORD_KIND,
            SCHEMA_VERSION,
            &record.id,
            &record.data_source_id,
            1,
            payload,
        );
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::ParserRecords,
            key,
            value: envelope.encode()?,
        });
        actions.extend(index_writes(record)?);
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }

    pub fn delete(&self, source_id: &str, id: &str, mode: PersistMode) -> Result<bool> {
        let snapshot = self.read_snapshot();
        let Some(previous) = self.get(&snapshot, source_id, id)? else {
            return Ok(false);
        };
        let mut actions = vec![WriteAction::Delete {
            keyspace: KeyspaceName::ParserRecords,
            key: record_key(source_id, id)?,
        }];
        actions.extend(index_deletes(&previous)?);
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(true)
    }

    pub fn list_by_family(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        family: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ParserPage> {
        self.page(
            snapshot,
            &encode_key(
                FAMILY_INDEX,
                &[
                    KeyPart::Text(source_id.to_owned()),
                    KeyPart::Text(family.to_owned()),
                ],
            )?,
            cursor,
            direction,
            limit,
        )
    }

    pub fn list_by_kind(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        kind: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ParserPage> {
        self.page(
            snapshot,
            &encode_key(
                KIND_INDEX,
                &[
                    KeyPart::Text(source_id.to_owned()),
                    KeyPart::Text(kind.to_owned()),
                ],
            )?,
            cursor,
            direction,
            limit,
        )
    }

    fn page(
        &self,
        snapshot: &SourceReadSnapshot,
        prefix: &[u8],
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ParserPage> {
        let entries = snapshot.keyset(
            KeyspaceName::ParserIndexes,
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
                .read_envelope(snapshot, KeyspaceName::ParserRecords, &entry.value)?
                .ok_or(FjallStorageError::ParserIndexCorrupt)?;
            records.push(decode(envelope)?);
        }
        let next_cursor = has_more
            .then(|| entries.last().map(|entry| entry.key.clone()))
            .flatten();
        Ok(ParserPage {
            records,
            next_cursor,
        })
    }
}

fn validate(record: &ParserRecord) -> Result<()> {
    if record.id.is_empty()
        || record.data_source_id.is_empty()
        || record.family.is_empty()
        || record.kind.is_empty()
        || record.status.is_empty()
        || record.summary_sha256.is_empty()
    {
        return Err(FjallStorageError::InvalidEnvelope(
            "parser record identity fields are empty".to_owned(),
        ));
    }
    Ok(())
}

fn decode(envelope: VersionedEnvelope) -> Result<ParserRecord> {
    if envelope.record_kind != RECORD_KIND || envelope.schema_version != SCHEMA_VERSION {
        return Err(FjallStorageError::ParserRecordIdentityMismatch);
    }
    let record: ParserRecord = serde_json::from_slice(&envelope.payload)
        .map_err(|error| FjallStorageError::Deserialization(error.to_string()))?;
    if record.id != envelope.record_id || record.data_source_id != envelope.source_id {
        return Err(FjallStorageError::ParserRecordIdentityMismatch);
    }
    validate(&record)?;
    Ok(record)
}

fn record_key(source_id: &str, id: &str) -> Result<Vec<u8>> {
    encode_key(
        RECORD_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(id.to_owned()),
        ],
    )
}
fn index_writes(record: &ParserRecord) -> Result<Vec<WriteAction>> {
    let record_key = record_key(&record.data_source_id, &record.id)?;
    Ok(vec![
        WriteAction::Put {
            keyspace: KeyspaceName::ParserIndexes,
            key: encode_key(
                FAMILY_INDEX,
                &[
                    KeyPart::Text(record.data_source_id.clone()),
                    KeyPart::Text(record.family.clone()),
                    KeyPart::Text(record.id.clone()),
                ],
            )?,
            value: record_key.clone(),
        },
        WriteAction::Put {
            keyspace: KeyspaceName::ParserIndexes,
            key: encode_key(
                KIND_INDEX,
                &[
                    KeyPart::Text(record.data_source_id.clone()),
                    KeyPart::Text(record.kind.clone()),
                    KeyPart::Text(record.id.clone()),
                ],
            )?,
            value: record_key,
        },
    ])
}
fn index_deletes(record: &ParserRecord) -> Result<Vec<WriteAction>> {
    Ok(vec![
        WriteAction::Delete {
            keyspace: KeyspaceName::ParserIndexes,
            key: encode_key(
                FAMILY_INDEX,
                &[
                    KeyPart::Text(record.data_source_id.clone()),
                    KeyPart::Text(record.family.clone()),
                    KeyPart::Text(record.id.clone()),
                ],
            )?,
        },
        WriteAction::Delete {
            keyspace: KeyspaceName::ParserIndexes,
            key: encode_key(
                KIND_INDEX,
                &[
                    KeyPart::Text(record.data_source_id.clone()),
                    KeyPart::Text(record.kind.clone()),
                    KeyPart::Text(record.id.clone()),
                ],
            )?,
        },
    ])
}
