use crate::{DbError, DbResult};
use rusqlite::{params, Connection, OptionalExtension};

pub const NTFS_STAGING_LAYOUT_VERSION: &str = "ntfs-v2";
pub struct NtfsStagingRepo;
impl NtfsStagingRepo {
    pub fn is_numeric(conn: &Connection) -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='view' AND name='file_entries')",
            [],
            |row| row.get(0),
        )
    }

    pub fn tree_shape(conn: &Connection) -> rusqlite::Result<(i64, i64, i64, i64)> {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS ntfs_parent ON ntfs_entries(parent_record_num)",
        )?;
        conn.query_row(
            "SELECT (SELECT COUNT(*) FROM ntfs_entries),
              (SELECT COUNT(*) FROM ntfs_entries WHERE record_num=5 AND parent_record_num IS NULL AND entry_type=1),
              (SELECT COUNT(*) FROM ntfs_entries child WHERE parent_record_num IS NOT NULL AND NOT EXISTS
                (SELECT 1 FROM ntfs_entries parent WHERE parent.record_num=child.parent_record_num)),
              (WITH RECURSIVE reachable(id) AS (SELECT record_num FROM ntfs_entries WHERE record_num=5
                UNION SELECT child.record_num FROM ntfs_entries child JOIN reachable parent ON child.parent_record_num=parent.id)
                SELECT COUNT(*) FROM reachable)", [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
    }

    pub fn ensure_schema(conn: &Connection) -> DbResult<()> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS ntfs_staging_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")?;
        if let Some(version) = Self::meta(conn, "layout_version")? {
            if version != NTFS_STAGING_LAYOUT_VERSION {
                return Err(DbError::System(
                    "Unsupported NTFS staging layout; re-enumeration required".into(),
                ));
            }
            return Ok(());
        }
        conn.execute_batch(include_str!("ntfs_staging/schema.sql"))?;
        Self::set_meta(conn, "layout_version", NTFS_STAGING_LAYOUT_VERSION)
    }
    /// IDs are projected only when the completed catalog is inspected or merged.
    pub fn begin(conn: &Connection, source_id: &str, partition: usize) -> DbResult<()> {
        Self::ensure_schema(conn)?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
        if count != 0 {
            return Err(DbError::System(
                "Cannot replace nonempty staging catalog".into(),
            ));
        }
        Self::set_meta(conn, "source_id", source_id)?;
        Self::set_meta(conn, "partition", &partition.to_string())?;
        conn.execute_batch("DROP TABLE file_entries;")?;
        conn.execute_batch(include_str!("ntfs_staging/projection.sql"))?;
        Ok(())
    }
    pub fn reset_layout(conn: &Connection) -> DbResult<()> {
        let is_view: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='view' AND name='file_entries')",
            [],
            |row| row.get(0),
        )?;
        if is_view {
            conn.execute_batch("DROP VIEW file_entries;")?;
            conn.execute_batch(include_str!("../migrations/scripts/staging_001.sql"))?;
        }
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ntfs_entries')",
            [],
            |row| row.get(0),
        )?;
        if exists {
            Self::clear(conn)?;
        }
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
