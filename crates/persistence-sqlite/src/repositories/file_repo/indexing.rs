use super::{
    mapping::{collect_entries, row_to_file_entry},
    FileRepo, FILE_ENTRY_COLUMNS,
};
use crate::DbResult;
use domain::FileEntry;
use rusqlite::params;
impl FileRepo<'_> {
    pub fn index_page(&self, source: &str, after: &str, limit: u32) -> DbResult<Vec<FileEntry>> {
        let mut statement=self.conn.prepare(&format!("SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id=?1 AND id>?2 ORDER BY id LIMIT ?3"))?;
        let rows =
            statement.query_map(params![source, after, limit.min(1000)], row_to_file_entry)?;
        collect_entries(rows)
    }
}
