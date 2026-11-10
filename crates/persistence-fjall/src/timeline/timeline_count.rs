use super::{TimelineRecord, TimelineRecordStore, RECORD_FAMILY};
use crate::{
    codec::{encode_key, KeyPart},
    database::{PersistMode, SourceReadSnapshot, WriteAction},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

mod timeline_count_ranges;

const COUNT_FAMILY: &str = "timeline.count";
const COUNT_SCHEMA_VALUE: &[u8] = b"v1";

impl TimelineRecordStore {
    pub fn count_by_time(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<u64> {
        self.count_by_time_and_type(snapshot, source_id, None, start, end)
    }
    pub fn count_by_type(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<u64> {
        self.count_by_time_and_type(snapshot, source_id, Some(event_type), start, end)
    }
    pub fn counts_ready(&self, snapshot: &SourceReadSnapshot, source_id: &str) -> Result<bool> {
        counts_ready(snapshot, source_id)
    }
    pub fn rebuild_count_indexes(&self, source_id: &str, mode: PersistMode) -> Result<()> {
        let snapshot = self.read_snapshot();
        let prefix = encode_key(RECORD_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
        let mut deltas = HashMap::new();
        for entry in snapshot.prefix(
            KeyspaceName::TimelineRecords,
            &prefix,
            ScanDirection::Forward,
        )? {
            let envelope = self
                .source_store
                .read_envelope(&snapshot, KeyspaceName::TimelineRecords, &entry.key)?
                .ok_or(FjallStorageError::TimelineIndexCorrupt)?;
            let record = super::decode_record(envelope)?;
            if record.data_source_id != source_id {
                return Err(FjallStorageError::TimelineRecordIdentityMismatch);
            }
            adjust_count_deltas(&mut deltas, &record, 1)?;
        }
        let count_prefix = encode_key(COUNT_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
        let mut actions = snapshot
            .prefix(
                KeyspaceName::TimelineIndexes,
                &count_prefix,
                ScanDirection::Forward,
            )?
            .into_iter()
            .map(|entry| WriteAction::Delete {
                keyspace: KeyspaceName::TimelineIndexes,
                key: entry.key,
            })
            .collect::<Vec<_>>();
        for (key, count) in deltas {
            let count = u64::try_from(count).map_err(|_| {
                FjallStorageError::InvalidEnvelope("timeline count is negative".to_owned())
            })?;
            actions.push(WriteAction::Put {
                keyspace: KeyspaceName::TimelineIndexes,
                key,
                value: count.to_be_bytes().to_vec(),
            });
        }
        actions.push(count_schema_marker_action(source_id)?);
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }
    fn count_by_time_and_type(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: Option<&str>,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<u64> {
        if start.zip(end).is_some_and(|(a, b)| a > b) {
            return Ok(0);
        }
        if !counts_ready(snapshot, source_id)? {
            let prefix = encode_key(RECORD_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
            let empty = snapshot
                .keyset(
                    KeyspaceName::TimelineRecords,
                    &prefix,
                    None,
                    ScanDirection::Forward,
                    1,
                )?
                .is_empty();
            if empty {
                return Ok(0);
            }
            return Err(FjallStorageError::TimelineIndexCorrupt);
        }
        if start.is_none() && end.is_none() {
            return read_count(snapshot, source_id, event_type, "total");
        }
        let start = match start {
            Some(v) => v,
            None => timeline_count_ranges::edge_timestamp_for_count(
                snapshot,
                source_id,
                event_type,
                ScanDirection::Reverse,
            )?
            .ok_or(FjallStorageError::TimelineIndexCorrupt)?,
        };
        let end = match end {
            Some(v) => v,
            None => timeline_count_ranges::edge_timestamp_for_count(
                snapshot,
                source_id,
                event_type,
                ScanDirection::Forward,
            )?
            .ok_or(FjallStorageError::TimelineIndexCorrupt)?,
        };
        if start > end {
            return Ok(0);
        }
        timeline_count_ranges::count_interval(snapshot, source_id, event_type, start, end)
    }
}

pub(super) fn count_write_policy(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
) -> Result<(bool, bool)> {
    if counts_ready(snapshot, source_id)? {
        return Ok((true, false));
    }
    let prefix = encode_key(RECORD_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
    let empty = snapshot
        .keyset(
            KeyspaceName::TimelineRecords,
            &prefix,
            None,
            ScanDirection::Forward,
            1,
        )?
        .is_empty();
    Ok((empty, empty))
}
pub(super) fn counts_ready(snapshot: &SourceReadSnapshot, source_id: &str) -> Result<bool> {
    let key = encode_key(
        "timeline.count-schema",
        &[KeyPart::Text(source_id.to_owned())],
    )?;
    let Some(value) = snapshot.get(KeyspaceName::Meta, &key)? else {
        return Ok(false);
    };
    if value == COUNT_SCHEMA_VALUE {
        Ok(true)
    } else {
        Err(FjallStorageError::InvalidEnvelope(
            "timeline count schema marker is malformed".to_owned(),
        ))
    }
}
pub(super) fn count_schema_marker_action(source_id: &str) -> Result<WriteAction> {
    Ok(WriteAction::Put {
        keyspace: KeyspaceName::Meta,
        key: encode_key(
            "timeline.count-schema",
            &[KeyPart::Text(source_id.to_owned())],
        )?,
        value: COUNT_SCHEMA_VALUE.to_vec(),
    })
}
pub(super) fn adjust_count_deltas(
    deltas: &mut HashMap<Vec<u8>, i64>,
    record: &TimelineRecord,
    amount: i64,
) -> Result<()> {
    for key in count_keys(record)? {
        let delta = deltas.entry(key).or_default();
        *delta = delta.checked_add(amount).ok_or_else(|| {
            FjallStorageError::InvalidEnvelope("timeline count delta overflowed".to_owned())
        })?;
    }
    Ok(())
}
pub(super) fn append_count_actions(
    snapshot: &SourceReadSnapshot,
    actions: &mut Vec<WriteAction>,
    deltas: HashMap<Vec<u8>, i64>,
) -> Result<()> {
    for (key, delta) in deltas {
        if delta == 0 {
            continue;
        }
        let current = match snapshot.get(KeyspaceName::TimelineIndexes, &key)? {
            Some(value) if value.len() == 8 => {
                u64::from_be_bytes(value.as_slice().try_into().expect("count is 8 bytes"))
            }
            Some(_) => return Err(FjallStorageError::TimelineIndexCorrupt),
            None => 0,
        };
        let next = if delta > 0 {
            current.checked_add(delta as u64)
        } else {
            current.checked_sub(delta.unsigned_abs())
        }
        .ok_or_else(|| {
            FjallStorageError::InvalidEnvelope("timeline count index underflowed".to_owned())
        })?;
        actions.push(if next == 0 {
            WriteAction::Delete {
                keyspace: KeyspaceName::TimelineIndexes,
                key,
            }
        } else {
            WriteAction::Put {
                keyspace: KeyspaceName::TimelineIndexes,
                key,
                value: next.to_be_bytes().to_vec(),
            }
        });
    }
    Ok(())
}
fn count_keys(record: &TimelineRecord) -> Result<[Vec<u8>; 8]> {
    let year = format!("year:{}", record.timestamp.format("%Y"));
    let month = format!("month:{}", record.timestamp.format("%Y-%m"));
    let day = format!("day:{}", record.timestamp.format("%Y-%m-%d"));
    Ok([
        counter_key(&record.data_source_id, "all", "", "total")?,
        counter_key(&record.data_source_id, "type", &record.event_type, "total")?,
        counter_key(&record.data_source_id, "all", "", &year)?,
        counter_key(&record.data_source_id, "type", &record.event_type, &year)?,
        counter_key(&record.data_source_id, "all", "", &month)?,
        counter_key(&record.data_source_id, "type", &record.event_type, &month)?,
        counter_key(&record.data_source_id, "all", "", &day)?,
        counter_key(&record.data_source_id, "type", &record.event_type, &day)?,
    ])
}
pub(super) fn counter_key(
    source_id: &str,
    scope: &str,
    event_type: &str,
    period: &str,
) -> Result<Vec<u8>> {
    encode_key(
        COUNT_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(scope.to_owned()),
            KeyPart::Text(event_type.to_owned()),
            KeyPart::Text(period.to_owned()),
        ],
    )
}
pub(super) fn read_count(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    event_type: Option<&str>,
    period: &str,
) -> Result<u64> {
    let (scope, kind) = event_type.map_or(("all", ""), |kind| ("type", kind));
    let Some(value) = snapshot.get(
        KeyspaceName::TimelineIndexes,
        &counter_key(source_id, scope, kind, period)?,
    )?
    else {
        return Ok(0);
    };
    if value.len() != 8 {
        return Err(FjallStorageError::TimelineIndexCorrupt);
    }
    Ok(u64::from_be_bytes(
        value.as_slice().try_into().expect("count is 8 bytes"),
    ))
}
