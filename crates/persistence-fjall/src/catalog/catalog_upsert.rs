use super::{
    index_write, orphan_key, record_key, tombstone_key, CatalogEntryType, CatalogRecordStore,
    FileEntryRecord, RECORD_KIND, SCHEMA_VERSION,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, WriteAction},
    keyspaces::KeyspaceName,
    Result,
};

impl CatalogRecordStore {
    pub fn upsert(&self, record: &FileEntryRecord, mode: PersistMode) -> Result<()> {
        let key = record_key(&record.data_source_id, &record.id)?;
        let snapshot = self.read_snapshot();
        let old = self.read_record(&snapshot, &key, &record.data_source_id, &record.id)?;
        if old.as_ref().is_some_and(|previous| previous == record) {
            return Ok(());
        }
        let mut actions = Vec::with_capacity(8);
        if let Some(previous) = old {
            self.index_deletes(&mut actions, &previous)?;
        }
        let envelope = VersionedEnvelope::from_serializable(
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
        if record.entry_type == CatalogEntryType::Directory {
            if let Some(parent_id) = record.parent_id.as_deref() {
                if parent_id == record.id
                    || self
                        .get(&snapshot, &record.data_source_id, parent_id)?
                        .is_none()
                {
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
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }
}
