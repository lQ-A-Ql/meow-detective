use super::{
    index_keys, timeline_batch::record_action, validate_record, TimelineRecord, TimelineRecordStore,
};
use crate::{FjallStorageError, Result, WriteAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineRecordStorageCost {
    pub write_bytes: u64,
    pub delete_bytes: u64,
    pub entries: u64,
}

impl TimelineRecordStore {
    /// Logical bytes for the record and its indexes, excluding aggregate counts
    /// and engine allocation overhead.
    pub fn record_storage_cost(record: &TimelineRecord) -> Result<TimelineRecordStorageCost> {
        validate_record(record)?;
        let WriteAction::Put { key, value, .. } = record_action(record)? else {
            return Err(FjallStorageError::InvalidEnvelope(
                "timeline record is not a put action".into(),
            ));
        };
        let record_key_bytes = key.len() as u64;
        let mut cost = TimelineRecordStorageCost {
            write_bytes: add_bytes(record_key_bytes, value.len() as u64)?,
            delete_bytes: record_key_bytes,
            entries: 1,
        };
        for key in index_keys(record)? {
            cost.delete_bytes = add_bytes(cost.delete_bytes, key.len() as u64)?;
            cost.write_bytes = add_bytes(
                cost.write_bytes,
                add_bytes(key.len() as u64, record_key_bytes)?,
            )?;
            cost.entries += 1;
        }
        Ok(cost)
    }
}

fn add_bytes(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right).ok_or_else(|| {
        FjallStorageError::InvalidEnvelope("timeline storage byte count overflowed".into())
    })
}

#[cfg(test)]
#[path = "../../tests/unit/timeline_storage_cost.rs"]
mod tests;
