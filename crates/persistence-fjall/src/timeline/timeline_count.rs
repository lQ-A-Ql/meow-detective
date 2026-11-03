use super::{decode_record, record_key, TimelineRecordStore, TIME_FAMILY, TYPE_FAMILY};
use crate::{
    codec::KeyPart,
    database::SourceReadSnapshot,
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
};
use chrono::{DateTime, Utc};

impl TimelineRecordStore {
    pub fn count_by_time(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<u64> {
        let prefix = crate::codec::encode_key(TIME_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
        count_matching(snapshot, &prefix, start, end)
    }

    pub fn count_by_type(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<u64> {
        let prefix = crate::codec::encode_key(
            TYPE_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(event_type.to_owned()),
            ],
        )?;
        count_matching(snapshot, &prefix, start, end)
    }
}

fn count_matching(
    snapshot: &SourceReadSnapshot,
    prefix: &[u8],
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<u64> {
    let mut count = 0u64;
    let mut cursor = None;
    loop {
        let entries = snapshot.keyset(
            KeyspaceName::TimelineIndexes,
            prefix,
            cursor.as_deref(),
            ScanDirection::Forward,
            512,
        )?;
        if entries.is_empty() {
            break;
        }
        for entry in &entries {
            let Some(envelope) = snapshot.get(KeyspaceName::TimelineRecords, &entry.value)? else {
                return Err(FjallStorageError::TimelineIndexCorrupt);
            };
            let record = decode_record(crate::codec::VersionedEnvelope::decode(&envelope)?)?;
            if record_key(&record.data_source_id, &record.id)? != entry.value {
                return Err(FjallStorageError::TimelineRecordIdentityMismatch);
            }
            if start.is_none_or(|bound| record.timestamp >= bound)
                && end.is_none_or(|bound| record.timestamp <= bound)
            {
                count = count.saturating_add(1);
            }
        }
        if entries.len() < 512 {
            break;
        }
        cursor = entries.last().map(|entry| entry.key.clone());
    }
    Ok(count)
}
