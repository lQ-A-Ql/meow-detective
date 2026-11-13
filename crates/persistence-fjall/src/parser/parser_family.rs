use super::{
    index_deletes, index_writes, record_key, validate, ParserRecord, ParserRecordStore,
    FAMILY_INDEX, RECORD_KIND, SCHEMA_VERSION,
};
use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::{SourceReadSnapshot, WriteStream},
    source_store::ensure_not_cancelled,
    FjallStorageError, KeyspaceName, PersistMode, Result, ScanDirection, WriteAction,
    WriteBatchReceipt,
};
use std::{collections::HashSet, sync::atomic::AtomicBool};

impl ParserRecordStore {
    /// Replace one source's complete parser family in a single mutation transaction.
    /// Progress callbacks may read the store but must not start another mutation.
    pub fn replace_family_with_control(
        &self,
        source_id: &str,
        family: &str,
        records: &[ParserRecord],
        mode: PersistMode,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64),
    ) -> Result<WriteBatchReceipt> {
        validate_replacement(source_id, family, records, cancel)?;
        let (_, receipt) =
            self.source_store
                .database()
                .write_streamed(mode, |snapshot, stream| {
                    for record in records {
                        ensure_not_cancelled(cancel)?;
                        if self
                            .get(snapshot, source_id, &record.id)?
                            .is_some_and(|previous| previous.family != family)
                        {
                            return Err(FjallStorageError::InvalidEnvelope(
                                "parser replacement id belongs to another family".into(),
                            ));
                        }
                    }
                    let mut completed = self.delete_family(
                        snapshot,
                        stream,
                        source_id,
                        family,
                        cancel,
                        &mut progress,
                    )?;
                    for record in records {
                        ensure_not_cancelled(cancel)?;
                        write_record(stream, record)?;
                        completed = completed.checked_add(1).ok_or_else(progress_overflow)?;
                        progress(completed);
                        ensure_not_cancelled(cancel)?;
                    }
                    Ok::<_, FjallStorageError>(())
                })?;
        Ok(receipt)
    }

    fn delete_family(
        &self,
        snapshot: &SourceReadSnapshot,
        stream: &mut WriteStream<'_>,
        source_id: &str,
        family: &str,
        cancel: &AtomicBool,
        progress: &mut impl FnMut(u64),
    ) -> Result<u64> {
        let prefix = encode_key(
            FAMILY_INDEX,
            &[
                KeyPart::Text(source_id.into()),
                KeyPart::Text(family.into()),
            ],
        )?;
        let mut cursor = None;
        let mut completed = 0u64;
        loop {
            ensure_not_cancelled(cancel)?;
            let page = snapshot.keyset(
                KeyspaceName::ParserIndexes,
                &prefix,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )?;
            for entry in &page {
                ensure_not_cancelled(cancel)?;
                let envelope = self
                    .source_store
                    .read_envelope(snapshot, KeyspaceName::ParserRecords, &entry.value)?
                    .ok_or(FjallStorageError::ParserIndexCorrupt)?;
                let record = super::decode(envelope)?;
                if record.data_source_id != source_id
                    || record.family != family
                    || record_key(source_id, &record.id)? != entry.value
                {
                    return Err(FjallStorageError::ParserRecordIdentityMismatch);
                }
                let expected = encode_key(
                    FAMILY_INDEX,
                    &[
                        KeyPart::Text(source_id.into()),
                        KeyPart::Text(family.into()),
                        KeyPart::Text(record.id.clone()),
                    ],
                )?;
                if entry.key != expected {
                    return Err(FjallStorageError::ParserIndexCorrupt);
                }
                for action in index_deletes(&record)? {
                    apply(stream, action);
                }
                stream.delete(KeyspaceName::ParserRecords, entry.value.clone());
                completed = completed.checked_add(1).ok_or_else(progress_overflow)?;
                progress(completed);
                ensure_not_cancelled(cancel)?;
            }
            if page.len() < 512 {
                break;
            }
            cursor = page.last().map(|entry| entry.key.clone());
        }
        Ok(completed)
    }
}

fn validate_replacement(
    source_id: &str,
    family: &str,
    records: &[ParserRecord],
    cancel: &AtomicBool,
) -> Result<()> {
    ensure_not_cancelled(cancel)?;
    if source_id.is_empty() || family.is_empty() {
        return Err(FjallStorageError::InvalidEnvelope(
            "parser replacement identity is empty".into(),
        ));
    }
    let mut ids = HashSet::with_capacity(records.len());
    for record in records {
        ensure_not_cancelled(cancel)?;
        validate(record)?;
        if record.data_source_id != source_id
            || record.family != family
            || !ids.insert(record.id.as_str())
        {
            return Err(FjallStorageError::InvalidEnvelope(
                "parser replacement crossed ownership or repeated an id".into(),
            ));
        }
    }
    Ok(())
}

fn write_record(stream: &mut WriteStream<'_>, record: &ParserRecord) -> Result<()> {
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
    stream.put(
        KeyspaceName::ParserRecords,
        record_key(&record.data_source_id, &record.id)?,
        envelope.encode()?,
    );
    for action in index_writes(record)? {
        apply(stream, action);
    }
    Ok(())
}

fn apply(stream: &mut WriteStream<'_>, action: WriteAction) {
    match action {
        WriteAction::Put {
            keyspace,
            key,
            value,
        } => stream.put(keyspace, key, value),
        WriteAction::Delete { keyspace, key } => stream.delete(keyspace, key),
    }
}

fn progress_overflow() -> FjallStorageError {
    FjallStorageError::InvalidEnvelope("parser replacement progress overflow".into())
}

#[cfg(test)]
#[path = "../../tests/unit/parser_family.rs"]
mod tests;
