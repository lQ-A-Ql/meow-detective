use super::{push_bounded, visiting, ArtifactRepo};
use crate::connection::DbResult;
use domain::Artifact;

impl ArtifactRepo<'_> {
    pub fn delete_analysis_outputs_in_transaction(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<usize> {
        if let Some(store) = crate::source_fjall::open_store_for_write(self.conn)? {
            let source_id = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
            return persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone())
                .delete_by_source_object_where(
                    &source_id,
                    source_object_id,
                    persistence_fjall::PersistMode::SyncAll,
                    |record| {
                        record
                            .attrs
                            .get("extractorId")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|id| id.starts_with(producer_prefix))
                    },
                )
                .map_err(|error| crate::DbError::System(error.to_string()));
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn delete_analysis_outputs_by_extractor_in_transaction(
        &self,
        source_object_id: &str,
        extractor_id: &str,
    ) -> DbResult<usize> {
        if let Some(store) = crate::source_fjall::open_store_for_write(self.conn)? {
            let source_id = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
            return persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone())
                .delete_by_source_object_where(
                    &source_id,
                    source_object_id,
                    persistence_fjall::PersistMode::SyncAll,
                    |record| {
                        record
                            .attrs
                            .get("extractorId")
                            .and_then(serde_json::Value::as_str)
                            == Some(extractor_id)
                    },
                )
                .map_err(|error| crate::DbError::System(error.to_string()));
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn list_analysis_outputs(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<Vec<Artifact>> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let mut artifacts = Vec::new();
            self.visit_by_source_object(source_object_id, |artifact| {
                if artifact
                    .extractor_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(producer_prefix))
                {
                    push_bounded(&mut artifacts, artifact)?;
                }
                Ok(())
            })?;
            return Ok(artifacts);
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn count_analysis_outputs(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<u64> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let mut count = 0u64;
            self.visit_by_source_object(source_object_id, |artifact| {
                if artifact
                    .extractor_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(producer_prefix))
                {
                    count = count.saturating_add(1);
                }
                Ok(())
            })?;
            return Ok(count);
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn list_analysis_outputs_for_prefixes(
        &self,
        producer_prefixes: &[&str],
    ) -> DbResult<Vec<Artifact>> {
        if producer_prefixes.is_empty() {
            return Ok(Vec::new());
        }
        let records = visiting::records(self.conn)?;
        records
            .into_iter()
            .filter(|record| {
                record
                    .attrs
                    .get("extractorId")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| {
                        producer_prefixes
                            .iter()
                            .any(|prefix| id.starts_with(prefix))
                    })
            })
            .map(visiting::decode)
            .collect()
    }

    pub fn list_analysis_outputs_for_sources(
        &self,
        source_object_ids: &[&str],
        producer_prefix: &str,
    ) -> DbResult<Vec<Artifact>> {
        if source_object_ids.is_empty() {
            return Ok(Vec::new());
        }
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let mut artifacts = Vec::new();
            self.visit_analysis_outputs_for_sources(
                source_object_ids,
                producer_prefix,
                |artifact| push_bounded(&mut artifacts, artifact),
            )?;
            return Ok(artifacts);
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }
}
