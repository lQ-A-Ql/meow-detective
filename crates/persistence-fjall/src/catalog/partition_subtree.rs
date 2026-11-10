use super::{
    index_write, orphan_key, record_key, tombstone_key, CatalogRecordStore, FileEntryRecord,
    RECORD_KIND, SCHEMA_VERSION,
};
use crate::{
    database::{PersistMode, WriteAction, WriteStream},
    keyspaces::KeyspaceName,
    FjallStorageError, Result, ScanDirection, SourceReadSnapshot,
};

impl CatalogRecordStore {
    pub fn assign_partition_to_subtree(
        &self,
        source_id: &str,
        root_id: &str,
        partition_index: u32,
        mode: PersistMode,
    ) -> Result<usize> {
        let (updated, _) =
            self.source_store
                .database()
                .write_streamed(mode, |snapshot, writer| {
                    let max_nodes = usize::try_from(self.count(snapshot, Some(source_id))?)
                        .map_err(|_| {
                            FjallStorageError::InvalidEnvelope(
                                "catalog count exceeds platform range".into(),
                            )
                        })?;
                    let mut state = SubtreeState {
                        partition_index,
                        max_nodes,
                        visited: 0,
                        changed: false,
                    };
                    self.update_subtree(snapshot, writer, source_id, root_id, 0, &mut state)?;
                    if state.changed {
                        let mut actions = Vec::new();
                        crate::record_metrics::advance(snapshot, &mut actions, "catalog")?;
                        for action in actions {
                            stream_action(writer, action);
                        }
                    }
                    Ok::<usize, FjallStorageError>(state.visited)
                })?;
        Ok(updated)
    }

    fn update_subtree(
        &self,
        snapshot: &SourceReadSnapshot,
        writer: &mut WriteStream<'_>,
        source_id: &str,
        record_id: &str,
        depth: usize,
        state: &mut SubtreeState,
    ) -> Result<()> {
        if depth > state.max_nodes {
            return Err(FjallStorageError::InvalidEnvelope(
                "catalog subtree contains a parent cycle".into(),
            ));
        }
        let Some(record) = self.get(snapshot, source_id, record_id)? else {
            return Ok(());
        };
        state.visited = state.visited.saturating_add(1);
        if state.visited > state.max_nodes {
            return Err(FjallStorageError::InvalidEnvelope(
                "catalog subtree exceeds catalog cardinality".into(),
            ));
        }
        let mut updated = record.clone();
        updated.partition_index = Some(state.partition_index);
        if updated != record {
            stream_record_update(self, snapshot, writer, &record, &updated)?;
            state.changed = true;
        }
        if record.entry_type != super::CatalogEntryType::Directory {
            return Ok(());
        }
        let mut cursor = None;
        loop {
            let page = self.list_by_parent(
                snapshot,
                source_id,
                Some(record_id),
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )?;
            for child in page.records {
                if child.id != record_id {
                    self.update_subtree(
                        snapshot,
                        writer,
                        source_id,
                        &child.id,
                        depth.saturating_add(1),
                        state,
                    )?;
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(());
            }
        }
    }
}

struct SubtreeState {
    partition_index: u32,
    max_nodes: usize,
    visited: usize,
    changed: bool,
}

fn stream_record_update(
    repository: &CatalogRecordStore,
    snapshot: &SourceReadSnapshot,
    writer: &mut WriteStream<'_>,
    previous: &FileEntryRecord,
    updated: &FileEntryRecord,
) -> Result<()> {
    let mut actions = Vec::new();
    repository.index_deletes(&mut actions, previous)?;
    let envelope = crate::codec::VersionedEnvelope::from_serializable(
        RECORD_KIND,
        SCHEMA_VERSION,
        &updated.id,
        &updated.data_source_id,
        1,
        updated,
    )?;
    actions.push(WriteAction::Put {
        keyspace: KeyspaceName::CatalogRecords,
        key: record_key(&updated.data_source_id, &updated.id)?,
        value: envelope.encode()?,
    });
    repository.index_writes(&mut actions, updated)?;
    if updated.entry_type == super::CatalogEntryType::Directory {
        if let Some(parent_id) = updated.parent_id.as_deref() {
            let parent_exists = parent_id == updated.id
                || repository
                    .get(snapshot, &updated.data_source_id, parent_id)?
                    .is_some();
            if !parent_exists {
                actions.push(index_write(
                    orphan_key(updated)?,
                    record_key(&updated.data_source_id, &updated.id)?,
                ));
            }
        }
    }
    actions.push(WriteAction::Delete {
        keyspace: KeyspaceName::Tombstones,
        key: tombstone_key(&updated.data_source_id, &updated.id)?,
    });
    for action in actions {
        stream_action(writer, action);
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
