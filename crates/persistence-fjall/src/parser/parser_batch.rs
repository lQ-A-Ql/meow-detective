use super::{
    index_deletes, index_writes, index_writes_without_fields, record_key, validate, ParserRecord,
    ParserRecordStore, RECORD_KIND, SCHEMA_VERSION,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, WriteAction},
    keyspaces::KeyspaceName,
    Result,
};

impl ParserRecordStore {
    pub fn upsert_batch(&self, records: &[ParserRecord], mode: PersistMode) -> Result<()> {
        self.upsert_batch_with_field_indexes(records, mode, true)
    }

    pub fn upsert_batch_without_field_indexes(
        &self,
        records: &[ParserRecord],
        mode: PersistMode,
    ) -> Result<()> {
        self.upsert_batch_with_field_indexes(records, mode, false)
    }

    fn upsert_batch_with_field_indexes(
        &self,
        records: &[ParserRecord],
        mode: PersistMode,
        include_field_indexes: bool,
    ) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let snapshot = self.read_snapshot();
        let mut actions = Vec::with_capacity(records.len().saturating_mul(4));
        for record in records {
            validate(record)?;
            let key = record_key(&record.data_source_id, &record.id)?;
            let previous = self.get(&snapshot, &record.data_source_id, &record.id)?;
            if previous.as_ref().is_some_and(|previous| previous == record) {
                continue;
            }
            if let Some(previous) = previous {
                actions.extend(index_deletes(&previous)?);
            }
            let payload = serde_json::to_vec(record).map_err(|error| {
                crate::errors::FjallStorageError::Serialization(error.to_string())
            })?;
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
            actions.extend(if include_field_indexes {
                index_writes(record)?
            } else {
                index_writes_without_fields(record)?
            });
        }
        if actions.is_empty() {
            return Ok(());
        }
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }
}
