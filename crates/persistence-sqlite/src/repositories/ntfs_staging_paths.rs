//! Narrow derived path projection, separate from parsed NTFS record facts.
use super::ntfs_staging_repo::NtfsStagingRepo;
use rusqlite::{CachedStatement, Connection};

impl NtfsStagingRepo {
    /// A path row distinguishes computed NULL parent from a missing projection.
    /// This savepoint nests inside the partition transaction and never publishes it.
    pub fn write_paths<T, E>(
        conn: &Connection,
        write: impl FnOnce(&mut CachedStatement<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<rusqlite::Error>,
    {
        conn.execute_batch("SAVEPOINT ntfs_paths_build")?;
        let result = (|| {
            conn.execute_batch("DROP INDEX IF EXISTS ntfs_parent; DELETE FROM ntfs_paths; DELETE FROM ntfs_staging_meta WHERE key IN ('paths_complete','tree_validated')")?;
            let mut statement=conn.prepare_cached("INSERT INTO ntfs_paths(record_num,resolved_path,catalog_name,catalog_parent_record_num,resolution_status) VALUES(?1,?2,?3,?4,?5)")?;
            let value = write(&mut statement)?;
            drop(statement);
            conn.execute(
                "INSERT OR REPLACE INTO ntfs_staging_meta(key,value) VALUES('paths_complete','1')",
                [],
            )?;
            conn.execute_batch("RELEASE ntfs_paths_build")?;
            Ok(value)
        })();
        if result.is_err() && !conn.is_autocommit() {
            conn.execute_batch("ROLLBACK TO ntfs_paths_build; RELEASE ntfs_paths_build")?;
        }
        result
    }

    pub fn validate_paths(conn: &Connection) -> rusqlite::Result<()> {
        let (complete,difference,extra):(bool,i64,bool)=conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM ntfs_staging_meta WHERE key='paths_complete' AND value='1'),
            (SELECT COUNT(*) FROM ntfs_entries)-(SELECT COUNT(*) FROM ntfs_paths),
            EXISTS(SELECT 1 FROM ntfs_paths p WHERE NOT EXISTS(SELECT 1 FROM ntfs_entries f WHERE f.record_num=p.record_num))",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if !complete || difference != 0 || extra {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(())
    }
}
