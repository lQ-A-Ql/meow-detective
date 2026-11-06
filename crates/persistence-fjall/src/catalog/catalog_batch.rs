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
        self.source_store
            .database()
            .write_planned(mode, |snapshot| self.upsert_actions(snapshot, records))?;
        Ok(())
    }

    fn upsert_actions(
        &self,
        snapshot: &crate::SourceReadSnapshot,
        records: &[FileEntryRecord],
    ) -> Result<Vec<WriteAction>> {
        let records = records
            .iter()
            .map(|record| ((record.data_source_id.as_str(), record.id.as_str()), record))
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_values()
            .collect::<Vec<_>>();
        let batch_ids = records
            .iter()
            .map(|record| (record.data_source_id.clone(), record.id.clone()))
            .collect::<HashSet<_>>();
        let mut actions = Vec::with_capacity(records.len().saturating_mul(8));
        let mut deltas = crate::record_metrics::CountDeltas::new();
        for record in records {
            let key = record_key(&record.data_source_id, &record.id)?;
            let previous = self.read_record(snapshot, &key, &record.data_source_id, &record.id)?;
            if previous.as_ref().is_some_and(|previous| previous == record) {
                continue;
            }
            if let Some(previous) = previous {
                self.index_deletes(&mut actions, &previous)?;
                if previous.entry_type == super::CatalogEntryType::File {
                    crate::record_metrics::adjust(
                        &mut deltas,
                        "catalog.files",
                        &record.data_source_id,
                        -1,
                    )?;
                }
            } else {
                crate::record_metrics::adjust(&mut deltas, "catalog", &record.data_source_id, 1)?;
            }
            if record.entry_type == super::CatalogEntryType::File {
                crate::record_metrics::adjust(
                    &mut deltas,
                    "catalog.files",
                    &record.data_source_id,
                    1,
                )?;
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
                            .get(snapshot, &record.data_source_id, parent_id)?
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
        if actions.is_empty() {
            return Ok(actions);
        }
        crate::record_metrics::append(snapshot, &mut actions, deltas)?;
        crate::record_metrics::advance(snapshot, &mut actions, "catalog")?;
        Ok(actions)
    }
}
