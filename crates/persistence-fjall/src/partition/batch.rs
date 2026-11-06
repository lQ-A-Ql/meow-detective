use super::{
    decode_record, index_deletes, index_writes, record_key, validate_record, PartitionRecord,
    PartitionRecordStore, RECORD_KIND, SCHEMA_VERSION,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, WriteAction},
    keyspaces::KeyspaceName,
    FjallStorageError, Result,
};
use std::collections::{BTreeMap, HashMap};

impl PartitionRecordStore {
    pub fn replace_by_source(
        &self,
        source_id: &str,
        records: &[PartitionRecord],
        mode: PersistMode,
    ) -> Result<()> {
        let mut replacement = BTreeMap::new();
        for record in records {
            validate_record(record)?;
            if record.data_source_id != source_id {
                return Err(FjallStorageError::InvalidEnvelope(
                    "partition replacement contains another source".to_owned(),
                ));
            }
            replacement.insert(record.id.clone(), record.clone());
        }

        let snapshot = self.read_snapshot();
        let current = self.current_partitions(&snapshot, source_id)?;

        let action_capacity = current
            .len()
            .saturating_add(replacement.len())
            .saturating_mul(4);
        let mut actions = Vec::with_capacity(action_capacity);
        for (id, previous) in &current {
            if replacement.contains_key(id) {
                continue;
            }
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::PartitionRecords,
                key: record_key(source_id, id)?,
            });
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::Meta,
                key: super::runtime_key(source_id, id)?,
            });
            index_deletes(&mut actions, previous)?;
            actions.push(WriteAction::Put {
                keyspace: KeyspaceName::Tombstones,
                key: super::tombstone_key(source_id, id)?,
                value: record_key(source_id, id)?,
            });
        }

        for (id, record) in replacement {
            let previous = current.get(&id);
            if previous.is_some_and(|previous| previous == &record) {
                continue;
            }
            if let Some(previous) = previous {
                index_deletes(&mut actions, previous)?;
            }
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::Meta,
                key: super::runtime_key(source_id, &id)?,
            });
            let envelope = VersionedEnvelope::from_serializable(
                RECORD_KIND,
                SCHEMA_VERSION,
                &record.id,
                &record.data_source_id,
                1,
                &record,
            )?;
            actions.push(WriteAction::Put {
                keyspace: KeyspaceName::PartitionRecords,
                key: record_key(source_id, &id)?,
                value: envelope.encode()?,
            });
            index_writes(&mut actions, &record)?;
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::Tombstones,
                key: super::tombstone_key(source_id, &id)?,
            });
        }

        if !actions.is_empty() {
            self.source_store.database().write_batch(&actions, mode)?;
        }
        Ok(())
    }

    fn current_partitions(
        &self,
        snapshot: &crate::SourceReadSnapshot,
        source: &str,
    ) -> Result<HashMap<String, PartitionRecord>> {
        let mut current = HashMap::new();
        let prefix =
            crate::encode_key(super::RECORD_FAMILY, &[crate::KeyPart::Text(source.into())])?;
        let mut cursor = None;
        loop {
            let entries = snapshot.keyset(
                KeyspaceName::PartitionRecords,
                &prefix,
                cursor.as_deref(),
                crate::ScanDirection::Forward,
                256,
            )?;
            let more = entries.len() == 256;
            for entry in &entries {
                let record = decode_record(VersionedEnvelope::decode(&entry.value)?, source, "")?;
                current.insert(record.id.clone(), record);
            }
            cursor = entries.last().map(|entry| entry.key.clone());
            if !more {
                return Ok(current);
            }
        }
    }
}
