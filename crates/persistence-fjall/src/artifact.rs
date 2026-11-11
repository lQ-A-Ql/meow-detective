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
mod indexing;
use indexing::{created_index_key, family_created_index_key, CREATED_INDEX, FAMILY_CREATED_INDEX};
pub(super) use indexing::{index_deletes, index_writes};

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
    pub fn count(&self, snapshot: &SourceReadSnapshot, source: Option<&str>) -> Result<u64> {
        crate::record_metrics::count(snapshot, "artifact", source)
    }
    pub fn rebuild_metrics_for_source(&self, source_id: &str, mode: PersistMode) -> Result<()> {
        let snapshot = self.read_snapshot();
        let previous_source = crate::record_metrics::count(&snapshot, "artifact", Some(source_id))?;
        let previous_total = crate::record_metrics::count(&snapshot, "artifact", None)?;
        let mut record_count = 0u64;
        let mut cursor = None;
        loop {
            let page = self.list_by_source(
                &snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )?;
            record_count = record_count.saturating_add(page.records.len() as u64);
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        let mut actions = Vec::new();
        crate::record_metrics::set(&mut actions, "artifact", Some(source_id), record_count)?;
        crate::record_metrics::set(
            &mut actions,
            "artifact",
            None,
            previous_total
                .checked_sub(previous_source)
                .ok_or_else(|| {
                    FjallStorageError::InvalidEnvelope(
                        "artifact aggregate count is smaller than its source count".into(),
                    )
                })?
                .checked_add(record_count)
                .ok_or_else(|| {
                    FjallStorageError::InvalidEnvelope("artifact aggregate count overflowed".into())
                })?,
        )?;
        actions.push(self.revision_action(&snapshot)?);
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }
    pub fn cursor_revision(&self, snapshot: &SourceReadSnapshot) -> Result<u64> {
        let key = encode_key("artifact.revision", &[])?;
        let Some(value) = snapshot.get(KeyspaceName::Meta, &key)? else {
            return Ok(0);
        };
        let bytes = <[u8; 8]>::try_from(value.as_slice()).map_err(|_| {
            FjallStorageError::InvalidEnvelope("artifact revision is malformed".into())
        })?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn revision_action(&self, snapshot: &SourceReadSnapshot) -> Result<WriteAction> {
        let revision = self
            .cursor_revision(snapshot)?
            .checked_add(1)
            .ok_or_else(|| {
                FjallStorageError::InvalidEnvelope("artifact revision overflowed".into())
            })?;
        Ok(WriteAction::Put {
            keyspace: KeyspaceName::Meta,
            key: encode_key("artifact.revision", &[])?,
            value: revision.to_be_bytes().to_vec(),
        })
    }
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
        self.upsert_batch(std::slice::from_ref(record), mode)
    }
    pub fn delete(&self, source_id: &str, id: &str, mode: PersistMode) -> Result<bool> {
        Ok(self.delete_batch(source_id, &[id.to_owned()], mode)? > 0)
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
        let mut deleted = 0usize;
        self.source_store
            .database()
            .write_planned(mode, |snapshot| {
                let mut actions = Vec::new();
                let mut deltas = crate::record_metrics::CountDeltas::new();
                let ids = ids.iter().collect::<std::collections::BTreeSet<_>>();
                for id in ids {
                    let Some(record) = self.get(snapshot, source_id, id)? else {
                        continue;
                    };
                    actions.push(WriteAction::Delete {
                        keyspace: KeyspaceName::ArtifactRecords,
                        key: record_key(source_id, id)?,
                    });
                    actions.extend(index_deletes(&record)?);
                    crate::record_metrics::adjust(&mut deltas, "artifact", source_id, -1)?;
                    deleted += 1;
                }
                if !actions.is_empty() {
                    crate::record_metrics::append(snapshot, &mut actions, deltas)?;
                    actions.push(self.revision_action(snapshot)?);
                }
                Ok(actions)
            })?;
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

    pub fn list_by_created_order(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        family: Option<&str>,
        after: Option<(&str, &str)>,
        limit: usize,
    ) -> Result<ArtifactPage> {
        let (prefix, cursor) = match family {
            Some(family) => {
                let prefix = encode_key(
                    FAMILY_CREATED_INDEX,
                    &[
                        KeyPart::Text(source_id.to_owned()),
                        KeyPart::Text(family.to_owned()),
                    ],
                )?;
                let cursor = after
                    .map(|(created_at, id)| {
                        family_created_index_key(source_id, family, created_at, id)
                    })
                    .transpose()?;
                (prefix, cursor)
            }
            None => {
                let prefix = encode_key(CREATED_INDEX, &[KeyPart::Text(source_id.to_owned())])?;
                let cursor = after
                    .map(|(created_at, id)| created_index_key(source_id, created_at, id))
                    .transpose()?;
                (prefix, cursor)
            }
        };
        self.page(
            snapshot,
            &prefix,
            cursor.as_deref(),
            ScanDirection::Forward,
            limit,
        )
    }

    pub fn count_by_type(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        artifact_type: &str,
    ) -> Result<u64> {
        let mut cursor = None;
        let mut count = 0u64;
        loop {
            let page = self.list_by_type(
                snapshot,
                source_id,
                artifact_type,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )?;
            count = count.saturating_add(page.records.len() as u64);
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(count);
            }
        }
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
