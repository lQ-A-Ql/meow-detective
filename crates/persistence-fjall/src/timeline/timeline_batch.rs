use super::{
    index_keys, record_key, validate_record, TimelineRecord, TimelineRecordStore, RECORD_KIND,
    SCHEMA_VERSION,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, WriteAction},
    keyspaces::KeyspaceName,
    Result,
};

impl TimelineRecordStore {
    pub fn upsert_batch(&self, records: &[TimelineRecord], mode: PersistMode) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let snapshot = self.read_snapshot();
        let mut actions = Vec::with_capacity(records.len().saturating_mul(6));
        for record in records {
            validate_record(record)?;
            let key = record_key(&record.data_source_id, &record.id)?;
            if let Some(previous) = self.get(&snapshot, &record.data_source_id, &record.id)? {
                for index in index_keys(&previous)? {
                    actions.push(WriteAction::Delete {
                        keyspace: KeyspaceName::TimelineIndexes,
                        key: index,
                    });
                }
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
                keyspace: KeyspaceName::TimelineRecords,
                key,
                value: envelope.encode()?,
            });
            let record_key = record_key(&record.data_source_id, &record.id)?;
            for index in index_keys(record)? {
                actions.push(WriteAction::Put {
                    keyspace: KeyspaceName::TimelineIndexes,
                    key: index,
                    value: record_key.clone(),
                });
            }
        }
        let revision = self.cursor_revision(&snapshot)?.saturating_add(1);
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::Meta,
            key: super::revision_key("timeline")?,
            value: revision.to_be_bytes().to_vec(),
        });
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }
}
