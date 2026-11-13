use super::{index_deletes, record_key, type_counts, ArtifactRecordStore};
use crate::{KeyspaceName, PersistMode, Result, VerifiedGenerationClone, WriteAction};
use std::collections::BTreeSet;

impl ArtifactRecordStore {
    /// Delete records from an unpublished verified clone without changing stale counters.
    /// Counts must be rebuilt after all independent writers finish and before sealing.
    pub fn delete_batch_in_generation(
        &self,
        generation: &VerifiedGenerationClone,
        source_id: &str,
        ids: &[String],
        mode: PersistMode,
    ) -> Result<usize> {
        generation.validate_unpublished_target(&self.source_store, source_id)?;
        let mut deleted = 0;
        self.source_store
            .database()
            .write_planned(mode, |snapshot| {
                let mut actions = Vec::new();
                for id in ids.iter().collect::<BTreeSet<_>>() {
                    let Some(record) = self.get(snapshot, source_id, id)? else {
                        continue;
                    };
                    actions.push(WriteAction::Delete {
                        keyspace: KeyspaceName::ArtifactRecords,
                        key: record_key(source_id, id)?,
                    });
                    actions.extend(index_deletes(&record)?);
                    deleted += 1;
                }
                if deleted > 0 {
                    actions.push(WriteAction::Delete {
                        keyspace: KeyspaceName::Meta,
                        key: type_counts::marker_key(source_id)?,
                    });
                }
                Ok(actions)
            })?;
        Ok(deleted)
    }
}
