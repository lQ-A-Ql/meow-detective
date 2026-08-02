use crate::connection::DbResult;
use domain::{DataSourceId, FileEncryptionStatus, FileEntry, FileEntryId};
use rusqlite::params;

use super::{
    mapping::{
        collect_entries, escape_like_literal, file_encryption_status_from_row, row_to_file_entry,
    },
    FileRepo, FILE_ENTRY_COLUMNS,
};

impl FileRepo<'_> {
    pub fn find_by_path_prefix(
        &self,
        data_source_id: &DataSourceId,
        prefix: &str,
    ) -> DbResult<Vec<FileEntry>> {
        let pattern = format!("{}%", escape_like_literal(prefix));
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id = ?1 AND path LIKE ?2 ESCAPE '\\' ORDER BY path ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![data_source_id.0, pattern], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn count_by_data_source(&self, data_source_id: &DataSourceId) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1",
            params![data_source_id.0],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn count_all(&self) -> DbResult<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn find_by_id(&self, id: &FileEntryId) -> DbResult<Option<FileEntry>> {
        let sql = format!("SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE id = ?1");
        let mut statement = self.conn.prepare(&sql)?;
        match statement.query_row(params![id.0], row_to_file_entry) {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn find_encryption_status(
        &self,
        id: &FileEntryId,
    ) -> DbResult<Option<FileEncryptionStatus>> {
        let result = self.conn.query_row(
            "SELECT encrypted FROM file_entries WHERE id = ?1",
            params![id.0],
            |row| file_encryption_status_from_row(row, 0),
        );
        match result {
            Ok(status) => Ok(Some(status)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn find_partition_index_by_id(&self, id: &FileEntryId) -> DbResult<Option<usize>> {
        let result = self.conn.query_row(
            "SELECT partition_index FROM file_entries WHERE id = ?1",
            params![id.0],
            |row| row.get::<_, Option<i64>>(0),
        );
        match result {
            Ok(Some(index)) => usize::try_from(index).map(Some).map_err(|_| {
                crate::connection::DbError::System(format!(
                    "File entry '{}' has invalid partition index {}",
                    id.0, index
                ))
            }),
            Ok(None) | Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn set_partition_index_by_id(
        &self,
        id: &FileEntryId,
        partition_index: usize,
    ) -> DbResult<()> {
        let partition_index = i64::try_from(partition_index).map_err(|_| {
            crate::connection::DbError::System(format!(
                "Partition index is too large for SQLite: {partition_index}"
            ))
        })?;
        self.conn.execute(
            "UPDATE file_entries SET partition_index = ?1 WHERE id = ?2",
            params![partition_index, id.0],
        )?;
        Ok(())
    }

    pub fn assign_partition_index_to_subtree(
        &self,
        root_id: &FileEntryId,
        partition_index: usize,
    ) -> DbResult<usize> {
        let partition_index = i64::try_from(partition_index).map_err(|_| {
            crate::connection::DbError::System(format!(
                "Partition index is too large for SQLite: {partition_index}"
            ))
        })?;
        let updated = self.conn.execute(
            "WITH RECURSIVE subtree(id, data_source_id) AS (
                 SELECT id, data_source_id FROM file_entries WHERE id = ?1
                 UNION
                 SELECT child.id, child.data_source_id
                 FROM file_entries AS child
                 JOIN subtree AS parent
                   ON child.parent_id = parent.id
                  AND child.data_source_id = parent.data_source_id
             )
             UPDATE file_entries
             SET partition_index = ?2
             WHERE id IN (SELECT id FROM subtree)",
            params![root_id.0, partition_index],
        )?;
        Ok(updated)
    }

    pub fn find_data_source_location(
        &self,
        data_source_id: &DataSourceId,
    ) -> DbResult<Option<(String, String)>> {
        let result = self.conn.query_row(
            "SELECT kind, source_path FROM data_sources WHERE id = ?1",
            params![data_source_id.0],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok(location) => Ok(Some(location)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}
