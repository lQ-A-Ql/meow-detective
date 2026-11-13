use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::{DatabaseConfig, PersistMode, SourceReadSnapshot, WriteAction},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
    source_store::SourceStore,
};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

mod facet_index;
mod staging;
mod storage_cost;
mod timeline_batch;
mod timeline_count;
mod timeline_facets;

pub use storage_cost::TimelineRecordStorageCost;
pub use timeline_facets::FacetSummary;

const RECORD_KIND: &str = "timeline_event";
const SCHEMA_VERSION: u16 = 1;
const RECORD_FAMILY: &str = "timeline.record";
const SOURCE_FAMILY: &str = "timeline.source";
const TIME_FAMILY: &str = "timeline.time";
const TYPE_FAMILY: &str = "timeline.type";
const OBJECT_FAMILY: &str = "timeline.object";
const CLUSTER_FAMILY: &str = "timeline.cluster";
const TIMESTAMP_SORT_MASK: u64 = 1 << 63;
const REVISION_KEY_PREFIX: &str = "timeline.revision/";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineRecord {
    pub id: String,
    pub data_source_id: String,
    pub source_object_id: String,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub title: String,
    pub description: String,
    pub parser_id: Option<String>,
    pub parser_version: Option<String>,
    pub confidence: Option<f32>,
    pub source_attribution: Option<String>,
    pub attrs: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelinePage {
    pub records: Vec<TimelineRecord>,
    pub next_cursor: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct TimelineTypeAfterQuery<'a> {
    pub source_id: &'a str,
    pub event_type: &'a str,
    pub after_timestamp: DateTime<Utc>,
    pub after_id: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum TimelineStreamError<E> {
    #[error(transparent)]
    Storage(#[from] FjallStorageError),
    #[error("timeline stream producer failed: {0}")]
    Producer(E),
}

#[derive(Clone)]
pub struct TimelineRecordStore {
    source_store: SourceStore,
}

impl TimelineRecordStore {
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

    pub fn cursor_revision(&self, snapshot: &SourceReadSnapshot) -> Result<u64> {
        let key = revision_key("timeline")?;
        let Some(value) = snapshot.get(KeyspaceName::Meta, &key)? else {
            return Ok(0);
        };
        if value.len() != std::mem::size_of::<u64>() {
            return Err(FjallStorageError::InvalidEnvelope(
                "timeline revision metadata is malformed".to_owned(),
            ));
        }
        let mut bytes = [0; std::mem::size_of::<u64>()];
        bytes.copy_from_slice(&value);
        Ok(u64::from_be_bytes(bytes))
    }

    pub fn get(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        record_id: &str,
    ) -> Result<Option<TimelineRecord>> {
        let key = record_key(source_id, record_id)?;
        self.read_record(snapshot, &key, source_id, record_id)
    }

    pub fn upsert(&self, record: &TimelineRecord, mode: PersistMode) -> Result<()> {
        self.upsert_batch(std::slice::from_ref(record), mode)
    }

    pub fn delete(&self, source_id: &str, record_id: &str, mode: PersistMode) -> Result<bool> {
        Ok(self.delete_batch(source_id, &[record_id.into()], mode)? > 0)
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
                let mut count_deltas = HashMap::new();
                let counts_ready = timeline_count::counts_ready(snapshot, source_id)?;
                let ids = ids.iter().collect::<std::collections::BTreeSet<_>>();
                for id in ids {
                    let Some(record) = self.get(snapshot, source_id, id)? else {
                        continue;
                    };
                    actions.push(WriteAction::Delete {
                        keyspace: KeyspaceName::TimelineRecords,
                        key: record_key(source_id, id)?,
                    });
                    self.index_deletes(&mut actions, &record)?;
                    if counts_ready {
                        timeline_count::adjust_count_deltas(&mut count_deltas, &record, -1)?;
                    }
                    deleted += 1;
                }
                if !actions.is_empty() {
                    if counts_ready {
                        timeline_count::append_count_actions(snapshot, &mut actions, count_deltas)?;
                    }
                    self.write_revision(snapshot, &mut actions)?;
                }
                Ok(actions)
            })?;
        Ok(deleted)
    }

    pub fn list_by_source(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage> {
        let prefix = encode_key(SOURCE_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }

    pub fn list_by_time(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage> {
        let prefix = encode_key(TIME_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }

    pub fn list_by_time_after(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        after_timestamp: DateTime<Utc>,
        after_id: &str,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage> {
        let prefix = encode_key(TIME_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
        let cursor = encode_key(
            TIME_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::InverseU64(timestamp_sort_value(after_timestamp)),
                KeyPart::Text(after_id.to_owned()),
            ],
        )?;
        self.page_from_index(snapshot, &prefix, Some(&cursor), direction, limit)
    }

    pub fn list_by_type(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage> {
        let prefix = encode_key(
            TYPE_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(event_type.to_owned()),
            ],
        )?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }

    pub fn list_by_type_after(
        &self,
        snapshot: &SourceReadSnapshot,
        query: TimelineTypeAfterQuery<'_>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage> {
        let prefix = encode_key(
            TYPE_FAMILY,
            &[
                KeyPart::Text(query.source_id.to_owned()),
                KeyPart::Text(query.event_type.to_owned()),
            ],
        )?;
        let cursor = encode_key(
            TYPE_FAMILY,
            &[
                KeyPart::Text(query.source_id.to_owned()),
                KeyPart::Text(query.event_type.to_owned()),
                KeyPart::InverseU64(timestamp_sort_value(query.after_timestamp)),
                KeyPart::Text(query.after_id.to_owned()),
            ],
        )?;
        self.page_from_index(snapshot, &prefix, Some(&cursor), direction, limit)
    }

    pub fn list_by_source_object(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        source_object_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage> {
        let prefix = encode_key(
            OBJECT_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(source_object_id.to_owned()),
            ],
        )?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }

    pub fn list_by_cluster(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage> {
        let prefix = encode_key(CLUSTER_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }

    fn page_from_index(
        &self,
        snapshot: &SourceReadSnapshot,
        prefix: &[u8],
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage> {
        if limit == 0 {
            return Ok(TimelinePage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let entries = snapshot.keyset(
            KeyspaceName::TimelineIndexes,
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
                .read_envelope(snapshot, KeyspaceName::TimelineRecords, &entry.value)?
                .ok_or(FjallStorageError::TimelineIndexCorrupt)?;
            records.push(decode_record(envelope)?);
        }
        let next_cursor = has_more
            .then(|| entries.last().map(|entry| entry.key.clone()))
            .flatten();
        Ok(TimelinePage {
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
    ) -> Result<Option<TimelineRecord>> {
        let Some(envelope) =
            self.source_store
                .read_envelope(snapshot, KeyspaceName::TimelineRecords, key)?
        else {
            return Ok(None);
        };
        if envelope.record_kind != RECORD_KIND
            || envelope.record_id != record_id
            || envelope.source_id != source_id
        {
            return Err(FjallStorageError::TimelineRecordIdentityMismatch);
        }
        Ok(Some(decode_record(envelope)?))
    }

    fn index_deletes(&self, actions: &mut Vec<WriteAction>, record: &TimelineRecord) -> Result<()> {
        for key in index_keys(record)? {
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::TimelineIndexes,
                key,
            });
        }
        Ok(())
    }

    fn write_revision(
        &self,
        snapshot: &SourceReadSnapshot,
        actions: &mut Vec<WriteAction>,
    ) -> Result<()> {
        let revision = self
            .cursor_revision(snapshot)?
            .checked_add(1)
            .ok_or_else(|| {
                FjallStorageError::InvalidEnvelope("timeline revision overflowed".into())
            })?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::Meta,
            key: revision_key("timeline")?,
            value: revision.to_be_bytes().to_vec(),
        });
        Ok(())
    }
}

fn validate_record(record: &TimelineRecord) -> Result<()> {
    if record.id.is_empty()
        || record.data_source_id.is_empty()
        || record.event_type.is_empty()
        || record.source_object_id.is_empty()
    {
        return Err(FjallStorageError::InvalidEnvelope(
            "timeline identity fields are empty".to_owned(),
        ));
    }
    Ok(())
}

fn decode_record(envelope: VersionedEnvelope) -> Result<TimelineRecord> {
    if envelope.record_kind != RECORD_KIND || envelope.schema_version != SCHEMA_VERSION {
        return Err(FjallStorageError::TimelineRecordIdentityMismatch);
    }
    let record: TimelineRecord = serde_json::from_slice(&envelope.payload)
        .map_err(|error| FjallStorageError::Deserialization(error.to_string()))?;
    if record.id != envelope.record_id || record.data_source_id != envelope.source_id {
        return Err(FjallStorageError::TimelineRecordIdentityMismatch);
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

fn revision_key(scope: &str) -> Result<Vec<u8>> {
    encode_key(
        "timeline.meta",
        &[KeyPart::Text(format!("{REVISION_KEY_PREFIX}{scope}"))],
    )
}

fn index_keys(record: &TimelineRecord) -> Result<[Vec<u8>; 5]> {
    let timestamp = KeyPart::InverseU64(timestamp_sort_value(record.timestamp));
    Ok([
        encode_key(
            SOURCE_FAMILY,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                timestamp.clone(),
                KeyPart::Text(record.id.clone()),
            ],
        )?,
        encode_key(
            TIME_FAMILY,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                timestamp.clone(),
                KeyPart::Text(record.id.clone()),
            ],
        )?,
        encode_key(
            TYPE_FAMILY,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                KeyPart::Text(record.event_type.clone()),
                timestamp.clone(),
                KeyPart::Text(record.id.clone()),
            ],
        )?,
        encode_key(
            OBJECT_FAMILY,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                KeyPart::Text(record.source_object_id.clone()),
                timestamp.clone(),
                KeyPart::Text(record.id.clone()),
            ],
        )?,
        encode_key(
            CLUSTER_FAMILY,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                KeyPart::Text(record.event_type.clone()),
                KeyPart::Text(record.description.clone()),
                timestamp,
                KeyPart::Text(record.id.clone()),
            ],
        )?,
    ])
}

fn timestamp_sort_value(timestamp: DateTime<Utc>) -> u64 {
    (timestamp.timestamp_micros() as u64) ^ TIMESTAMP_SORT_MASK
}

pub fn encode_timestamp_desc(timestamp: DateTime<Utc>) -> [u8; 8] {
    (!timestamp_sort_value(timestamp)).to_be_bytes()
}

pub fn decode_timestamp_desc(bytes: &[u8]) -> Result<DateTime<Utc>> {
    let encoded: [u8; 8] = bytes
        .try_into()
        .map_err(|_| FjallStorageError::InvalidTimelineTimestampEncoding)?;
    let sortable = !u64::from_be_bytes(encoded);
    let micros = (sortable ^ TIMESTAMP_SORT_MASK) as i64;
    Utc.timestamp_micros(micros)
        .single()
        .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)
}
