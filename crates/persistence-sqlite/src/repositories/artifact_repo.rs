use crate::connection::DbResult;
use crate::repositories::source_meta_repo::{SourceMetaRepo, ARTIFACT_CURSOR_REVISION_KEY};
use crate::sql_builder::ClauseBuilder;
use domain::Artifact;
use rusqlite::{params, Connection};
mod analysis_outputs;
mod carving;
mod mapping;
mod persistence;
mod visit_sources;
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
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let source_id = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
            let repository =
                persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
            let snapshot = repository.read_snapshot();
            let page = repository
                .list_by_created_order(
                    &snapshot,
                    &source_id,
                    family,
                    after.map(|key| (key.created_at.as_str(), key.artifact_id.as_str())),
                    limit as usize,
                )
                .map_err(|error| crate::DbError::System(error.to_string()))?;
            return page
                .records
                .into_iter()
                .map(|record| {
                    let artifact = visiting::decode(record)?;
                    let created_at = artifact.created_at.to_rfc3339();
                    let artifact_id = artifact.id.0.clone();
                    Ok(ArtifactCursorRow {
                        artifact,
                        sort_key: ArtifactSortKey {
                            created_at,
                            artifact_id,
                        },
                    })
                })
                .collect();
        }
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            return Err(crate::DbError::System(
                "source detail storage is missing".into(),
            ));
        }
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
        if crate::source_fjall::has_source_store(self.conn)? {
            return i64::try_from(self.cursor_revision()?).map_err(|_| {
                crate::connection::DbError::System("artifact revision exceeds cursor range".into())
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
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let source_id = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
            let repository =
                persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
            let snapshot = repository.read_snapshot();
            return family.map_or_else(
                || {
                    repository
                        .count(&snapshot, Some(&source_id))
                        .map_err(|error| crate::DbError::System(error.to_string()))
                },
                |family| {
                    repository
                        .count_by_type(&snapshot, &source_id, family)
                        .map_err(|error| crate::DbError::System(error.to_string()))
                },
            );
        }
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            return Err(crate::DbError::System(
                "source detail storage is missing".into(),
            ));
        }
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
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let repository =
                persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
            return repository
                .count(&repository.read_snapshot(), None)
                .map_err(|error| crate::DbError::System(error.to_string()));
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
