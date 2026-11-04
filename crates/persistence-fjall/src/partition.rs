use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::{DatabaseConfig, PersistMode, SourceReadSnapshot, WriteAction},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
    source_store::SourceStore,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

mod batch;

const RECORD_KIND: &str = "data_source_partition";
const SCHEMA_VERSION: u16 = 2;
const RECORD_FAMILY: &str = "partition.record";
const SOURCE_FAMILY: &str = "partition.source";
const INDEX_FAMILY: &str = "partition.index";
const TOMBSTONE_FAMILY: &str = "partition.tombstone";
const RUNTIME_FAMILY: &str = "partition.runtime";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimePartitionState {
    status: String,
    filesystem: Option<String>,
    unlock_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRecord {
    pub id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub name: String,
    pub kind_label: String,
    pub status: String,
    pub type_guid: Option<String>,
    pub offset: u64,
    pub length: u64,
    pub filesystem: Option<String>,
    pub unlock_hint: Option<String>,
    pub lvm_vg_uuid: Option<String>,
    pub lvm_vg_name: Option<String>,
    pub lvm_lv_uuid: Option<String>,
    pub lvm_lv_name: Option<String>,
    pub lvm_pv_offsets_json: Option<String>,
    pub lvm_pv_sources_json: Option<String>,
    pub gpt_disk_guid: Option<String>,
    pub gpt_partition_uuid: Option<String>,
    pub filesystem_uuid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionPage {
    pub records: Vec<PartitionRecord>,
    pub next_cursor: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct PartitionRecordStore {
    source_store: SourceStore,
}

impl PartitionRecordStore {
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
    ) -> Result<Option<PartitionRecord>> {
        let key = record_key(source_id, record_id)?;
        let Some(envelope) =
            self.source_store
                .read_envelope(snapshot, KeyspaceName::PartitionRecords, &key)?
        else {
            return Ok(None);
        };
        self.apply_runtime_state(snapshot, decode_record(envelope, source_id, record_id)?)
            .map(Some)
    }

    pub fn find_by_data_source_and_index(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        partition_index: u32,
    ) -> Result<Option<PartitionRecord>> {
        let prefix = encode_key(
            SOURCE_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::U32(partition_index),
            ],
        )?;
        let entries = snapshot.keyset(
            KeyspaceName::PartitionIndexes,
            &prefix,
            None,
            ScanDirection::Forward,
            1,
        )?;
        let Some(entry) = entries.first() else {
            return Ok(None);
        };
        self.read_indexed(snapshot, entry).map(Some)
    }

    pub fn upsert(&self, record: &PartitionRecord, mode: PersistMode) -> Result<()> {
        validate_record(record)?;
        let key = record_key(&record.data_source_id, &record.id)?;
        let snapshot = self.read_snapshot();
        let old = self.get(&snapshot, &record.data_source_id, &record.id)?;
        let mut actions = Vec::with_capacity(6);
        if let Some(previous) = old.as_ref() {
            index_deletes(&mut actions, previous)?;
        }
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::Meta,
            key: runtime_key(&record.data_source_id, &record.id)?,
        });
        let envelope = VersionedEnvelope::from_serializable(
            RECORD_KIND,
            SCHEMA_VERSION,
            &record.id,
            &record.data_source_id,
            1,
            record,
        )?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::PartitionRecords,
            key,
            value: envelope.encode()?,
        });
        index_writes(&mut actions, record)?;
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::Tombstones,
            key: tombstone_key(&record.data_source_id, &record.id)?,
        });
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }

    pub fn mark_catalog_ready(
        &self,
        source_id: &str,
        partition_index: u32,
        filesystem: &str,
        mode: PersistMode,
    ) -> Result<bool> {
        let snapshot = self.read_snapshot();
        let Some(record) =
            self.find_by_data_source_and_index(&snapshot, source_id, partition_index)?
        else {
            return Ok(false);
        };
        let envelope = VersionedEnvelope::from_serializable(
            "partition_runtime_state",
            SCHEMA_VERSION,
            &record.id,
            source_id,
            1,
            &RuntimePartitionState {
                status: "ready".to_owned(),
                filesystem: Some(filesystem.to_owned()),
                unlock_hint: None,
            },
        )?;
        self.source_store.database().write_batch(
            &[WriteAction::Put {
                keyspace: KeyspaceName::Meta,
                key: runtime_key(source_id, &record.id)?,
                value: envelope.encode()?,
            }],
            mode,
        )?;
        Ok(true)
    }

    pub fn delete(&self, source_id: &str, record_id: &str, mode: PersistMode) -> Result<bool> {
        let key = record_key(source_id, record_id)?;
        let snapshot = self.read_snapshot();
        let Some(previous) = self.get(&snapshot, source_id, record_id)? else {
            return Ok(false);
        };
        let mut actions = vec![WriteAction::Delete {
            keyspace: KeyspaceName::PartitionRecords,
            key: key.clone(),
        }];
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::Meta,
            key: runtime_key(source_id, record_id)?,
        });
        index_deletes(&mut actions, &previous)?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::Tombstones,
            key: tombstone_key(source_id, record_id)?,
            value: key,
        });
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(true)
    }

    pub fn list_by_source(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<PartitionPage> {
        let prefix = encode_key(SOURCE_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }

    pub fn list_by_data_source(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<PartitionPage> {
        self.list_by_source(snapshot, source_id, cursor, direction, limit)
    }

    pub fn find_by_type_guid(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        type_guid: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<PartitionPage> {
        let prefix = encode_key(
            INDEX_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(type_guid.to_owned()),
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
    ) -> Result<PartitionPage> {
        if limit == 0 {
            return Ok(PartitionPage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let entries = snapshot.keyset(
            KeyspaceName::PartitionIndexes,
            prefix,
            cursor,
            direction,
            limit.saturating_add(1),
        )?;
        let has_more = entries.len() > limit;
        let entries = entries.into_iter().take(limit).collect::<Vec<_>>();
        let mut records = Vec::with_capacity(entries.len());
        for entry in &entries {
            records.push(self.read_indexed(snapshot, entry)?);
        }
        Ok(PartitionPage {
            next_cursor: has_more
                .then(|| entries.last().map(|e| e.key.clone()))
                .flatten(),
            records,
        })
    }

    fn read_indexed(
        &self,
        snapshot: &SourceReadSnapshot,
        entry: &crate::SnapshotEntry,
    ) -> Result<PartitionRecord> {
        let envelope = self
            .source_store
            .read_envelope(snapshot, KeyspaceName::PartitionRecords, &entry.value)?
            .ok_or(FjallStorageError::InvalidEnvelope(
                "partition index is stale".to_owned(),
            ))?;
        self.apply_runtime_state(snapshot, decode_record(envelope, "", "")?)
    }

    fn apply_runtime_state(
        &self,
        snapshot: &SourceReadSnapshot,
        mut record: PartitionRecord,
    ) -> Result<PartitionRecord> {
        let Some(envelope) = self.source_store.read_envelope(
            snapshot,
            KeyspaceName::Meta,
            &runtime_key(&record.data_source_id, &record.id)?,
        )?
        else {
            return Ok(record);
        };
        if envelope.record_kind != "partition_runtime_state"
            || envelope.schema_version != SCHEMA_VERSION
            || envelope.source_id != record.data_source_id
            || envelope.record_id != record.id
        {
            return Err(FjallStorageError::InvalidEnvelope(
                "partition runtime state identity mismatch".to_owned(),
            ));
        }
        let state: RuntimePartitionState = envelope.decode_payload()?;
        record.status = state.status;
        record.filesystem = state.filesystem;
        record.unlock_hint = state.unlock_hint;
        Ok(record)
    }
}

fn runtime_key(source_id: &str, record_id: &str) -> Result<Vec<u8>> {
    encode_key(
        RUNTIME_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(record_id.to_owned()),
        ],
    )
}

fn validate_record(record: &PartitionRecord) -> Result<()> {
    if record.id.is_empty() || record.data_source_id.is_empty() || record.name.is_empty() {
        return Err(FjallStorageError::InvalidEnvelope(
            "partition identity fields are empty".to_owned(),
        ));
    }
    Ok(())
}

fn decode_record(
    envelope: VersionedEnvelope,
    source_id: &str,
    record_id: &str,
) -> Result<PartitionRecord> {
    if envelope.record_kind != RECORD_KIND
        || envelope.schema_version != SCHEMA_VERSION
        || (!source_id.is_empty() && envelope.source_id != source_id)
        || (!record_id.is_empty() && envelope.record_id != record_id)
    {
        return Err(FjallStorageError::InvalidEnvelope(
            "partition record identity mismatch".to_owned(),
        ));
    }
    let record: PartitionRecord = envelope.decode_payload()?;
    if record.id != envelope.record_id || record.data_source_id != envelope.source_id {
        return Err(FjallStorageError::InvalidEnvelope(
            "partition record identity mismatch".to_owned(),
        ));
    }
    validate_record(&record)?;
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

fn index_keys(record: &PartitionRecord) -> Result<Vec<Vec<u8>>> {
    let mut keys = vec![encode_key(
        SOURCE_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::U32(record.partition_index),
            KeyPart::Text(record.id.clone()),
        ],
    )?];
    if let Some(type_guid) = record
        .type_guid
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        keys.push(encode_key(
            INDEX_FAMILY,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                KeyPart::Text(type_guid.to_owned()),
                KeyPart::Text(record.id.clone()),
            ],
        )?);
    }
    Ok(keys)
}

fn index_writes(actions: &mut Vec<WriteAction>, record: &PartitionRecord) -> Result<()> {
    let value = record_key(&record.data_source_id, &record.id)?;
    for key in index_keys(record)? {
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::PartitionIndexes,
            key,
            value: value.clone(),
        });
    }
    Ok(())
}

fn index_deletes(actions: &mut Vec<WriteAction>, record: &PartitionRecord) -> Result<()> {
    for key in index_keys(record)? {
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::PartitionIndexes,
            key,
        });
    }
    Ok(())
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
