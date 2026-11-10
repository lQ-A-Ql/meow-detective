use super::ArtifactRepo;
use crate::{DbError, DbResult};
use domain::Artifact;
use persistence_fjall::{ArtifactRecordStore, ScanDirection};
impl ArtifactRepo<'_> {
    /// Bounded metadata search, separate from FileEntry search documents.
    pub fn carving_page(&self, source: &str, after: &str, query: &str) -> DbResult<Vec<Artifact>> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let store = crate::source_fjall::require_source_store(self.conn)?;
            let repository = ArtifactRecordStore::from_source_store((*store).clone());
            let snapshot = repository.read_snapshot();
            let needle = query.to_lowercase();
            let mut cursor = None;
            let mut results = Vec::with_capacity(101);
            while results.len() <= 100 {
                let page = repository
                    .list_by_source(
                        &snapshot,
                        source,
                        cursor.as_deref(),
                        ScanDirection::Forward,
                        256,
                    )
                    .map_err(|error| DbError::System(error.to_string()))?;
                for record in page.records {
                    if !matches!(record.artifact_type.as_str(), "carving.png" | "carving.zip") {
                        continue;
                    }
                    if record.id.as_str() <= after {
                        continue;
                    }
                    let artifact = super::visiting::decode(record.clone())?;
                    let haystack = format!(
                        "{}\n{}",
                        artifact.title,
                        serde_json::to_string(&artifact.attrs)
                            .map_err(|error| DbError::System(error.to_string()))?
                    )
                    .to_lowercase();
                    if needle.is_empty() || haystack.contains(&needle) {
                        results.push(artifact);
                        if results.len() == 101 {
                            break;
                        }
                    }
                }
                let Some(next) = page.next_cursor else {
                    break;
                };
                cursor = Some(next);
            }
            return Ok(results);
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }
}
