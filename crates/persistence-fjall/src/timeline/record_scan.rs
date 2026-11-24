use super::{
    decode_record, record_key, KeyPart, KeyspaceName, Result, ScanDirection, SourceReadSnapshot,
    TimelinePage, TimelineRecordStore, VersionedEnvelope, RECORD_FAMILY,
};
use crate::{encode_key, FjallStorageError};

impl TimelineRecordStore {
    /// Page source records in primary-key order for complete batch processing.
    pub fn scan_by_source(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
        cursor: Option<&[u8]>,
        limit: usize,
    ) -> Result<TimelinePage> {
        if limit == 0 {
            return Ok(TimelinePage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let prefix = encode_key(RECORD_FAMILY, &[KeyPart::Text(source.to_owned())])?;
        let entries = snapshot.keyset(
            KeyspaceName::TimelineRecords,
            &prefix,
            cursor,
            ScanDirection::Forward,
            limit.saturating_add(1),
        )?;
        let more = entries.len() > limit;
        let mut records = Vec::with_capacity(entries.len().min(limit));
        let mut last = None;
        for entry in entries.into_iter().take(limit) {
            let record = decode_record(VersionedEnvelope::decode(&entry.value)?)?;
            if record.data_source_id != source || entry.key != record_key(source, &record.id)? {
                return Err(FjallStorageError::TimelineRecordIdentityMismatch);
            }
            last = Some(entry.key);
            records.push(record);
        }
        Ok(TimelinePage {
            records,
            next_cursor: more.then_some(last).flatten(),
        })
    }
}
