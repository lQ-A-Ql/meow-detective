use crate::connection::DbResult;
use domain::{DataSourceId, FileEncryptionStatus, FileEntry, FileEntryId};
use rusqlite::params;

use super::{push_bounded, FileRepo};

impl FileRepo<'_> {
    pub fn find_ancestor_chain(
        &self,
        entry_id: &FileEntryId,
        max_depth: usize,
    ) -> DbResult<(Vec<FileEntry>, bool)> {
        if let Some(owner) = crate::source_fjall::source_id(self.conn)? {
            let store = crate::source_fjall::require_source_store(self.conn)?;
            let catalog =
                persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
            let snapshot = catalog.read_snapshot();
            let mut chain = Vec::new();
            let mut current = Some(entry_id.0.clone());
            let mut visited = std::collections::BTreeSet::new();
            for _ in 0..=max_depth {
                let Some(id) = current else { break };
                if !visited.insert(id.clone()) {
                    break;
                }
                let Some(entry) = catalog
                    .get(&snapshot, &owner, &id)
                    .map_err(|error| crate::DbError::System(error.to_string()))?
                else {
                    break;
                };
                let parent = entry.parent_id.clone();
                chain.push(crate::source_fjall::file_entry(entry));
                current = parent;
            }
            chain.reverse();
            let complete = chain.first().is_some_and(|entry| entry.parent_id.is_none());
            return Ok((chain, complete));
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_by_partition_and_path(
        &self,
        data_source_id: &DataSourceId,
        partition_index: usize,
        path: &str,
    ) -> DbResult<Option<FileEntry>> {
        let normalized = path.trim_start_matches('/');
        let candidates = [
            path.to_owned(),
            format!("[P{partition_index}]/{normalized}"),
            format!("/{normalized}"),
        ];
        if let Some(entry) =
            self.native_partition_path(data_source_id, partition_index, &candidates, false)?
        {
            return Ok(entry);
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    /// Case-insensitive variant for well-known Windows system paths: NTFS
    /// preserves the install-time case (e.g. `Utilman.exe` vs `utilman.exe`),
    /// which must not produce false negatives in capability probes.
    pub fn find_by_partition_and_path_ci(
        &self,
        data_source_id: &DataSourceId,
        partition_index: usize,
        path: &str,
    ) -> DbResult<Option<FileEntry>> {
        let normalized = path.trim_start_matches('/');
        let candidates = [
            normalized.to_owned(),
            format!("[P{partition_index}]/{normalized}"),
            format!("/{normalized}"),
        ];
        if let Some(entry) =
            self.native_partition_path(data_source_id, partition_index, &candidates, true)?
        {
            return Ok(entry);
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn find_by_path_prefix(
        &self,
        data_source_id: &DataSourceId,
        prefix: &str,
    ) -> DbResult<Vec<FileEntry>> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            crate::source_fjall::require_source_store(self.conn)?;
            let mut entries = Vec::new();
            self.visit_all(Some(&data_source_id.0), |entry| {
                if entry.path.starts_with(prefix) {
                    push_bounded(&mut entries, entry)?;
                }
                Ok(())
            })?;
            entries.sort_by(|left, right| left.path.cmp(&right.path));
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn count_by_data_source(&self, data_source_id: &DataSourceId) -> DbResult<u64> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            crate::source_fjall::require_source_store(self.conn)?;
        }
        if let Some(count) = self.native_count(Some(&data_source_id.0))? {
            return Ok(count);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn count_all(&self) -> DbResult<u64> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            crate::source_fjall::require_source_store(self.conn)?;
        }
        if let Some(count) = self.native_count(None)? {
            return Ok(count);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_by_id(&self, id: &FileEntryId) -> DbResult<Option<FileEntry>> {
        if let Some(source_id) = crate::source_fjall::source_id(self.conn)? {
            let store = crate::source_fjall::require_source_store(self.conn)?;
            let snapshot = store.database().read_snapshot();
            let source = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
            return source
                .get(&snapshot, &source_id, &id.0)
                .map(|record| record.map(crate::source_fjall::file_entry))
                .map_err(|error| crate::connection::DbError::System(error.to_string()));
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_encryption_status(
        &self,
        id: &FileEntryId,
    ) -> DbResult<Option<FileEncryptionStatus>> {
        if let Some(owner) = crate::source_fjall::source_id(self.conn)? {
            let store = crate::source_fjall::require_source_store(self.conn)?;
            let repository =
                persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
            return repository
                .get(&repository.read_snapshot(), &owner, &id.0)
                .map(|record| {
                    record.map(|record| match record.encrypted {
                        None => FileEncryptionStatus::Unknown,
                        Some(true) => FileEncryptionStatus::Encrypted,
                        Some(false) => FileEncryptionStatus::Clear,
                    })
                })
                .map_err(|error| crate::connection::DbError::System(error.to_string()));
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_partition_index_by_id(&self, id: &FileEntryId) -> DbResult<Option<usize>> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            return crate::source_fjall::partition_index(self.conn, &id.0);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn set_partition_index_by_id(
        &self,
        id: &FileEntryId,
        partition_index: usize,
    ) -> DbResult<()> {
        if crate::source_fjall::set_partition_index(self.conn, &id.0, partition_index)? {
            return Ok(());
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn assign_partition_index_to_subtree(
        &self,
        root_id: &FileEntryId,
        partition_index: usize,
    ) -> DbResult<usize> {
        if let Some(updated) = crate::source_fjall::assign_partition_index_to_subtree(
            self.conn,
            &root_id.0,
            partition_index,
        )? {
            return Ok(updated);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    /// Find non-deleted file entries whose path contains every fragment
    /// (AND of escaped `LIKE '%fragment%'` terms), path-ordered. Fragments
    /// are matched with SQLite's default case-insensitive ASCII LIKE.
    pub fn find_by_path_fragments(
        &self,
        data_source_id: &DataSourceId,
        fragments: &[&str],
    ) -> DbResult<Vec<FileEntry>> {
        if fragments.is_empty() {
            return Ok(Vec::new());
        }
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            crate::source_fjall::require_source_store(self.conn)?;
            let mut entries = Vec::new();
            self.visit_all(Some(&data_source_id.0), |entry| {
                if !entry.deleted
                    && fragments
                        .iter()
                        .all(|fragment| entry.path.contains(fragment))
                {
                    push_bounded(&mut entries, entry)?;
                }
                Ok(())
            })?;
            entries.sort_by(|left, right| left.path.cmp(&right.path));
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
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
