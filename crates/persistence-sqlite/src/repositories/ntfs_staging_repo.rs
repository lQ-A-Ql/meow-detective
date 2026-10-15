use crate::connection::DbResult;
use rusqlite::{params, Connection, OptionalExtension};

pub const NTFS_STAGING_LAYOUT_VERSION: &str = "ntfs-v2";
pub struct NtfsStagingRepo;
impl NtfsStagingRepo {
    pub fn ensure_schema(conn: &Connection) -> DbResult<()> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS ntfs_entries (record_num INTEGER PRIMARY KEY, parent_record_num INTEGER, sequence_num INTEGER NOT NULL, name TEXT NOT NULL, path TEXT, entry_type INTEGER NOT NULL, size INTEGER, deleted INTEGER NOT NULL, hidden INTEGER NOT NULL, system INTEGER NOT NULL, read_only INTEGER NOT NULL, archive INTEGER NOT NULL, encrypted INTEGER, created_at TEXT, modified_at TEXT, accessed_at TEXT, changed_at TEXT); CREATE TABLE IF NOT EXISTS ntfs_staging_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
        Self::set_meta(conn, "layout_version", NTFS_STAGING_LAYOUT_VERSION)?;
        Ok(())
    }
    pub fn set_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
        conn.execute(
            "INSERT OR REPLACE INTO ntfs_staging_meta(key,value) VALUES(?1,?2)",
            params![key, value],
        )?;
        Ok(())
    }
    pub fn meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
        conn.query_row(
            "SELECT value FROM ntfs_staging_meta WHERE key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }
    pub fn clear(conn: &Connection) -> DbResult<()> {
        conn.execute_batch("DELETE FROM ntfs_entries; DELETE FROM ntfs_staging_meta WHERE key <> 'layout_version';")?;
        Ok(())
    }
}
