use super::{row_to_artifact, ArtifactRepo, ARTIFACT_SELECT_COLUMNS};
use crate::DbResult;
use domain::Artifact;
impl ArtifactRepo<'_> {
    /// Bounded metadata search, separate from FileEntry search documents.
    pub fn carving_page(&self, source: &str, after: &str, query: &str) -> DbResult<Vec<Artifact>> {
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        let sql=format!("SELECT {ARTIFACT_SELECT_COLUMNS} FROM artifacts WHERE data_source_id=?1 AND artifact_type IN ('carving.png','carving.zip') AND id>?2
            AND (title LIKE ?3 ESCAPE '\\' OR attrs LIKE ?3 ESCAPE '\\') ORDER BY id LIMIT 101");
        let mut statement = self.conn.prepare(&sql)?;
        let rows =
            statement.query_map(rusqlite::params![source, after, pattern], row_to_artifact)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
