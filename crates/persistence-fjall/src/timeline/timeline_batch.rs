use super::{
    index_keys, record_key, validate_record, TimelineRecord, TimelineRecordStore, RECORD_KIND,
    SCHEMA_VERSION,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, SourceReadSnapshot, WriteAction, WriteStream},
    keyspaces::KeyspaceName,
    FjallStorageError, Result,
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

    pub fn upsert_batch_independent(
        &self,
        records: &[TimelineRecord],
        mode: PersistMode,
    ) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let mut actions = Vec::with_capacity(records.len().saturating_mul(8));
        for record in records {
            validate_record(record)?;
            let record_key = record_key(&record.data_source_id, &record.id)?;
            actions.push(record_action(record)?);
            for index in index_keys(record)? {
                actions.push(WriteAction::Put {
                    keyspace: KeyspaceName::TimelineIndexes,
                    key: index,
                    value: record_key.clone(),
                });
            }
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
        F: FnMut(&TimelineRecord) -> bool,
    {
        let ((deleted, _), _) =
            self.source_store
                .database()
                .write_streamed(mode, |snapshot, writer| {
                    let mut count_deltas = HashMap::new();
                    let mut count_policies = BTreeMap::new();
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
                            let policy = count_policies
                                .entry(record.data_source_id.clone())
                                .or_insert(super::timeline_count::count_write_policy(
                                    snapshot,
                                    &record.data_source_id,
                                )?);
                            if policy.0 {
                                super::timeline_count::adjust_count_deltas(
                                    &mut count_deltas,
                                    &record,
                                    -1,
                                )?;
                            }
                            stream_delete(writer, &record)?;
                            deleted += 1;
                        }
                        cursor = page.next_cursor;
                        if cursor.is_none() {
                            break;
                        }
                    }
                    if deleted > 0 {
                        self.finish_streamed_mutations(
                            snapshot,
                            writer,
                            count_deltas,
                            count_policies,
                        )?;
                    }
                    Ok::<(usize, ()), FjallStorageError>((deleted, ()))
                })?;
        Ok(deleted)
    }

    pub fn replace_by_parser_prefix(
        &self,
        source_id: &str,
        prefix: &str,
        records: &[TimelineRecord],
        mode: PersistMode,
    ) -> Result<()> {
        let mut records = Some(records.to_vec());
        self.replace_by_parser_prefix_streamed(
            source_id,
            prefix,
            mode,
            move || -> Result<Option<Vec<TimelineRecord>>> { Ok(records.take()) },
        )
        .map(|_| ())
        .map_err(|error| match error {
            crate::TimelineStreamError::Storage(error) => error,
            crate::TimelineStreamError::Producer(error) => error,
        })
    }

    pub fn replace_by_parser_prefix_streamed<E, F>(
        &self,
        source_id: &str,
        prefix: &str,
        mode: PersistMode,
        mut next_batch: F,
    ) -> std::result::Result<u64, crate::TimelineStreamError<E>>
    where
        E: std::error::Error + 'static,
        F: FnMut() -> std::result::Result<Option<Vec<TimelineRecord>>, E>,
    {
        if prefix.is_empty() {
            return Err(FjallStorageError::InvalidEnvelope(
                "timeline replacement prefix is empty".into(),
            )
            .into());
        }
        let ((inserted, _), _) =
            self.source_store
                .database()
                .write_streamed(mode, |snapshot, writer| {
                    let mut count_deltas = HashMap::new();
                    let mut count_policies = BTreeMap::new();
                    let mut changed = self
                        .delete_parser_prefix(
                            snapshot,
                            writer,
                            source_id,
                            prefix,
                            &mut count_deltas,
                            &mut count_policies,
                        )
                        .map_err(crate::TimelineStreamError::Storage)?;
                    let mut inserted = 0u64;
                    while let Some(records) =
                        next_batch().map_err(crate::TimelineStreamError::Producer)?
                    {
                        if records.is_empty() {
                            return Err(crate::TimelineStreamError::Storage(
                                FjallStorageError::InvalidEnvelope(
                                    "streamed timeline replacement produced an empty batch".into(),
                                ),
                            ));
                        }
                        for record in &records {
                            validate_replacement_record(record, source_id, prefix)
                                .map_err(crate::TimelineStreamError::Storage)?;
                            changed |= self
                                .stream_upsert(
                                    snapshot,
                                    writer,
                                    record,
                                    &mut count_deltas,
                                    &mut count_policies,
                                )
                                .map_err(crate::TimelineStreamError::Storage)?;
                        }
                        inserted = inserted.saturating_add(records.len() as u64);
                    }
                    if changed {
                        self.finish_streamed_mutations(
                            snapshot,
                            writer,
                            count_deltas,
                            count_policies,
                        )
                        .map_err(crate::TimelineStreamError::Storage)?;
                    }
                    Ok((inserted, ()))
                })?;
        Ok(inserted)
    }

    fn delete_parser_prefix(
        &self,
        snapshot: &SourceReadSnapshot,
        writer: &mut WriteStream<'_>,
        source_id: &str,
        prefix: &str,
        count_deltas: &mut HashMap<Vec<u8>, i64>,
        count_policies: &mut BTreeMap<String, (bool, bool)>,
    ) -> Result<bool> {
        let mut cursor = None;
        let mut deleted = false;
        loop {
            let page = self.list_by_source(
                snapshot,
                source_id,
                cursor.as_deref(),
                crate::ScanDirection::Forward,
                512,
            )?;
            for record in page.records {
                if !record
                    .parser_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(prefix))
                {
                    continue;
                }
                let policy = count_policies
                    .entry(record.data_source_id.clone())
                    .or_insert(super::timeline_count::count_write_policy(
                        snapshot,
                        &record.data_source_id,
                    )?);
                if policy.0 {
                    super::timeline_count::adjust_count_deltas(count_deltas, &record, -1)?;
                }
                stream_delete(writer, &record)?;
                deleted = true;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(deleted);
            }
        }
    }

    fn stream_upsert(
        &self,
        snapshot: &SourceReadSnapshot,
        writer: &mut WriteStream<'_>,
        record: &TimelineRecord,
        count_deltas: &mut HashMap<Vec<u8>, i64>,
        count_policies: &mut BTreeMap<String, (bool, bool)>,
    ) -> Result<bool> {
        let existing = writer
            .get(
                KeyspaceName::TimelineRecords,
                &record_key(&record.data_source_id, &record.id)?,
            )?
            .map(|envelope| {
                VersionedEnvelope::decode(&envelope).and_then(|envelope| {
                    if envelope.record_id != record.id
                        || envelope.source_id != record.data_source_id
                    {
                        return Err(FjallStorageError::TimelineRecordIdentityMismatch);
                    }
                    super::decode_record(envelope)
                })
            })
            .transpose()?;
        if existing.as_ref().is_some_and(|existing| existing == record) {
            return Ok(false);
        }
        let policy = count_policies
            .entry(record.data_source_id.clone())
            .or_insert(super::timeline_count::count_write_policy(
                snapshot,
                &record.data_source_id,
            )?);
        if let Some(existing) = existing.as_ref() {
            if policy.0 {
                super::timeline_count::adjust_count_deltas(count_deltas, existing, -1)?;
            }
            stream_delete(writer, existing)?;
        }
        stream_action(writer, record_action(record)?);
        for index in index_keys(record)? {
            writer.put(
                KeyspaceName::TimelineIndexes,
                index,
                record_key(&record.data_source_id, &record.id)?,
            );
        }
        if policy.0 {
            super::timeline_count::adjust_count_deltas(count_deltas, record, 1)?;
        }
        Ok(true)
    }

    fn finish_streamed_mutations(
        &self,
        snapshot: &SourceReadSnapshot,
        writer: &mut WriteStream<'_>,
        count_deltas: HashMap<Vec<u8>, i64>,
        count_policies: BTreeMap<String, (bool, bool)>,
    ) -> Result<()> {
        let mut actions = Vec::new();
        super::timeline_count::append_count_actions(snapshot, &mut actions, count_deltas)?;
        for source_id in count_policies
            .into_iter()
            .filter_map(|(source_id, (_, initialize))| initialize.then_some(source_id))
        {
            actions.push(super::timeline_count::count_schema_marker_action(
                &source_id,
            )?);
        }
        let revision = self
            .cursor_revision(snapshot)?
            .checked_add(1)
            .ok_or_else(|| {
                FjallStorageError::InvalidEnvelope("timeline revision overflowed".into())
            })?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::Meta,
            key: super::revision_key("timeline")?,
            value: revision.to_be_bytes().to_vec(),
        });
        for action in actions {
            stream_action(writer, action);
        }
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

fn validate_replacement_record(
    record: &TimelineRecord,
    source_id: &str,
    prefix: &str,
) -> Result<()> {
    validate_record(record)?;
    if record.data_source_id != source_id
        || !record
            .parser_id
            .as_deref()
            .is_some_and(|id| id.starts_with(prefix))
    {
        return Err(FjallStorageError::InvalidEnvelope(
            "timeline replacement identity is invalid".into(),
        ));
    }
    Ok(())
}

fn stream_delete(writer: &mut WriteStream<'_>, record: &TimelineRecord) -> Result<()> {
    writer.delete(
        KeyspaceName::TimelineRecords,
        record_key(&record.data_source_id, &record.id)?,
    );
    for index in index_keys(record)? {
        writer.delete(KeyspaceName::TimelineIndexes, index);
    }
    Ok(())
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
