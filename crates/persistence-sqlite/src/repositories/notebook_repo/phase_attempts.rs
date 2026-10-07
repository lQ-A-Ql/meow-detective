use rusqlite::params;

use super::NotebookRepo;
use crate::connection::DbResult;

impl NotebookRepo<'_> {
    /// Count only this case, step kind, import set and phase's recorded attempts.
    pub fn count_import_phase_steps(
        &self,
        case_id: &str,
        step_kind: &str,
        import_set_id: &str,
        phase: &str,
    ) -> DbResult<u64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM investigation_steps
                 WHERE case_id = ?1 AND step_kind = ?2
                   AND json_extract(params_json, '$.importSetId') = ?3
                   AND json_extract(params_json, '$.phase') = ?4",
                params![case_id, step_kind, import_set_id, phase],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }
}
