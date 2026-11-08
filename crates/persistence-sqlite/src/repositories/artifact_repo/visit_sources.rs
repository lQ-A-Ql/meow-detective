use super::{mapping::row_to_artifact, visiting, ArtifactRepo, ARTIFACT_SELECT_COLUMNS};
use crate::DbResult;
use domain::Artifact;

impl ArtifactRepo<'_> {
    pub fn visit_analysis_outputs_for_sources<F>(
        &self,
        source_object_ids: &[&str],
        producer_prefix: &str,
        mut visit: F,
    ) -> DbResult<()>
    where
        F: FnMut(Artifact) -> DbResult<()>,
    {
        if source_object_ids.is_empty() {
            return Ok(());
        }
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let store = crate::source_fjall::require_source_store(self.conn)?;
            let source_id = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
            let repository =
                persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
            let snapshot = repository.read_snapshot();
            let mut source_object_ids = source_object_ids.to_vec();
            source_object_ids.sort_unstable();
            source_object_ids.dedup();
            for source_object_id in source_object_ids {
                let mut cursor = None;
                loop {
                    crate::read_operation::ensure_active()?;
                    let page = repository
                        .list_by_source_object(
                            &snapshot,
                            &source_id,
                            source_object_id,
                            cursor.as_deref(),
                            persistence_fjall::ScanDirection::Forward,
                            256,
                        )
                        .map_err(|error| crate::DbError::System(error.to_string()))?;
                    for record in page.records {
                        let artifact = visiting::decode(record)?;
                        if artifact
                            .extractor_id
                            .as_deref()
                            .is_some_and(|id| id.starts_with(producer_prefix))
                        {
                            visit(artifact)?;
                        }
                    }
                    cursor = page.next_cursor;
                    if cursor.is_none() {
                        break;
                    }
                }
            }
            return Ok(());
        }
        let placeholders = (1..=source_object_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix_parameter = source_object_ids.len() + 1;
        let sql = format!(
            "SELECT {ARTIFACT_SELECT_COLUMNS} FROM artifacts
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
        for row in rows {
            visit(row?)?;
        }
        Ok(())
    }
}
