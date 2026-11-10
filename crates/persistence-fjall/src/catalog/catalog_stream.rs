use super::{
    decode_record, orphan_key, record_key, tombstone_key, CatalogEntryType, CatalogRecordStore,
    FileEntryRecord,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, SourceReadSnapshot, WriteAction, WriteStream},
    keyspaces::KeyspaceName,
    record_metrics, FjallStorageError, Result, ScanDirection,
};

#[derive(Debug, thiserror::Error)]
pub enum CatalogStreamError<E> {
    #[error(transparent)]
    Storage(#[from] FjallStorageError),
    #[error("catalog stream producer failed: {0}")]
    Producer(E),
}

impl CatalogRecordStore {
    pub fn replace_subtree_streamed<E, F>(
        &self,
        source_id: &str,
        root: &FileEntryRecord,
        partition_index: u32,
        mode: PersistMode,
        mut next_batch: F,
    ) -> std::result::Result<u64, CatalogStreamError<E>>
    where
        E: std::error::Error + 'static,
        F: FnMut() -> std::result::Result<Option<Vec<FileEntryRecord>>, E>,
    {
        validate_record(root)?;
        if root.data_source_id != source_id || root.parent_id.is_some() {
            return Err(FjallStorageError::InvalidEnvelope(
                "catalog subtree root identity is invalid".into(),
            )
            .into());
        }
        let ((inserted, _), _) =
            self.source_store
                .database()
                .write_streamed(mode, |snapshot, writer| {
                    let max_nodes = usize::try_from(self.count(snapshot, Some(source_id))?)
                        .map_err(|_| {
                            FjallStorageError::InvalidEnvelope(
                                "catalog count exceeds platform range".into(),
                            )
                        })?;
                    let mut metrics = record_metrics::CountDeltas::new();
                    let mut visited = 0usize;
                    self.delete_children(
                        snapshot,
                        writer,
                        source_id,
                        &root.id,
                        0,
                        max_nodes,
                        &mut visited,
                        &mut metrics,
                    )
                    .map_err(CatalogStreamError::Storage)?;
                    let mut root = root.clone();
                    root.partition_index = Some(partition_index);
                    self.stream_upsert(writer, &root, &mut metrics)
                        .map_err(CatalogStreamError::Storage)?;

                    let mut inserted = 0u64;
                    while let Some(mut records) =
                        next_batch().map_err(CatalogStreamError::Producer)?
                    {
                        if records.is_empty() {
                            return Err(CatalogStreamError::Storage(
                                FjallStorageError::InvalidEnvelope(
                                    "catalog subtree replacement produced an empty batch".into(),
                                ),
                            ));
                        }
                        for record in &mut records {
                            if record.data_source_id != source_id {
                                return Err(CatalogStreamError::Storage(
                                    FjallStorageError::InvalidEnvelope(
                                        "catalog subtree replacement crossed source ownership"
                                            .into(),
                                    ),
                                ));
                            }
                            record.partition_index = Some(partition_index);
                            self.stream_upsert(writer, record, &mut metrics)
                                .map_err(CatalogStreamError::Storage)?;
                        }
                        inserted = inserted.saturating_add(records.len() as u64);
                    }
                    let mut actions = Vec::new();
                    record_metrics::append(snapshot, &mut actions, metrics)?;
                    record_metrics::advance(snapshot, &mut actions, "catalog")?;
                    for action in actions {
                        stream_action(writer, action);
                    }
                    Ok::<(u64, ()), CatalogStreamError<E>>((inserted, ()))
                })?;
        Ok(inserted)
    }

    #[allow(clippy::too_many_arguments)]
    fn delete_children(
        &self,
        snapshot: &SourceReadSnapshot,
        writer: &mut WriteStream<'_>,
        source_id: &str,
        parent_id: &str,
        depth: usize,
        max_nodes: usize,
        visited: &mut usize,
        metrics: &mut record_metrics::CountDeltas,
    ) -> Result<()> {
        if depth > max_nodes {
            return Err(FjallStorageError::InvalidEnvelope(
                "catalog subtree contains a parent cycle".into(),
            ));
        }
        let mut cursor = None;
        loop {
            let page = self.list_by_parent(
                snapshot,
                source_id,
                Some(parent_id),
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )?;
            for child in page.records {
                *visited = visited.saturating_add(1);
                if *visited > max_nodes {
                    return Err(FjallStorageError::InvalidEnvelope(
                        "catalog subtree exceeds catalog cardinality".into(),
                    ));
                }
                self.delete_children(
                    snapshot,
                    writer,
                    source_id,
                    &child.id,
                    depth.saturating_add(1),
                    max_nodes,
                    visited,
                    metrics,
                )?;
                self.stream_delete(writer, &child, metrics)?;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(());
            }
        }
    }

    fn stream_upsert(
        &self,
        writer: &mut WriteStream<'_>,
        record: &FileEntryRecord,
        metrics: &mut record_metrics::CountDeltas,
    ) -> Result<()> {
        validate_record(record)?;
        let key = record_key(&record.data_source_id, &record.id)?;
        let previous = writer
            .get(KeyspaceName::CatalogRecords, &key)?
            .map(|bytes| decode_record(VersionedEnvelope::decode(&bytes)?))
            .transpose()?;
        if previous.as_ref().is_some_and(|previous| previous == record) {
            return Ok(());
        }
        if let Some(previous) = previous.as_ref() {
            self.delete_record_actions(writer, previous, metrics)?;
        }
        record_metrics::adjust(metrics, "catalog", &record.data_source_id, 1)?;
        if record.entry_type == CatalogEntryType::File {
            record_metrics::adjust(metrics, "catalog.files", &record.data_source_id, 1)?;
        }
        let envelope = VersionedEnvelope::from_serializable(
            super::RECORD_KIND,
            super::SCHEMA_VERSION,
            &record.id,
            &record.data_source_id,
            1,
            record,
        )?;
        writer.put(KeyspaceName::CatalogRecords, key, envelope.encode()?);
        let mut actions = Vec::new();
        self.index_writes(&mut actions, record)?;
        if record.entry_type == CatalogEntryType::Directory {
            if let Some(parent_id) = record.parent_id.as_deref() {
                let parent_exists = parent_id == record.id
                    || writer
                        .get(
                            KeyspaceName::CatalogRecords,
                            &record_key(&record.data_source_id, parent_id)?,
                        )?
                        .is_some();
                if !parent_exists {
                    actions.push(super::index_write(
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
        for action in actions {
            stream_action(writer, action);
        }
        unindex_orphan(writer, record)?;
        Ok(())
    }

    fn stream_delete(
        &self,
        writer: &mut WriteStream<'_>,
        record: &FileEntryRecord,
        metrics: &mut record_metrics::CountDeltas,
    ) -> Result<()> {
        let key = record_key(&record.data_source_id, &record.id)?;
        let Some(bytes) = writer.get(KeyspaceName::CatalogRecords, &key)? else {
            return Ok(());
        };
        let current = decode_record(VersionedEnvelope::decode(&bytes)?)?;
        self.delete_record_actions(writer, &current, metrics)
    }

    fn delete_record_actions(
        &self,
        writer: &mut WriteStream<'_>,
        record: &FileEntryRecord,
        metrics: &mut record_metrics::CountDeltas,
    ) -> Result<()> {
        writer.delete(
            KeyspaceName::CatalogRecords,
            record_key(&record.data_source_id, &record.id)?,
        );
        let mut actions = Vec::new();
        self.index_deletes(&mut actions, record)?;
        for action in actions {
            stream_action(writer, action);
        }
        unindex_orphan(writer, record)?;
        record_metrics::adjust(metrics, "catalog", &record.data_source_id, -1)?;
        if record.entry_type == CatalogEntryType::File {
            record_metrics::adjust(metrics, "catalog.files", &record.data_source_id, -1)?;
        }
        Ok(())
    }
}

fn unindex_orphan(writer: &mut WriteStream<'_>, record: &FileEntryRecord) -> Result<()> {
    if record.entry_type == CatalogEntryType::Directory {
        writer.delete(KeyspaceName::CatalogIndexes, orphan_key(record)?);
    }
    Ok(())
}

fn validate_record(record: &FileEntryRecord) -> Result<()> {
    if record.id.is_empty() || record.data_source_id.is_empty() || record.name.is_empty() {
        return Err(FjallStorageError::InvalidEnvelope(
            "catalog identity fields are empty".into(),
        ));
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
