use rusqlite::{params, OptionalExtension};

use super::LinuxImportSetRepo;
use crate::connection::DbResult;

impl LinuxImportSetRepo<'_> {
    /// Resolve a member binding only within its owning case and import set.
    pub fn find_member_source_id(
        &self,
        case_id: &str,
        import_set_id: &str,
        member_index: u32,
    ) -> DbResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT source.id FROM linux_import_set_members AS member
                 JOIN linux_import_sets AS import_set ON import_set.id = member.import_set_id
                 JOIN data_sources AS source ON source.id = member.data_source_id
                    AND source.case_id = import_set.case_id
                 WHERE import_set.case_id = ?1 AND member.import_set_id = ?2
                   AND member.member_index = ?3",
                params![case_id, import_set_id, member_index],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }
}
