use crate::connection::DbResult;
use domain::Artifact;
use rusqlite::Connection;
mod analysis_outputs;
mod carving;
mod persistence;
mod visit_sources;
mod visiting;
pub(crate) use visiting::decode as decode_fjall_artifact;

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

pub(crate) const MAX_ARTIFACT_QUERY_ROWS: usize = 100_000;
const MAX_ARTIFACT_PAGE_SIZE: u32 = 1_024;

fn push_bounded<T>(rows: &mut Vec<T>, row: T) -> DbResult<()> {
    if rows.len() >= MAX_ARTIFACT_QUERY_ROWS {
        return Err(crate::DbError::System(
            "artifact query exceeds the bounded row limit".into(),
        ));
    }
    rows.push(row);
    Ok(())
}

impl<'a> ArtifactRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list_by_family(&self, family: Option<&str>) -> DbResult<Vec<Artifact>> {
        if crate::source_fjall::source_id(self.conn)?.is_none() {
            return Err(crate::DbError::System("source identity is missing".into()));
        }
        let mut artifacts = Vec::new();
        self.visit_by_family(family, |artifact| push_bounded(&mut artifacts, artifact))?;
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

    pub fn list_by_family_after(
        &self,
        family: Option<&str>,
        after: Option<&ArtifactSortKey>,
        limit: u32,
    ) -> DbResult<Vec<ArtifactCursorRow>> {
        self.list_by_family_after_with_snapshot(family, after, None, limit)
    }

    pub fn count_for_family(&self, family: Option<&str>) -> DbResult<u64> {
        self.count_for_family_with_snapshot(family, None)
    }

    fn list_by_family_after_with_snapshot(
        &self,
        family: Option<&str>,
        after: Option<&ArtifactSortKey>,
        _snapshot_high_water: Option<i64>,
        limit: u32,
    ) -> DbResult<Vec<ArtifactCursorRow>> {
        if limit > MAX_ARTIFACT_PAGE_SIZE {
            return Err(crate::DbError::System(
                "artifact page exceeds the bounded page limit".into(),
            ));
        }
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
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn snapshot_high_water(&self) -> DbResult<i64> {
        i64::try_from(self.cursor_revision()?).map_err(|_| {
            crate::connection::DbError::System("artifact revision exceeds cursor range".into())
        })
    }

    pub fn cursor_revision(&self) -> DbResult<u64> {
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let repository =
                persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
            return repository
                .cursor_revision(&repository.read_snapshot())
                .map_err(|error| crate::DbError::System(error.to_string()));
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
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
        _snapshot_high_water: Option<i64>,
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
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn families(&self) -> DbResult<Vec<String>> {
        if crate::source_fjall::source_id(self.conn)?.is_none() {
            return Err(crate::DbError::System("source identity is missing".into()));
        }
        let mut families = std::collections::BTreeSet::new();
        self.visit_by_family(None, |artifact| {
            if families.len() >= MAX_ARTIFACT_QUERY_ROWS {
                return Err(crate::DbError::System(
                    "artifact family query exceeds the bounded row limit".into(),
                ));
            }
            families.insert(artifact.family);
            Ok(())
        })?;
        Ok(families.into_iter().collect())
    }

    pub fn count(&self) -> DbResult<u64> {
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let repository =
                persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
            return repository
                .count(&repository.read_snapshot(), None)
                .map_err(|error| crate::DbError::System(error.to_string()));
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn count_by_family(&self) -> DbResult<Vec<(String, u64)>> {
        if crate::source_fjall::source_id(self.conn)?.is_none() {
            return Err(crate::DbError::System("source identity is missing".into()));
        }
        let mut counts = std::collections::BTreeMap::<String, u64>::new();
        self.visit_by_family(None, |artifact| {
            if !counts.contains_key(&artifact.family) && counts.len() >= MAX_ARTIFACT_QUERY_ROWS {
                return Err(crate::DbError::System(
                    "artifact family query exceeds the bounded row limit".into(),
                ));
            }
            *counts.entry(artifact.family).or_default() += 1;
            Ok(())
        })?;
        Ok(counts.into_iter().collect())
    }

    pub fn find_by_id(&self, artifact_id: &str) -> DbResult<Option<Artifact>> {
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let source_id = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
            let repository =
                persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
            return repository
                .get(&repository.read_snapshot(), &source_id, artifact_id)
                .map_err(|error| crate::DbError::System(error.to_string()))?
                .map(visiting::decode)
                .transpose();
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/artifact_repo.rs"]
mod tests;
