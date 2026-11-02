use super::{
    index_write, orphan_key, record_key, tombstone_key, CatalogRecordStore, FileEntryRecord,
    RECORD_KIND, SCHEMA_VERSION,
};
use crate::keyspaces::KeyspaceName;
use crate::{database::WriteAction, PersistMode, Result};
use std::collections::HashSet;

impl CatalogRecordStore {
    pub fn upsert_batch(&self, records: &[FileEntryRecord], mode: PersistMode) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let snapshot = self.read_snapshot();
        let batch_ids = records
            .iter()
            .map(|record| (record.data_source_id.clone(), record.id.clone()))
            .collect::<HashSet<_>>();
        let mut actions = Vec::with_capacity(records.len().saturating_mul(8));
        for record in records {
            let key = record_key(&record.data_source_id, &record.id)?;
            if let Some(previous) =
                self.read_record(&snapshot, &key, &record.data_source_id, &record.id)?
            {
                self.index_deletes(&mut actions, &previous)?;
            }
            let envelope = crate::codec::VersionedEnvelope::from_serializable(
                RECORD_KIND,
                SCHEMA_VERSION,
                &record.id,
                &record.data_source_id,
                1,
                record,
            )?;
            actions.push(WriteAction::Put {
                keyspace: KeyspaceName::CatalogRecords,
                key,
                value: envelope.encode()?,
            });
            self.index_writes(&mut actions, record)?;
            if record.entry_type == super::CatalogEntryType::Directory {
                if let Some(parent_id) = record.parent_id.as_deref() {
                    let parent_exists = batch_ids
                        .contains(&(record.data_source_id.clone(), parent_id.to_owned()))
                        || self
                            .get(&snapshot, &record.data_source_id, parent_id)?
                            .is_some();
                    if parent_id == record.id || !parent_exists {
                        actions.push(index_write(
                            orphan_key(record)?,
                            record_key(&record.data_source_id, &record.id)?,
                        ));
                    }
                }
            }
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::Tombstones,
                key: tombstone_key(&record.data_source_id, &record.id)?,
            });
        }
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }
}
