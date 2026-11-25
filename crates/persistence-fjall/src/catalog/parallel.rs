use super::{
    orphan_key, record_key, tombstone_key, CatalogEntryType, CatalogRecordStore, FileEntryRecord,
};
use crate::{
    record_metrics, FjallStorageError, KeyspaceName, NativeWriteReport, PersistMode, Result,
    ScanDirection, SourceReadSnapshot, VersionedEnvelope, WriteAction, WriteSessionConfig,
};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::atomic::AtomicBool,
};

impl CatalogRecordStore {
    pub fn upsert_parallel(
        &self,
        source: &str,
        cancel: &AtomicBool,
        producer: impl FnMut() -> Result<Option<Vec<FileEntryRecord>>>,
    ) -> Result<NativeWriteReport> {
        self.source_store.database().write_batch(
            &[WriteAction::Delete {
                keyspace: KeyspaceName::Meta,
                key: super::browse_counts::marker(source)?,
            }],
            PersistMode::Buffer,
        )?;
        let session = self
            .source_store
            .database()
            .begin_write_session(WriteSessionConfig::default())?;
        session.run(
            cancel,
            producer,
            |record| {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                record.id.hash(&mut hash);
                hash.finish()
            },
            |handle, records| {
                let snapshot = handle.read_snapshot();
                self.parallel_actions(&snapshot, source, records)
            },
        )?;
        let report = session.finish()?;
        self.rebuild_parallel_metrics(source, cancel)?;
        Ok(report)
    }

    fn parallel_actions(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
        records: Vec<FileEntryRecord>,
    ) -> Result<Vec<WriteAction>> {
        let records = records
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect::<BTreeMap<_, _>>();
        let mut actions = Vec::new();
        for (id, record) in records {
            if record.data_source_id != source || id.is_empty() || record.name.is_empty() {
                return Err(FjallStorageError::CatalogRecordIdentityMismatch);
            }
            if let Some(previous) = self.get(snapshot, source, &id)? {
                if previous.partition_index != record.partition_index {
                    return Err(FjallStorageError::CatalogRecordIdentityMismatch);
                }
                if previous == record {
                    continue;
                }
                self.index_deletes(&mut actions, &previous)?;
            }
            actions.push(WriteAction::Put {
                keyspace: KeyspaceName::CatalogRecords,
                key: record_key(source, &id)?,
                value: VersionedEnvelope::from_serializable(
                    super::RECORD_KIND,
                    super::SCHEMA_VERSION,
                    &id,
                    source,
                    1,
                    &record,
                )?
                .encode()?,
            });
            Self::index_writes(&mut actions, &record)?;
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::Tombstones,
                key: tombstone_key(source, &id)?,
            });
        }
        Ok(actions)
    }

    fn rebuild_parallel_metrics(&self, source: &str, cancel: &AtomicBool) -> Result<()> {
        self.clear_browse_counts(source, cancel)?;
        let snapshot = self.read_snapshot();
        let mut cursor = None;
        let mut count = 0u64;
        let mut files = 0u64;
        loop {
            crate::source_store::ensure_not_cancelled(cancel)?;
            let page = self.list_all(
                &snapshot,
                source,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )?;
            let mut actions = Vec::new();
            let mut browse_deltas = super::browse_counts::Deltas::new();
            for record in page.records {
                super::browse_counts::adjust(&mut browse_deltas, &record, 1)?;
                count += 1;
                if record.entry_type == CatalogEntryType::File {
                    files += 1;
                }
                if record.entry_type == CatalogEntryType::Directory {
                    let missing = match record.parent_id.as_deref() {
                        Some(parent) => {
                            parent == record.id || self.get(&snapshot, source, parent)?.is_none()
                        }
                        None => false,
                    };
                    let key = orphan_key(&record)?;
                    actions.push(if missing {
                        WriteAction::Put {
                            keyspace: KeyspaceName::CatalogIndexes,
                            key,
                            value: record_key(source, &record.id)?,
                        }
                    } else {
                        WriteAction::Delete {
                            keyspace: KeyspaceName::CatalogIndexes,
                            key,
                        }
                    });
                }
            }
            self.source_store
                .database()
                .write_transaction(PersistMode::Buffer, |latest| {
                    super::browse_counts::append(latest, &mut actions, browse_deltas)?;
                    Ok(actions)
                })?;
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        drop(snapshot);
        self.source_store
            .database()
            .write_transaction(PersistMode::SyncAll, |snapshot| {
                let mut actions = Vec::new();
                for (domain, next) in [("catalog", count), ("catalog.files", files)] {
                    let old = record_metrics::count(snapshot, domain, Some(source))?;
                    let total = record_metrics::count(snapshot, domain, None)?
                        .checked_sub(old)
                        .and_then(|total| total.checked_add(next))
                        .ok_or_else(|| {
                            FjallStorageError::InvalidEnvelope("catalog count overflow".into())
                        })?;
                    record_metrics::set(&mut actions, domain, None, total)?;
                    record_metrics::set(&mut actions, domain, Some(source), next)?;
                }
                record_metrics::advance(snapshot, &mut actions, "catalog")?;
                super::browse_counts::mark(&mut actions, source)?;
                Ok(actions)
            })?;
        Ok(())
    }

    fn clear_browse_counts(&self, source: &str, cancel: &AtomicBool) -> Result<()> {
        let prefix = super::browse_counts::source_prefix(source)?;
        loop {
            crate::source_store::ensure_not_cancelled(cancel)?;
            let snapshot = self.read_snapshot();
            let entries = snapshot.keyset(
                KeyspaceName::Meta,
                &prefix,
                None,
                ScanDirection::Forward,
                512,
            )?;
            if entries.is_empty() {
                return Ok(());
            }
            let actions = entries
                .into_iter()
                .map(|entry| WriteAction::Delete {
                    keyspace: KeyspaceName::Meta,
                    key: entry.key,
                })
                .collect::<Vec<_>>();
            self.source_store
                .database()
                .write_batch(&actions, PersistMode::Buffer)?;
        }
    }
}

pub(crate) fn encoded_record_bytes(record: &FileEntryRecord) -> Result<usize> {
    let mut actions = Vec::new();
    CatalogRecordStore::index_writes(&mut actions, record)?;
    let envelope = VersionedEnvelope::from_serializable(
        super::RECORD_KIND,
        super::SCHEMA_VERSION,
        &record.id,
        &record.data_source_id,
        1,
        record,
    )?;
    let mut bytes = envelope.encode()?.len()
        + record_key(&record.data_source_id, &record.id)?.len()
        + tombstone_key(&record.data_source_id, &record.id)?.len();
    for action in actions {
        match action {
            WriteAction::Put { key, value, .. } => bytes += key.len() + value.len(),
            WriteAction::Delete { key, .. } => bytes += key.len(),
        }
    }
    Ok(bytes)
}
