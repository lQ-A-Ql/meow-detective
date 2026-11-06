use super::{
    index_deletes, index_writes, record_key, validate, ArtifactRecord, ArtifactRecordStore,
    RECORD_KIND, SCHEMA_VERSION,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, WriteAction},
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

    fn upsert_actions(
        &self,
        snapshot: &crate::SourceReadSnapshot,
        records: &[ArtifactRecord],
    ) -> Result<Vec<WriteAction>> {
        let mut actions = Vec::with_capacity(records.len().saturating_mul(4));
        let mut deltas = crate::record_metrics::CountDeltas::new();
        let latest = records
            .iter()
            .map(|record| ((record.data_source_id.as_str(), record.id.as_str()), record))
            .collect::<std::collections::BTreeMap<_, _>>();
        for record in latest.into_values() {
            validate(record)?;
            let previous = self.get(snapshot, &record.data_source_id, &record.id)?;
            if previous.as_ref().is_some_and(|previous| previous == record) {
                continue;
            }
            if let Some(previous) = previous {
                actions.extend(index_deletes(&previous)?);
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
        }
        if actions.is_empty() {
            return Ok(actions);
        }
        crate::record_metrics::append(snapshot, &mut actions, deltas)?;
        actions.push(self.revision_action(snapshot)?);
        Ok(actions)
    }
}
