use super::{
    index_deletes, index_writes, record_key, validate, ParserRecord, ParserRecordStore,
    RECORD_KIND, SCHEMA_VERSION,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, WriteAction},
    keyspaces::KeyspaceName,
    Result,
};

impl ParserRecordStore {
    pub fn upsert_batch(&self, records: &[ParserRecord], mode: PersistMode) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let snapshot = self.read_snapshot();
        let mut actions = Vec::with_capacity(records.len().saturating_mul(4));
        for record in records {
            validate(record)?;
            let key = record_key(&record.data_source_id, &record.id)?;
            if let Some(previous) = self.get(&snapshot, &record.data_source_id, &record.id)? {
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
            actions.extend(index_writes(record)?);
        }
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }
}
