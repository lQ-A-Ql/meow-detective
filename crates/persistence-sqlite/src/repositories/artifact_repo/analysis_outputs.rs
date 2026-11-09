use super::{mapping::row_to_artifact, visiting, ArtifactRepo};
use crate::{
    connection::DbResult,
    repositories::source_meta_repo::{SourceMetaRepo, ARTIFACT_CURSOR_REVISION_KEY},
};
use domain::Artifact;
use rusqlite::params;

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
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            return Err(crate::DbError::System(
                "source detail storage is missing".into(),
            ));
        }
        let deleted = self.conn.execute(
            "DELETE FROM artifacts
             WHERE source_object_id = ?1 AND extractor_id LIKE ?2",
            params![source_object_id, format!("{producer_prefix}%")],
        )?;
        if deleted > 0 {
            SourceMetaRepo::new(self.conn).bump_revision(ARTIFACT_CURSOR_REVISION_KEY)?;
        }
        Ok(deleted)
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
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            return Err(crate::DbError::System(
                "source detail storage is missing".into(),
            ));
        }
        let deleted = self.conn.execute(
            "DELETE FROM artifacts
             WHERE source_object_id = ?1 AND extractor_id = ?2",
            params![source_object_id, extractor_id],
        )?;
        if deleted > 0 {
            SourceMetaRepo::new(self.conn).bump_revision(ARTIFACT_CURSOR_REVISION_KEY)?;
        }
        Ok(deleted)
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
                    artifacts.push(artifact);
                }
                Ok(())
            })?;
            return Ok(artifacts);
        }
        let mut statement = self.conn.prepare(
            "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version,
                    confidence, source_attribution, title, summary, attrs, created_at
             FROM artifacts
             WHERE source_object_id = ?1
               AND extractor_id LIKE ?2
             ORDER BY rowid ASC",
        )?;
        let rows = statement.query_map(
            params![source_object_id, format!("{producer_prefix}%")],
            row_to_artifact,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*)
             FROM artifacts
             WHERE source_object_id = ?1
               AND extractor_id LIKE ?2",
            params![source_object_id, format!("{producer_prefix}%")],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn list_analysis_outputs_for_prefixes(
        &self,
        producer_prefixes: &[&str],
    ) -> DbResult<Vec<Artifact>> {
        if producer_prefixes.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(records) = visiting::records(self.conn)? {
            return records
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
                .collect();
        }
        let predicates = (1..=producer_prefixes.len())
            .map(|index| format!("extractor_id LIKE ?{index}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let sql = format!(
            "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version,
                    confidence, source_attribution, title, summary, attrs, created_at
             FROM artifacts
             WHERE source_object_id IS NOT NULL
               AND extractor_id IS NOT NULL
               AND ({predicates})
             ORDER BY rowid ASC"
        );
        let patterns = producer_prefixes
            .iter()
            .map(|prefix| format!("{prefix}%"))
            .collect::<Vec<_>>();
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(patterns), row_to_artifact)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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
                |artifact| {
                    artifacts.push(artifact);
                    Ok(())
                },
            )?;
            return Ok(artifacts);
        }
        let placeholders = (1..=source_object_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix_parameter = source_object_ids.len() + 1;
        let sql = format!(
            "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version,
                    confidence, source_attribution, title, summary, attrs, created_at
             FROM artifacts
             WHERE source_object_id IN ({placeholders})
               AND extractor_id LIKE ?{prefix_parameter}
             ORDER BY source_object_id ASC, rowid ASC"
        );
        let mut parameters = source_object_ids
            .iter()
            .map(|source_id| (*source_id).to_string())
            .collect::<Vec<_>>();
        parameters.push(format!("{producer_prefix}%"));
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(parameters), row_to_artifact)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
