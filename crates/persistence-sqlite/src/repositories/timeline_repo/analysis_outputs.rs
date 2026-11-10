//! Exact-producer analysis-output deletion for plugin re-extraction.
//!
//! Split out of `timeline_repo.rs` to keep that module inside the size
//! budget. Plugin timeline events carry the bare plugin id as `parser_id`
//! (M2 provenance contract), so the shared `LIKE` prefix replacement cannot
//! address them; plugin candidates replace outputs by exact id.

use super::TimelineRepo;
use crate::connection::DbResult;

impl TimelineRepo<'_> {
    /// Delete one source's timeline events produced by one exact parser id.
    pub fn delete_analysis_outputs_by_parser_in_transaction(
        &self,
        source_object_id: &str,
        parser_id: &str,
    ) -> DbResult<usize> {
        if let Some(store) = crate::source_fjall::open_store_for_write(self.conn)? {
            let source_id = crate::source_fjall::source_id(self.conn)?.ok_or_else(|| {
                crate::connection::DbError::System("source identity is missing".into())
            })?;
            return persistence_fjall::TimelineRecordStore::from_source_store((*store).clone())
                .delete_by_source_object_where(
                    &source_id,
                    source_object_id,
                    persistence_fjall::PersistMode::SyncAll,
                    |event| event.parser_id.as_deref() == Some(parser_id),
                )
                .map_err(|error| crate::connection::DbError::System(error.to_string()));
        }
        Err(crate::connection::DbError::System(
            "source detail storage is missing".into(),
        ))
    }
}
