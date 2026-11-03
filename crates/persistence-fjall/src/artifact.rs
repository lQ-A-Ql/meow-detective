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

mod artifact_batch;

const RECORD_KIND: &str = "artifact_record";
const SCHEMA_VERSION: u16 = 1;
const RECORD_FAMILY: &str = "artifact.record";
const TYPE_INDEX: &str = "artifact.type";
const OBJECT_INDEX: &str = "artifact.object";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactRecord {
    pub id: String,
    pub data_source_id: String,
    pub artifact_type: String,
    pub source_object_id: Option<String>,
    pub content_sha256: Option<String>,
    pub attrs: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactPage {
    pub records: Vec<ArtifactRecord>,
    pub next_cursor: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct ArtifactRecordStore {
    source_store: SourceStore,
}

impl ArtifactRecordStore {
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
    ) -> Result<Option<ArtifactRecord>> {
        let Some(envelope) = self.source_store.read_envelope(
            snapshot,
            KeyspaceName::ArtifactRecords,
            &record_key(source_id, id)?,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(decode(envelope)?))
    }
    pub fn upsert(&self, record: &ArtifactRecord, mode: PersistMode) -> Result<()> {
        validate(record)?;
        let snapshot = self.read_snapshot();
        let mut actions = Vec::new();
        let old = self.get(&snapshot, &record.data_source_id, &record.id)?;
        if old.as_ref().is_some_and(|previous| previous == record) {
            return Ok(());
        }
        if let Some(old) = old {
            actions.extend(index_deletes(&old)?);
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
            keyspace: KeyspaceName::ArtifactRecords,
            key: record_key(&record.data_source_id, &record.id)?,
            value: envelope.encode()?,
        });
        actions.extend(index_writes(record)?);
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }
    pub fn delete(&self, source_id: &str, id: &str, mode: PersistMode) -> Result<bool> {
        let snapshot = self.read_snapshot();
        let Some(old) = self.get(&snapshot, source_id, id)? else {
            return Ok(false);
        };
        let mut actions = vec![WriteAction::Delete {
            keyspace: KeyspaceName::ArtifactRecords,
            key: record_key(source_id, id)?,
        }];
        actions.extend(index_deletes(&old)?);
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(true)
    }

    pub fn delete_batch(
        &self,
        source_id: &str,
        ids: &[String],
        mode: PersistMode,
    ) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let snapshot = self.read_snapshot();
        let mut actions = Vec::new();
        let mut deleted = 0usize;
        for id in ids {
            let Some(record) = self.get(&snapshot, source_id, id)? else {
                continue;
            };
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::ArtifactRecords,
                key: record_key(source_id, id)?,
            });
            actions.extend(index_deletes(&record)?);
            deleted = deleted.saturating_add(1);
        }
        if !actions.is_empty() {
            self.source_store.database().write_batch(&actions, mode)?;
        }
        Ok(deleted)
    }
    pub fn list_by_type(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        artifact_type: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ArtifactPage> {
        self.page(
            snapshot,
            &encode_key(
                TYPE_INDEX,
                &[
                    KeyPart::Text(source_id.to_owned()),
                    KeyPart::Text(artifact_type.to_owned()),
                ],
            )?,
            cursor,
            direction,
            limit,
        )
    }

    pub fn list_by_source(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ArtifactPage> {
        self.page_records(
            snapshot,
            &encode_key(RECORD_FAMILY, &[KeyPart::Text(source_id.to_owned())])?,
            cursor,
            direction,
            limit,
        )
    }
    pub fn list_by_source_object(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        source_object_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ArtifactPage> {
        self.page(
            snapshot,
            &encode_key(
                OBJECT_INDEX,
                &[
                    KeyPart::Text(source_id.to_owned()),
                    KeyPart::Text(source_object_id.to_owned()),
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
    ) -> Result<ArtifactPage> {
        let entries = snapshot.keyset(
            KeyspaceName::ArtifactIndexes,
            prefix,
            cursor,
            direction,
            limit.saturating_add(1),
        )?;
        let has_more = entries.len() > limit;
        let entries = entries.into_iter().take(limit).collect::<Vec<_>>();
        let mut records = Vec::with_capacity(entries.len());
        for entry in &entries {
            records.push(decode(
                self.source_store
                    .read_envelope(snapshot, KeyspaceName::ArtifactRecords, &entry.value)?
                    .ok_or(FjallStorageError::ArtifactIndexCorrupt)?,
            )?);
        }
        let next_cursor = has_more
            .then(|| entries.last().map(|entry| entry.key.clone()))
            .flatten();
        Ok(ArtifactPage {
            records,
            next_cursor,
        })
    }

    fn page_records(
        &self,
        snapshot: &SourceReadSnapshot,
        prefix: &[u8],
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ArtifactPage> {
        if limit == 0 {
            return Ok(ArtifactPage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let entries = snapshot.keyset(
            KeyspaceName::ArtifactRecords,
            prefix,
            cursor,
            direction,
            limit.saturating_add(1),
        )?;
        let has_more = entries.len() > limit;
        let entries = entries.into_iter().take(limit).collect::<Vec<_>>();
        let records = entries
            .iter()
            .map(|entry| decode(VersionedEnvelope::decode(&entry.value)?))
            .collect::<Result<Vec<_>>>()?;
        let next_cursor = has_more
            .then(|| entries.last().map(|entry| entry.key.clone()))
            .flatten();
        Ok(ArtifactPage {
            records,
            next_cursor,
        })
    }
}

fn validate(record: &ArtifactRecord) -> Result<()> {
    if record.id.is_empty() || record.data_source_id.is_empty() || record.artifact_type.is_empty() {
        return Err(FjallStorageError::InvalidEnvelope(
            "artifact record identity fields are empty".to_owned(),
        ));
    }
    Ok(())
}
fn decode(envelope: VersionedEnvelope) -> Result<ArtifactRecord> {
    if envelope.record_kind != RECORD_KIND || envelope.schema_version != SCHEMA_VERSION {
        return Err(FjallStorageError::ArtifactRecordIdentityMismatch);
    }
    let record: ArtifactRecord = serde_json::from_slice(&envelope.payload)
        .map_err(|error| FjallStorageError::Deserialization(error.to_string()))?;
    if record.id != envelope.record_id || record.data_source_id != envelope.source_id {
        return Err(FjallStorageError::ArtifactRecordIdentityMismatch);
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
fn index_writes(record: &ArtifactRecord) -> Result<Vec<WriteAction>> {
    let value = record_key(&record.data_source_id, &record.id)?;
    let mut actions = vec![WriteAction::Put {
        keyspace: KeyspaceName::ArtifactIndexes,
        key: encode_key(
            TYPE_INDEX,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                KeyPart::Text(record.artifact_type.clone()),
                KeyPart::Text(record.id.clone()),
            ],
        )?,
        value: value.clone(),
    }];
    if let Some(source_object_id) = &record.source_object_id {
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::ArtifactIndexes,
            key: encode_key(
                OBJECT_INDEX,
                &[
                    KeyPart::Text(record.data_source_id.clone()),
                    KeyPart::Text(source_object_id.clone()),
                    KeyPart::Text(record.id.clone()),
                ],
            )?,
            value,
        });
    }
    Ok(actions)
}
fn index_deletes(record: &ArtifactRecord) -> Result<Vec<WriteAction>> {
    let mut actions = vec![WriteAction::Delete {
        keyspace: KeyspaceName::ArtifactIndexes,
        key: encode_key(
            TYPE_INDEX,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                KeyPart::Text(record.artifact_type.clone()),
                KeyPart::Text(record.id.clone()),
            ],
        )?,
    }];
    if let Some(source_object_id) = &record.source_object_id {
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::ArtifactIndexes,
            key: encode_key(
                OBJECT_INDEX,
                &[
                    KeyPart::Text(record.data_source_id.clone()),
                    KeyPart::Text(source_object_id.clone()),
                    KeyPart::Text(record.id.clone()),
                ],
            )?,
        });
    }
    Ok(actions)
}
