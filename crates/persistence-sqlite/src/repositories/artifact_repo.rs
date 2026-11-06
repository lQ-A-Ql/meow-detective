use crate::connection::DbResult;
use crate::repositories::source_meta_repo::{SourceMetaRepo, ARTIFACT_CURSOR_REVISION_KEY};
use crate::sql_builder::ClauseBuilder;
use domain::Artifact;
use rusqlite::{params, Connection};
mod carving;
mod mapping;
mod persistence;
mod visiting;
use mapping::{row_to_artifact, row_to_artifact_cursor_row};
pub(crate) use visiting::decode as decode_fjall_artifact;
use visiting::records as fjall_records;

const ARTIFACT_SELECT_COLUMNS: &str = "id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSortKey {
    pub created_at: String,
    pub artifact_id: String,
}

#[derive(Debug, Clone)]
pub struct ArtifactCursorRow {
    pub artifact: Artifact,
    pub sort_key: ArtifactSortKey,
}

pub struct ArtifactRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ArtifactRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn delete_analysis_outputs_in_transaction(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<usize> {
        if let Some(records) = fjall_records(self.conn)? {
            let store = crate::source_fjall::open_store_for_write(self.conn)?
                .expect("Fjall source store is configured");
            let ids = records
                .into_iter()
                .filter(|record| {
                    record.source_object_id.as_deref() == Some(source_object_id)
                        && record
                            .attrs
                            .get("extractorId")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|id| id.starts_with(producer_prefix))
                })
                .map(|record| record.id)
                .collect::<Vec<_>>();
            return persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone())
                .delete_batch(
                    &crate::source_fjall::source_id(self.conn)?
                        .expect("source identity is configured"),
                    &ids,
                    persistence_fjall::PersistMode::SyncAll,
                )
                .map_err(|error| crate::connection::DbError::System(error.to_string()));
        }
        let deleted = self.conn.execute(
            "DELETE FROM artifacts
                 WHERE source_object_id = ?1
                   AND extractor_id LIKE ?2",
            params![source_object_id, format!("{producer_prefix}%")],
        )?;
        if deleted > 0 {
            SourceMetaRepo::new(self.conn).bump_revision(ARTIFACT_CURSOR_REVISION_KEY)?;
        }
        Ok(deleted)
    }

    /// Delete one source's outputs produced by one exact extractor id. Plugin
    /// artifacts carry the bare plugin id (no shared prefix), so plugin
    /// re-extraction replaces outputs by exact match instead of `LIKE`.
    pub fn delete_analysis_outputs_by_extractor_in_transaction(
        &self,
        source_object_id: &str,
        extractor_id: &str,
    ) -> DbResult<usize> {
        if let Some(records) = fjall_records(self.conn)? {
            let store = crate::source_fjall::open_store_for_write(self.conn)?
                .expect("Fjall source store is configured");
            let ids = records
                .into_iter()
                .filter(|record| {
                    record.source_object_id.as_deref() == Some(source_object_id)
                        && record
                            .attrs
                            .get("extractorId")
                            .and_then(serde_json::Value::as_str)
                            == Some(extractor_id)
                })
                .map(|record| record.id)
                .collect::<Vec<_>>();
            return persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone())
                .delete_batch(
                    &crate::source_fjall::source_id(self.conn)?
                        .expect("source identity is configured"),
                    &ids,
                    persistence_fjall::PersistMode::SyncAll,
                )
                .map_err(|error| crate::connection::DbError::System(error.to_string()));
        }
        let deleted = self.conn.execute(
            "DELETE FROM artifacts
                 WHERE source_object_id = ?1
                   AND extractor_id = ?2",
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
        if let Some(records) = fjall_records(self.conn)? {
            return records
                .into_iter()
                .filter(|record| {
                    record.source_object_id.as_deref() == Some(source_object_id)
                        && record
                            .attrs
                            .get("extractorId")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|id| id.starts_with(producer_prefix))
                })
                .map(visiting::decode)
                .collect::<DbResult<Vec<_>>>();
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
        if let Some(records) = fjall_records(self.conn)? {
            return Ok(records
                .iter()
                .filter(|record| {
                    record.source_object_id.as_deref() == Some(source_object_id)
                        && record
                            .attrs
                            .get("extractorId")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|id| id.starts_with(producer_prefix))
                })
                .count() as u64);
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

    pub fn list_by_family(&self, family: Option<&str>) -> DbResult<Vec<Artifact>> {
        if let Some(records) = fjall_records(self.conn)? {
            return records
                .into_iter()
                .filter(|record| family.is_none_or(|value| value == record.artifact_type))
                .map(visiting::decode)
                .collect::<DbResult<Vec<_>>>();
        }
        let mut stmt = match family {
            Some(_) => self.conn.prepare(
                "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at
                 FROM artifacts WHERE artifact_type = ?1 ORDER BY created_at DESC, id ASC",
            )?,
            None => self.conn.prepare(
                "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at
                 FROM artifacts ORDER BY created_at DESC, id ASC",
            )?,
        };
        let rows = match family {
            Some(f) => stmt.query_map(params![f], row_to_artifact)?,
            None => stmt.query_map([], row_to_artifact)?,
        };
        let mut artifacts = Vec::new();
        for row in rows {
            artifacts.push(row?);
        }
        Ok(artifacts)
    }

    pub fn list_by_family_after_at_snapshot(
        &self,
        family: Option<&str>,
        after: Option<&ArtifactSortKey>,
        snapshot_high_water: i64,
        limit: u32,
    ) -> DbResult<Vec<ArtifactCursorRow>> {
        self.list_by_family_after_with_snapshot(family, after, Some(snapshot_high_water), limit)
    }

    fn list_by_family_after_with_snapshot(
        &self,
        family: Option<&str>,
        after: Option<&ArtifactSortKey>,
        snapshot_high_water: Option<i64>,
        limit: u32,
    ) -> DbResult<Vec<ArtifactCursorRow>> {
        let mut builder = ClauseBuilder::new();
        if let Some(family) = family {
            builder.push_eq("artifact_type", family.to_string());
        }
        if let Some(high_water) = snapshot_high_water {
            builder.push_cmp("rowid", "<=", high_water);
        }
        if let Some(after) = after {
            let created_at_param = builder.next_param();
            let artifact_id_param = created_at_param + 1;
            builder.push_raw(
                format!(
                    "(created_at < ?{created_at_param} OR (created_at = ?{created_at_param} AND id > ?{artifact_id_param}))"
                ),
                vec![after.created_at.clone(), after.artifact_id.clone()],
            );
        }
        let limit_param = builder.push_param(limit);
        let sql = format!(
            "SELECT {ARTIFACT_SELECT_COLUMNS} FROM artifacts {} \
             ORDER BY created_at DESC, id ASC LIMIT ?{limit_param}",
            builder.where_clause()
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows =
            statement.query_map(builder.param_refs().as_slice(), row_to_artifact_cursor_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn snapshot_high_water(&self) -> DbResult<i64> {
        if let Some(records) = fjall_records(self.conn)? {
            return i64::try_from(records.len()).map_err(|_| {
                crate::connection::DbError::System(
                    "artifact count exceeds SQLite integer range".into(),
                )
            });
        }
        self.conn
            .query_row("SELECT COALESCE(MAX(rowid), 0) FROM artifacts", [], |row| {
                row.get(0)
            })
            .map_err(Into::into)
    }

    pub fn cursor_revision(&self) -> DbResult<u64> {
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let repository =
                persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
            return repository
                .cursor_revision(&repository.read_snapshot())
                .map_err(|error| crate::DbError::System(error.to_string()));
        }
        SourceMetaRepo::new(self.conn).read_revision(ARTIFACT_CURSOR_REVISION_KEY)
    }

    pub fn count_for_family_at_snapshot(
        &self,
        family: Option<&str>,
        snapshot_high_water: i64,
    ) -> DbResult<u64> {
        self.count_for_family_with_snapshot(family, Some(snapshot_high_water))
    }

    fn count_for_family_with_snapshot(
        &self,
        family: Option<&str>,
        snapshot_high_water: Option<i64>,
    ) -> DbResult<u64> {
        let mut builder = ClauseBuilder::new();
        if let Some(family) = family {
            builder.push_eq("artifact_type", family.to_string());
        }
        if let Some(high_water) = snapshot_high_water {
            builder.push_cmp("rowid", "<=", high_water);
        }
        let sql = format!("SELECT COUNT(*) FROM artifacts {}", builder.where_clause());
        let mut statement = self.conn.prepare(&sql)?;
        let count: i64 = statement.query_row(builder.param_refs().as_slice(), |row| row.get(0))?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    pub fn families(&self) -> DbResult<Vec<String>> {
        if let Some(records) = fjall_records(self.conn)? {
            let mut families = records
                .into_iter()
                .map(|record| record.artifact_type)
                .collect::<Vec<_>>();
            families.sort();
            families.dedup();
            return Ok(families);
        }
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT artifact_type FROM artifacts ORDER BY artifact_type")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut families = Vec::new();
        for row in rows {
            families.push(row?);
        }
        Ok(families)
    }

    pub fn count(&self) -> DbResult<u64> {
        if let Some(records) = fjall_records(self.conn)? {
            return Ok(records.len() as u64);
        }
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn count_by_family(&self) -> DbResult<Vec<(String, u64)>> {
        if let Some(records) = fjall_records(self.conn)? {
            let mut counts = std::collections::BTreeMap::<String, u64>::new();
            for record in records {
                *counts.entry(record.artifact_type).or_default() += 1;
            }
            return Ok(counts.into_iter().collect());
        }
        let mut stmt = self.conn.prepare(
            "SELECT artifact_type, COUNT(*) FROM artifacts GROUP BY artifact_type ORDER BY artifact_type",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        let mut counts = Vec::new();
        for row in rows {
            counts.push(row?);
        }
        Ok(counts)
    }

    pub fn find_by_id(&self, artifact_id: &str) -> DbResult<Option<Artifact>> {
        if let Some(records) = fjall_records(self.conn)? {
            return records
                .into_iter()
                .find(|record| record.id == artifact_id)
                .map(visiting::decode)
                .transpose();
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at
             FROM artifacts WHERE id = ?1",
        )?;
        let result = stmt.query_row(params![artifact_id], row_to_artifact);
        match result {
            Ok(artifact) => Ok(Some(artifact)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/artifact_repo.rs"]
mod tests;
