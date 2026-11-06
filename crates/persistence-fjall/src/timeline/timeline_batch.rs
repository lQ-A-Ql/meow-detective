use super::{
    index_keys, record_key, validate_record, TimelineRecord, TimelineRecordStore, RECORD_KIND,
    SCHEMA_VERSION,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, SourceReadSnapshot, WriteAction},
    keyspaces::KeyspaceName,
    Result,
};
use std::collections::{BTreeMap, HashMap};

impl TimelineRecordStore {
    pub fn upsert_batch(&self, records: &[TimelineRecord], mode: PersistMode) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        self.source_store
            .database()
            .write_planned(mode, |snapshot| {
                self.mutation_actions(snapshot, records, &[])
            })?;
        Ok(())
    }

    pub fn replace_by_parser_prefix(
        &self,
        source_id: &str,
        prefix: &str,
        records: &[TimelineRecord],
        mode: PersistMode,
    ) -> Result<()> {
        if prefix.is_empty()
            || records.iter().any(|record| {
                record.data_source_id != source_id
                    || !record
                        .parser_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with(prefix))
            })
        {
            return Err(crate::FjallStorageError::InvalidEnvelope(
                "timeline replacement identity is invalid".into(),
            ));
        }
        self.source_store
            .database()
            .write_planned(mode, |snapshot| {
                let mut cursor = None;
                let mut deleted = Vec::new();
                loop {
                    let page = self.list_by_source(
                        snapshot,
                        source_id,
                        cursor.as_deref(),
                        crate::ScanDirection::Forward,
                        512,
                    )?;
                    deleted.extend(page.records.into_iter().filter(|record| {
                        record
                            .parser_id
                            .as_deref()
                            .is_some_and(|id| id.starts_with(prefix))
                    }));
                    cursor = page.next_cursor;
                    if cursor.is_none() {
                        break;
                    }
                }
                self.mutation_actions(snapshot, records, &deleted)
            })?;
        Ok(())
    }

    fn mutation_actions(
        &self,
        snapshot: &SourceReadSnapshot,
        records: &[TimelineRecord],
        deleted: &[TimelineRecord],
    ) -> Result<Vec<WriteAction>> {
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
        for record in deleted {
            count_policies
                .entry(record.data_source_id.clone())
                .or_insert(super::timeline_count::count_write_policy(
                    snapshot,
                    &record.data_source_id,
                )?);
            if count_policies[&record.data_source_id].0 {
                super::timeline_count::adjust_count_deltas(&mut count_deltas, record, -1)?;
            }
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::TimelineRecords,
                key: record_key(&record.data_source_id, &record.id)?,
            });
            for key in index_keys(record)? {
                actions.push(WriteAction::Delete {
                    keyspace: KeyspaceName::TimelineIndexes,
                    key,
                });
            }
        }
        let deleted_ids = deleted
            .iter()
            .map(|record| (record.data_source_id.as_str(), record.id.as_str()))
            .collect::<std::collections::HashSet<_>>();
        for ((source_id, _), record) in &latest_records {
            if !count_policies.contains_key(source_id) {
                count_policies.insert(
                    source_id.clone(),
                    super::timeline_count::count_write_policy(snapshot, source_id)?,
                );
            }
            let previous =
                if deleted_ids.contains(&(record.data_source_id.as_str(), record.id.as_str())) {
                    None
                } else {
                    self.get(snapshot, &record.data_source_id, &record.id)?
                };
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
            actions.push(record_action(record)?);
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
            return Ok(actions);
        }
        self.finish_mutations(snapshot, actions, count_deltas, count_policies)
    }

    fn finish_mutations(
        &self,
        snapshot: &SourceReadSnapshot,
        mut actions: Vec<WriteAction>,
        count_deltas: HashMap<Vec<u8>, i64>,
        count_policies: BTreeMap<String, (bool, bool)>,
    ) -> Result<Vec<WriteAction>> {
        super::timeline_count::append_count_actions(snapshot, &mut actions, count_deltas)?;
        for (source_id, (_, initialize)) in count_policies {
            if initialize {
                actions.push(super::timeline_count::count_schema_marker_action(
                    &source_id,
                )?);
            }
        }
        let revision = self
            .cursor_revision(snapshot)?
            .checked_add(1)
            .ok_or_else(|| {
                crate::FjallStorageError::InvalidEnvelope("timeline revision overflowed".into())
            })?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::Meta,
            key: super::revision_key("timeline")?,
            value: revision.to_be_bytes().to_vec(),
        });
        Ok(actions)
    }
}

fn record_action(record: &TimelineRecord) -> Result<WriteAction> {
    let payload = serde_json::to_vec(record)
        .map_err(|error| crate::FjallStorageError::Serialization(error.to_string()))?;
    let envelope = VersionedEnvelope::new(
        RECORD_KIND,
        SCHEMA_VERSION,
        &record.id,
        &record.data_source_id,
        1,
        payload,
    );
    Ok(WriteAction::Put {
        keyspace: KeyspaceName::TimelineRecords,
        key: record_key(&record.data_source_id, &record.id)?,
        value: envelope.encode()?,
    })
}
