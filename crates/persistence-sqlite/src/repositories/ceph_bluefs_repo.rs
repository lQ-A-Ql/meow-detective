use std::collections::HashSet;

use rusqlite::{params, Connection};

use crate::connection::{DbError, DbResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluefsSuperblockRecord {
    pub inventory_id: String,
    pub data_source_id: String,
    pub bluefs_uuid: String,
    pub osd_uuid: String,
    pub sequence: u64,
    pub block_size: u32,
    pub crc32c: u32,
    pub struct_version: u8,
    pub struct_compat_version: u8,
    pub log_inode: u64,
    pub log_size: u64,
    pub log_mtime_seconds: i64,
    pub log_mtime_nanoseconds: u32,
    pub log_encoding: u8,
    pub log_content_size: u64,
    pub shared_bdev: Option<u32>,
    pub dedicated_db: Option<bool>,
    pub dedicated_wal: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluefsLogExtentRecord {
    pub inventory_id: String,
    pub ordinal: u32,
    pub device_id: u8,
    pub offset: u64,
    pub length: u32,
}

pub struct CephBluefsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephBluefsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_by_data_source(
        &self,
        data_source_id: &str,
    ) -> DbResult<Vec<CephBluefsSuperblockRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, data_source_id, bluefs_uuid, osd_uuid, sequence,
                    block_size, crc32c, struct_version, struct_compat_version, log_inode,
                    log_size, log_mtime_seconds, log_mtime_nanoseconds, log_encoding,
                    log_content_size, shared_bdev, dedicated_db, dedicated_wal
             FROM ceph_bluefs_superblocks
             WHERE data_source_id = ?1
             ORDER BY osd_uuid, inventory_id",
        )?;
        let rows = statement.query_map(params![data_source_id], map_superblock)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn find_log_extents(&self, inventory_id: &str) -> DbResult<Vec<CephBluefsLogExtentRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT inventory_id, ordinal, device_id, offset, length
             FROM ceph_bluefs_log_extents
             WHERE inventory_id = ?1
             ORDER BY ordinal",
        )?;
        let rows = statement.query_map(params![inventory_id], map_extent)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

pub(super) fn replace_for_inventory_on(
    conn: &Connection,
    superblock: &CephBluefsSuperblockRecord,
    extents: &[CephBluefsLogExtentRecord],
) -> DbResult<()> {
    conn.execute(
        "DELETE FROM ceph_bluefs_superblocks WHERE inventory_id = ?1",
        params![superblock.inventory_id],
    )?;
    conn.execute(
        "INSERT INTO ceph_bluefs_superblocks (
                inventory_id, data_source_id, bluefs_uuid, osd_uuid, sequence,
                block_size, crc32c, struct_version, struct_compat_version, log_inode,
                log_size, log_mtime_seconds, log_mtime_nanoseconds, log_encoding,
                log_content_size, shared_bdev, dedicated_db, dedicated_wal
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18
             )",
        params![
            superblock.inventory_id,
            superblock.data_source_id,
            superblock.bluefs_uuid,
            superblock.osd_uuid,
            superblock.sequence,
            superblock.block_size,
            superblock.crc32c,
            superblock.struct_version,
            superblock.struct_compat_version,
            superblock.log_inode,
            superblock.log_size,
            superblock.log_mtime_seconds,
            superblock.log_mtime_nanoseconds,
            superblock.log_encoding,
            superblock.log_content_size,
            superblock.shared_bdev,
            superblock.dedicated_db,
            superblock.dedicated_wal,
        ],
    )?;
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_bluefs_log_extents (
            inventory_id, ordinal, device_id, offset, length
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for extent in extents {
        statement.execute(params![
            extent.inventory_id,
            extent.ordinal,
            extent.device_id,
            extent.offset,
            extent.length,
        ])?;
    }
    Ok(())
}

pub(super) fn validate_replacement(
    superblock: &CephBluefsSuperblockRecord,
    extents: &[CephBluefsLogExtentRecord],
) -> DbResult<()> {
    if superblock.block_size == 0 {
        return Err(DbError::System(
            "BlueFS superblock block size must be non-zero".to_string(),
        ));
    }
    if superblock.log_mtime_nanoseconds >= 1_000_000_000 {
        return Err(DbError::System(
            "BlueFS log mtime nanoseconds must be below one second".to_string(),
        ));
    }
    let mut ordinals = HashSet::new();
    for extent in extents {
        if extent.inventory_id != superblock.inventory_id {
            return Err(DbError::System(format!(
                "BlueFS log extent references another inventory: {}",
                extent.inventory_id
            )));
        }
        if extent.length == 0 {
            return Err(DbError::System(
                "BlueFS log extent length must be non-zero".to_string(),
            ));
        }
        if !ordinals.insert(extent.ordinal) {
            return Err(DbError::System(format!(
                "BlueFS log extent ordinal is duplicated: {}",
                extent.ordinal
            )));
        }
    }
    Ok(())
}

fn map_superblock(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluefsSuperblockRecord> {
    Ok(CephBluefsSuperblockRecord {
        inventory_id: row.get(0)?,
        data_source_id: row.get(1)?,
        bluefs_uuid: row.get(2)?,
        osd_uuid: row.get(3)?,
        sequence: row.get(4)?,
        block_size: row.get(5)?,
        crc32c: row.get(6)?,
        struct_version: row.get(7)?,
        struct_compat_version: row.get(8)?,
        log_inode: row.get(9)?,
        log_size: row.get(10)?,
        log_mtime_seconds: row.get(11)?,
        log_mtime_nanoseconds: row.get(12)?,
        log_encoding: row.get(13)?,
        log_content_size: row.get(14)?,
        shared_bdev: row.get(15)?,
        dedicated_db: row.get(16)?,
        dedicated_wal: row.get(17)?,
    })
}

fn map_extent(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluefsLogExtentRecord> {
    Ok(CephBluefsLogExtentRecord {
        inventory_id: row.get(0)?,
        ordinal: row.get(1)?,
        device_id: row.get(2)?,
        offset: row.get(3)?,
        length: row.get(4)?,
    })
}
