use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluefsReplayRecord {
    pub inventory_id: String,
    pub transaction_count: u32,
    pub first_sequence: u64,
    pub final_sequence: u64,
    pub logical_bytes: u64,
    pub stop_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluefsDirectoryRecord {
    pub inventory_id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluefsFileRecord {
    pub inventory_id: String,
    pub path: String,
    pub inode: u64,
    pub size: u64,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub encoding: u8,
    pub content_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluefsFileExtentRecord {
    pub inventory_id: String,
    pub file_path: String,
    pub ordinal: u32,
    pub device_id: u8,
    pub offset: u64,
    pub length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluefsReplayAggregate {
    pub replay: CephBluefsReplayRecord,
    pub directories: Vec<CephBluefsDirectoryRecord>,
    pub files: Vec<CephBluefsFileRecord>,
    pub file_extents: Vec<CephBluefsFileExtentRecord>,
}

pub struct CephBluefsReplayRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephBluefsReplayRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_replay(&self, inventory_id: &str) -> DbResult<Option<CephBluefsReplayRecord>> {
        self.conn
            .query_row(
                "SELECT inventory_id, transaction_count, first_sequence, final_sequence,
                        logical_bytes, stop_reason
                 FROM ceph_bluefs_replays
                 WHERE inventory_id = ?1",
                params![inventory_id],
                map_replay,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_directories(&self, inventory_id: &str) -> DbResult<Vec<CephBluefsDirectoryRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, path
             FROM ceph_bluefs_directories
             WHERE inventory_id = ?1
             ORDER BY path",
        )?;
        let rows = statement.query_map(params![inventory_id], |row| {
            Ok(CephBluefsDirectoryRecord {
                inventory_id: row.get(0)?,
                path: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn find_files(&self, inventory_id: &str) -> DbResult<Vec<CephBluefsFileRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, path, inode, size, mtime_seconds, mtime_nanoseconds,
                    encoding, content_size
             FROM ceph_bluefs_files
             WHERE inventory_id = ?1
             ORDER BY path, inode",
        )?;
        let rows = statement.query_map(params![inventory_id], map_file)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn find_file_extents(
        &self,
        inventory_id: &str,
        file_path: &str,
    ) -> DbResult<Vec<CephBluefsFileExtentRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, file_path, ordinal, device_id, offset, length
             FROM ceph_bluefs_file_extents
             WHERE inventory_id = ?1 AND file_path = ?2
             ORDER BY ordinal",
        )?;
        let rows = statement.query_map(params![inventory_id, file_path], map_file_extent)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

pub(super) fn replace_for_inventory_on(
    conn: &Connection,
    records: &CephBluefsReplayAggregate,
) -> DbResult<()> {
    conn.execute(
        "DELETE FROM ceph_bluefs_replays WHERE inventory_id = ?1",
        params![records.replay.inventory_id],
    )?;
    insert_replay(conn, &records.replay)?;
    insert_directories(conn, &records.directories)?;
    insert_files(conn, &records.files)?;
    insert_file_extents(conn, &records.file_extents)
}

pub(super) fn validate_replacement(records: &CephBluefsReplayAggregate) -> DbResult<()> {
    validate_replay(&records.replay)?;
    let inventory_id = records.replay.inventory_id.as_str();
    let mut directories = HashSet::new();
    for directory in &records.directories {
        validate_binding(inventory_id, &directory.inventory_id, &directory.path)?;
        if !directories.insert(directory.path.as_str()) {
            return Err(DbError::System(format!(
                "BlueFS directory path is duplicated: {}",
                directory.path
            )));
        }
    }
    let mut files = HashSet::new();
    for file in &records.files {
        validate_binding(inventory_id, &file.inventory_id, &file.path)?;
        if file.inode <= 1 || file.mtime_nanoseconds >= 1_000_000_000 {
            return Err(DbError::System(
                "BlueFS file metadata is outside the supported range".to_string(),
            ));
        }
        if !files.insert(file.path.as_str()) {
            return Err(DbError::System(format!(
                "BlueFS file path is duplicated: {}",
                file.path
            )));
        }
    }
    let mut extent_keys = HashSet::new();
    for extent in &records.file_extents {
        validate_binding(inventory_id, &extent.inventory_id, &extent.file_path)?;
        if !files.contains(extent.file_path.as_str()) || extent.length == 0 {
            return Err(DbError::System(
                "BlueFS file extent references an unknown file or empty range".to_string(),
            ));
        }
        if !extent_keys.insert((extent.file_path.as_str(), extent.ordinal)) {
            return Err(DbError::System(format!(
                "BlueFS file extent ordinal is duplicated for {}",
                extent.file_path
            )));
        }
    }
    Ok(())
}

fn validate_replay(replay: &CephBluefsReplayRecord) -> DbResult<()> {
    if replay.inventory_id.is_empty()
        || replay.transaction_count == 0
        || replay.first_sequence == 0
        || replay.final_sequence < replay.first_sequence
        || replay.logical_bytes == 0
        || replay.stop_reason.is_empty()
    {
        return Err(DbError::System(
            "BlueFS replay summary is incomplete or inconsistent".to_string(),
        ));
    }
    Ok(())
}

fn validate_binding(inventory_id: &str, actual: &str, path: &str) -> DbResult<()> {
    if actual != inventory_id {
        return Err(DbError::System(
            "BlueFS replay record belongs to another inventory".to_string(),
        ));
    }
    if path.is_empty() || path.contains('\0') {
        return Err(DbError::System(
            "BlueFS replay path is empty or contains a null byte".to_string(),
        ));
    }
    Ok(())
}

fn insert_replay(conn: &Connection, replay: &CephBluefsReplayRecord) -> DbResult<()> {
    conn.execute(
        "INSERT INTO ceph_bluefs_replays (
            inventory_id, transaction_count, first_sequence, final_sequence,
            logical_bytes, stop_reason
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            replay.inventory_id,
            replay.transaction_count,
            replay.first_sequence,
            replay.final_sequence,
            replay.logical_bytes,
            replay.stop_reason,
        ],
    )?;
    Ok(())
}

fn insert_directories(conn: &Connection, records: &[CephBluefsDirectoryRecord]) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_bluefs_directories (inventory_id, path) VALUES (?1, ?2)",
    )?;
    for record in records {
        statement.execute(params![record.inventory_id, record.path])?;
    }
    Ok(())
}

fn insert_files(conn: &Connection, records: &[CephBluefsFileRecord]) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_bluefs_files (
            inventory_id, path, inode, size, mtime_seconds, mtime_nanoseconds,
            encoding, content_size
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.path,
            record.inode,
            record.size,
            record.mtime_seconds,
            record.mtime_nanoseconds,
            record.encoding,
            record.content_size,
        ])?;
    }
    Ok(())
}

fn insert_file_extents(conn: &Connection, records: &[CephBluefsFileExtentRecord]) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_bluefs_file_extents (
            inventory_id, file_path, ordinal, device_id, offset, length
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.file_path,
            record.ordinal,
            record.device_id,
            record.offset,
            record.length,
        ])?;
    }
    Ok(())
}

fn map_replay(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluefsReplayRecord> {
    Ok(CephBluefsReplayRecord {
        inventory_id: row.get(0)?,
        transaction_count: row.get(1)?,
        first_sequence: row.get(2)?,
        final_sequence: row.get(3)?,
        logical_bytes: row.get(4)?,
        stop_reason: row.get(5)?,
    })
}

fn map_file(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluefsFileRecord> {
    Ok(CephBluefsFileRecord {
        inventory_id: row.get(0)?,
        path: row.get(1)?,
        inode: row.get(2)?,
        size: row.get(3)?,
        mtime_seconds: row.get(4)?,
        mtime_nanoseconds: row.get(5)?,
        encoding: row.get(6)?,
        content_size: row.get(7)?,
    })
}

fn map_file_extent(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluefsFileExtentRecord> {
    Ok(CephBluefsFileExtentRecord {
        inventory_id: row.get(0)?,
        file_path: row.get(1)?,
        ordinal: row.get(2)?,
        device_id: row.get(3)?,
        offset: row.get(4)?,
        length: row.get(5)?,
    })
}
