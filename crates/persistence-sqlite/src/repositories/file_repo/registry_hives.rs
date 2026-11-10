//! Stable, bounded discovery of live registry candidates in one source catalog.
use super::{push_bounded, FileRepo, MAX_FILE_QUERY_ROWS};
use crate::DbResult;
use domain::{DataSourceId, FileEntry};

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
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let paths = [format!("{path}.LOG1"), format!("{path}.LOG2")];
            let mut entries = Vec::new();
            let mut counts = [0; 2];
            self.visit_all(Some(&source.0), |entry| {
                if !entry.deleted && !entry.is_directory() {
                    let normalized = entry.path.replace('\\', "/");
                    for (index, path) in paths.iter().enumerate() {
                        if counts[index] < 2 && normalized.eq_ignore_ascii_case(path) {
                            push_bounded(&mut entries, entry.clone())?;
                            counts[index] += 1;
                        }
                    }
                }
                Ok(())
            })?;
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }
    pub fn registry_hive_candidates(
        &self,
        source: &DataSourceId,
        after: &str,
        limit: u32,
    ) -> DbResult<Vec<FileEntry>> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            if limit as usize > MAX_FILE_QUERY_ROWS {
                return Err(crate::DbError::System(
                    "registry candidate query exceeds the bounded row limit".into(),
                ));
            }
            let mut entries = Vec::new();
            let mut after = after.to_owned();
            while entries.len() < limit as usize {
                let page = self.index_page(&source.0, &after, 512)?;
                if page.is_empty() {
                    break;
                }
                for entry in page {
                    after = entry.id.0.clone();
                    if is_hive(&entry) {
                        entries.push(entry);
                    }
                    if entries.len() == limit as usize {
                        break;
                    }
                }
            }
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn registry_hive_candidate_count(&self, source: &DataSourceId) -> DbResult<u64> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let mut count = 0;
            self.visit_all(Some(&source.0), |entry| {
                count += u64::from(is_hive(&entry));
                Ok(())
            })?;
            return Ok(count);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }
}

fn is_hive(entry: &FileEntry) -> bool {
    !entry.deleted
        && !entry.is_directory()
        && matches!(
            entry.name.to_ascii_lowercase().as_str(),
            "system"
                | "software"
                | "sam"
                | "security"
                | "default"
                | "ntuser.dat"
                | "usrclass.dat"
                | "amcache.hve"
        )
}
