use crate::{DbError, DbResult};
use rusqlite::{params, Connection, OptionalExtension};

pub const NTFS_STAGING_LAYOUT_VERSION: &str = "ntfs-v3";
pub struct NtfsStagingRepo;
impl NtfsStagingRepo {
    /// Called only by the partition-staging owner. A known v2 numeric catalog
    /// is disposable and must be enumerated again; published source DBs refuse it.
    pub fn rebuild_previous_layout(conn: &Connection) -> DbResult<bool> {
        let has_meta: bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ntfs_staging_meta')",[],|r|r.get(0))?;
        if !has_meta || Self::meta(conn, "layout_version")?.as_deref() != Some("ntfs-v2") {
            return Ok(false);
        }
        let published: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='source_meta')",
            [],
            |r| r.get(0),
        )?;
        if published {
            return Err(DbError::System(
                "Cannot rebuild published source as staging".into(),
            ));
        }
        conn.prepare(
            "SELECT record_num,parent_record_num,sequence_num,name,path FROM ntfs_entries LIMIT 0",
        )?;
        with_layout_savepoint(conn, || {
            if Self::is_numeric(conn)? {
                conn.execute_batch("DROP VIEW file_entries")?;
                conn.execute_batch(include_str!("../migrations/scripts/staging_001.sql"))?;
                conn.execute_batch("DELETE FROM staging_meta WHERE key IN ('status','enum_strategy','mft_records','mft_fallback_warning','file_count','dir_count')")?;
            } else {
                let rows: u64 =
                    conn.query_row("SELECT COUNT(*) FROM ntfs_entries", [], |r| r.get(0))?;
                if rows != 0 {
                    return Err(DbError::System("Inconsistent legacy staging facts".into()));
                }
            }
            conn.execute_batch("DROP TABLE ntfs_entries; DROP TABLE ntfs_staging_meta")?;
            Self::ensure_schema(conn)?;
            Ok(true)
        })
    }
    pub fn is_numeric(conn: &Connection) -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='view' AND name='file_entries')",
            [],
            |row| row.get(0),
        )
    }

    pub fn tree_shape(conn: &Connection) -> rusqlite::Result<(i64, i64, i64, i64)> {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS ntfs_parent ON ntfs_paths(catalog_parent_record_num)",
        )?;
        conn.query_row(
            "SELECT (SELECT COUNT(*) FROM ntfs_entries),
              (SELECT COUNT(*) FROM ntfs_paths p JOIN ntfs_entries f USING(record_num) WHERE p.record_num=5 AND p.catalog_parent_record_num IS NULL AND f.entry_type=1),
              (SELECT COUNT(*) FROM ntfs_paths child WHERE catalog_parent_record_num IS NOT NULL AND NOT EXISTS
                (SELECT 1 FROM ntfs_paths parent WHERE parent.record_num=child.catalog_parent_record_num)),
              (WITH RECURSIVE reachable(id) AS (SELECT record_num FROM ntfs_paths WHERE record_num=5
                UNION SELECT child.record_num FROM ntfs_paths child JOIN reachable parent ON child.catalog_parent_record_num=parent.id)
                SELECT COUNT(*) FROM reachable)", [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
    }

    pub fn ensure_schema(conn: &Connection) -> DbResult<()> {
        let published: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='source_meta')",
            [],
            |r| r.get(0),
        )?;
        if published {
            return Err(DbError::System(
                "NTFS staging layout cannot replace a source database".into(),
            ));
        }
        conn.execute_batch("CREATE TABLE IF NOT EXISTS ntfs_staging_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")?;
        if let Some(version) = Self::meta(conn, "layout_version")? {
            if version != NTFS_STAGING_LAYOUT_VERSION {
                return Err(DbError::System(
                    "Unsupported NTFS staging layout; re-enumeration required".into(),
                ));
            }
            validate_numeric_schema(conn)?;
            return Ok(());
        }
        let unversioned: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ntfs_entries')",
            [],
            |r| r.get(0),
        )?;
        if unversioned {
            return Err(DbError::System(
                "Unversioned NTFS facts require re-enumeration".into(),
            ));
        }
        conn.execute_batch(include_str!("ntfs_staging/schema.sql"))?;
        Self::set_meta(conn, "layout_version", NTFS_STAGING_LAYOUT_VERSION)
    }
    /// IDs are projected only when the completed catalog is inspected or merged.
    pub fn begin(conn: &Connection, source_id: &str, partition: usize) -> DbResult<()> {
        with_layout_savepoint(conn, || Self::begin_layout(conn, source_id, partition))
    }
    fn begin_layout(conn: &Connection, source_id: &str, partition: usize) -> DbResult<()> {
        Self::ensure_schema(conn)?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
        if count != 0 {
            return Err(DbError::System(
                "Cannot replace nonempty staging catalog".into(),
            ));
        }
        conn.execute_batch("DROP INDEX IF EXISTS ntfs_parent")?;
        Self::set_meta(conn, "source_id", source_id)?;
        Self::set_meta(conn, "partition", &partition.to_string())?;
        conn.execute_batch("DROP TABLE file_entries;")?;
        conn.execute_batch(include_str!("ntfs_staging/projection.sql"))?;
        Ok(())
    }
    pub fn reset_layout(conn: &Connection) -> DbResult<()> {
        with_layout_savepoint(conn, || Self::reset_layout_inner(conn))
    }
    fn reset_layout_inner(conn: &Connection) -> DbResult<()> {
        let has_entries: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ntfs_entries')",
            [],
            |r| r.get(0),
        )?;
        if has_entries {
            Self::ensure_schema(conn)?;
        }
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
        with_layout_savepoint(conn, || {
            Self::ensure_schema(conn)?;
            conn.execute_batch("DROP INDEX IF EXISTS ntfs_parent; DELETE FROM ntfs_paths; DELETE FROM ntfs_entries; DELETE FROM ntfs_staging_meta WHERE key <> 'layout_version';")?;
            Ok(())
        })
    }
}

fn validate_numeric_schema(conn: &Connection) -> DbResult<()> {
    conn.prepare("SELECT record_num,parent_record_num,sequence_num,name,entry_type,size,ext,deleted,hidden,system,read_only,archive,encrypted,created_at,modified_at,accessed_at,changed_at FROM ntfs_entries LIMIT 0")?;
    conn.prepare("SELECT record_num,resolved_path,catalog_name,catalog_parent_record_num,resolution_status FROM ntfs_paths LIMIT 0")?;
    let valid:bool=conn.query_row("SELECT (SELECT COUNT(*)=17 FROM pragma_table_info('ntfs_entries')) AND (SELECT COUNT(*)=5 FROM pragma_table_info('ntfs_paths')) AND EXISTS(SELECT 1 FROM pragma_table_info('ntfs_entries') WHERE name='record_num' AND type='INTEGER' AND pk=1) AND EXISTS(SELECT 1 FROM pragma_table_info('ntfs_paths') WHERE name='record_num' AND type='INTEGER' AND pk=1)",[],|r|r.get(0))?;
    if !valid {
        return Err(DbError::System("Inconsistent NTFS numeric layout".into()));
    }
    Ok(())
}

fn with_layout_savepoint<T>(
    conn: &Connection,
    action: impl FnOnce() -> DbResult<T>,
) -> DbResult<T> {
    conn.execute_batch("SAVEPOINT ntfs_layout")?;
    let result = action().and_then(|value| {
        conn.execute_batch("RELEASE ntfs_layout")?;
        Ok(value)
    });
    if result.is_err() && !conn.is_autocommit() {
        // SQLITE_FULL can already have rolled back the outer transaction.
        conn.execute_batch("ROLLBACK TO ntfs_layout; RELEASE ntfs_layout")?;
    }
    result
}
