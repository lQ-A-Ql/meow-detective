use super::{
    index_deletes, index_writes, record_key, type_counts, validate, ArtifactRecord,
    ArtifactRecordStore, RECORD_KIND, SCHEMA_VERSION,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, WriteAction, WriteStream},
    keyspaces::KeyspaceName,
    Result,
};

impl ArtifactRecordStore {
    pub fn upsert_batch(&self, records: &[ArtifactRecord], mode: PersistMode) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        self.source_store
            .database()
            .write_planned(mode, |snapshot| self.upsert_actions(snapshot, records))?;
        Ok(())
    }

    pub fn upsert_batch_independent(
        &self,
        records: &[ArtifactRecord],
        mode: PersistMode,
    ) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let mut actions = Vec::with_capacity(records.len().saturating_mul(4));
        let sources = records
            .iter()
            .map(|record| &record.data_source_id)
            .collect::<std::collections::BTreeSet<_>>();
        // Independent writes cannot maintain counts; invalidate them atomically until rebuilt.
        for source_id in sources {
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::Meta,
                key: type_counts::marker_key(source_id)?,
            });
        }
        for record in records {
            validate(record)?;
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
            actions.push(WriteAction::Put {
                keyspace: KeyspaceName::ArtifactRecords,
                key: record_key(&record.data_source_id, &record.id)?,
                value: envelope.encode()?,
            });
            actions.extend(index_writes(record)?);
        }
        self.source_store.database().write_batch(&actions, mode)?;
        Ok(())
    }

    pub fn delete_by_source_object_where<F>(
        &self,
        source_id: &str,
        source_object_id: &str,
        mode: PersistMode,
        mut predicate: F,
    ) -> Result<usize>
    where
        F: FnMut(&ArtifactRecord) -> bool,
    {
        let ((deleted, _), _) =
            self.source_store
                .database()
                .write_streamed(mode, |snapshot, writer| {
                    let mut deltas = crate::record_metrics::CountDeltas::new();
                    let mut type_deltas = type_counts::TypeCountDeltas::new();
                    let counts_ready = type_counts::counts_ready(snapshot, source_id)?;
                    let mut deleted = 0usize;
                    let mut cursor = None;
                    loop {
                        let page = self.list_by_source_object(
                            snapshot,
                            source_id,
                            source_object_id,
                            cursor.as_deref(),
                            crate::ScanDirection::Forward,
                            512,
                        )?;
                        for record in page.records {
                            if !predicate(&record) {
                                continue;
                            }
                            writer.delete(
                                KeyspaceName::ArtifactRecords,
                                record_key(source_id, &record.id)?,
                            );
                            for action in index_deletes(&record)? {
                                stream_action(writer, action);
                            }
                            crate::record_metrics::adjust(&mut deltas, "artifact", source_id, -1)?;
                            if counts_ready {
                                type_counts::adjust(
                                    &mut type_deltas,
                                    source_id,
                                    &record.artifact_type,
                                    -1,
                                )?;
                            }
                            deleted += 1;
                        }
                        cursor = page.next_cursor;
                        if cursor.is_none() {
                            break;
                        }
                    }
                    if deleted > 0 {
                        let mut actions = Vec::new();
                        crate::record_metrics::append(snapshot, &mut actions, deltas)?;
                        type_counts::append_actions(snapshot, &mut actions, type_deltas)?;
                        actions.push(self.revision_action(snapshot)?);
                        for action in actions {
                            stream_action(writer, action);
                        }
                    }
                    Ok::<(usize, ()), crate::FjallStorageError>((deleted, ()))
                })?;
        Ok(deleted)
    }

    fn upsert_actions(
        &self,
        snapshot: &crate::SourceReadSnapshot,
        records: &[ArtifactRecord],
    ) -> Result<Vec<WriteAction>> {
        let mut actions = Vec::with_capacity(records.len().saturating_mul(4));
        let mut deltas = crate::record_metrics::CountDeltas::new();
        let mut type_deltas = type_counts::TypeCountDeltas::new();
        let mut policies = std::collections::BTreeMap::<String, type_counts::CountPolicy>::new();
        let mut bootstrap = std::collections::BTreeSet::new();
        let latest = records
            .iter()
            .map(|record| ((record.data_source_id.as_str(), record.id.as_str()), record))
            .collect::<std::collections::BTreeMap<_, _>>();
        for record in latest.into_values() {
            validate(record)?;
            let policy = match policies.get(&record.data_source_id) {
                Some(policy) => *policy,
                None => {
                    let policy = type_counts::count_policy(snapshot, &record.data_source_id)?;
                    if policy == type_counts::CountPolicy::Bootstrap {
                        bootstrap.insert(record.data_source_id.clone());
                    }
                    policies.insert(record.data_source_id.clone(), policy);
                    policy
                }
            };
            let previous = self.get(snapshot, &record.data_source_id, &record.id)?;
            if previous.as_ref().is_some_and(|previous| previous == record) {
                continue;
            }
            if let Some(previous) = previous {
                actions.extend(index_deletes(&previous)?);
                if policy.maintains() {
                    type_counts::adjust(
                        &mut type_deltas,
                        &record.data_source_id,
                        &previous.artifact_type,
                        -1,
                    )?;
                }
            } else {
                crate::record_metrics::adjust(&mut deltas, "artifact", &record.data_source_id, 1)?;
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
                keyspace: KeyspaceName::ArtifactRecords,
                key: record_key(&record.data_source_id, &record.id)?,
                value: envelope.encode()?,
            });
            actions.extend(index_writes(record)?);
            if policy.maintains() {
                type_counts::adjust(
                    &mut type_deltas,
                    &record.data_source_id,
                    &record.artifact_type,
                    1,
                )?;
            }
        }
        if actions.is_empty() {
            return Ok(actions);
        }
        crate::record_metrics::append(snapshot, &mut actions, deltas)?;
        type_counts::append_actions(snapshot, &mut actions, type_deltas)?;
        for source_id in bootstrap {
            actions.push(type_counts::marker_action(&source_id)?);
        }
        actions.push(self.revision_action(snapshot)?);
        Ok(actions)
    }
}

fn stream_action(writer: &mut WriteStream<'_>, action: WriteAction) {
    match action {
        WriteAction::Put {
            keyspace,
            key,
            value,
        } => writer.put(keyspace, key, value),
        WriteAction::Delete { keyspace, key } => writer.delete(keyspace, key),
    }
}
