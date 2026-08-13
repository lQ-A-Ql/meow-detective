//! Exact-producer analysis-output deletion for plugin re-extraction.
//!
//! Split out of `timeline_repo.rs` to keep that module inside the size
//! budget. Plugin timeline events carry the bare plugin id as `parser_id`
//! (M2 provenance contract), so the shared `LIKE` prefix replacement cannot
//! address them; plugin candidates replace outputs by exact id.

use super::{TimelineRepo, TIMELINE_CURSOR_REVISION_KEY};
use crate::connection::DbResult;
use crate::repositories::source_meta_repo::SourceMetaRepo;
use rusqlite::params;

impl TimelineRepo<'_> {
    /// Delete one source's timeline events produced by one exact parser id.
    pub fn delete_analysis_outputs_by_parser_in_transaction(
        &self,
        source_object_id: &str,
        parser_id: &str,
    ) -> DbResult<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM timeline_events
                 WHERE source_object_id = ?1
                   AND parser_id = ?2",
            params![source_object_id, parser_id],
        )?;
        if deleted > 0 {
            SourceMetaRepo::new(self.conn).bump_revision(TIMELINE_CURSOR_REVISION_KEY)?;
        }
        Ok(deleted)
    }
}
