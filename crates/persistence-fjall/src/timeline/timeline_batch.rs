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
use std::collections::{BTreeMap, HashMap};

impl TimelineRecordStore {
    pub fn upsert_batch(&self, records: &[TimelineRecord], mode: PersistMode) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let snapshot = self.read_snapshot();
        let mut latest_records = BTreeMap::new();
        for record in records {
            validate_record(record)?;
            latest_records.insert(
                (record.data_source_id.clone(), record.id.clone()),
                record.clone(),
            );
        }
        let mut actions = Vec::with_capacity(latest_records.len().saturating_mul(14));
        let mut count_deltas = HashMap::new();
        let mut count_policies = BTreeMap::new();
        for ((source_id, _), record) in &latest_records {
            if !count_policies.contains_key(source_id) {
                count_policies.insert(
                    source_id.clone(),
                    super::timeline_count::count_write_policy(&snapshot, source_id)?,
                );
            }
            let key = record_key(&record.data_source_id, &record.id)?;
            let previous = self.get(&snapshot, &record.data_source_id, &record.id)?;
            if previous.as_ref().is_some_and(|previous| previous == record) {
                continue;
            }
            let update_counts = count_policies.get(source_id).is_some_and(|policy| policy.0);
            if let Some(previous) = previous {
                if update_counts {
                    super::timeline_count::adjust_count_deltas(&mut count_deltas, &previous, -1)?;
                }
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
            if update_counts {
                super::timeline_count::adjust_count_deltas(&mut count_deltas, record, 1)?;
            }
        }
        if actions.is_empty() {
            return Ok(());
        }
        super::timeline_count::append_count_actions(&snapshot, &mut actions, count_deltas)?;
        for (source_id, (_, initialize)) in count_policies {
            if initialize {
                actions.push(super::timeline_count::count_schema_marker_action(
                    &source_id,
                )?);
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
