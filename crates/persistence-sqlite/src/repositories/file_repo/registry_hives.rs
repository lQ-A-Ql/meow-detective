//! Stable, bounded discovery of live registry candidates in one source catalog.
use super::{mapping::row_to_file_entry, FileRepo, FILE_ENTRY_COLUMNS};
use crate::DbResult;
use domain::{DataSourceId, FileEntry};
use rusqlite::params;

const NAMES: &str =
    "('system','software','sam','security','default','ntuser.dat','usrclass.dat','amcache.hve')";

impl FileRepo<'_> {
    /// Match exact source-relative sibling paths, including partition prefixes.
    /// A bounded result preserves duplicate-name ambiguity instead of selecting
    /// an arbitrary log when the catalog contains conflicting entries.
    pub fn registry_log_companions(
        &self,
        source: &DataSourceId,
        hive_path: &str,
    ) -> DbResult<Vec<FileEntry>> {
        let path = hive_path.replace('\\', "/");
        let sql=format!("SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id=?1 AND deleted=0 AND entry_type='file' AND replace(path,'\\','/')=?2 COLLATE NOCASE ORDER BY id LIMIT 2");
        let mut statement = self.conn.prepare(&sql)?;
        let mut entries = Vec::new();
        for suffix in ["LOG1", "LOG2"] {
            let rows = statement.query_map(
                params![source.0, format!("{path}.{suffix}")],
                row_to_file_entry,
            )?;
            entries.extend(rows.collect::<Result<Vec<_>, _>>()?);
        }
        Ok(entries)
    }
    pub fn registry_hive_candidates(
        &self,
        source: &DataSourceId,
        after: &str,
        limit: u32,
    ) -> DbResult<Vec<FileEntry>> {
        let sql = format!("SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id=?1 AND deleted=0 AND entry_type='file' AND lower(name) IN {NAMES} AND id>?2 ORDER BY id LIMIT ?3");
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![source.0, after, limit], row_to_file_entry)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn registry_hive_candidate_count(&self, source: &DataSourceId) -> DbResult<u64> {
        let sql = format!("SELECT COUNT(*) FROM file_entries WHERE data_source_id=?1 AND deleted=0 AND entry_type='file' AND lower(name) IN {NAMES}");
        Ok(self
            .conn
            .query_row(&sql, params![source.0], |row| row.get(0))?)
    }
}
