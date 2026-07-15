use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

use super::{validation, CephBluestoreSemanticAggregate};
use crate::repositories::{
    ceph_rocksdb_latest_state_repo::{self, CephRocksdbLatestStateRepo},
    ceph_rocksdb_repo::CephRocksdbRepo,
};

pub(super) fn validate_persisted_binding(
    conn: &Connection,
    aggregate: &CephBluestoreSemanticAggregate,
) -> DbResult<()> {
    let inventory_id = aggregate.scan.inventory_id.as_str();
    let rocksdb = CephRocksdbRepo::new(conn)
        .find_aggregate(inventory_id)?
        .ok_or_else(|| {
            DbError::System(
                "BlueStore semantic snapshot references an unknown RocksDB inventory".to_string(),
            )
        })?;
    let latest_state = CephRocksdbLatestStateRepo::new(conn).find(inventory_id)?;
    ceph_rocksdb_latest_state_repo::validate_replacement(&rocksdb, &latest_state)?;
    let (data_source_id, device_size) = find_osd_binding(conn, inventory_id)?;
    if data_source_id != rocksdb.manifest.data_source_id {
        return Err(DbError::System(
            "BlueStore semantic snapshot crosses data-source ownership".to_string(),
        ));
    }
    validation::validate_recovery_binding(&rocksdb, &latest_state, aggregate)?;
    validation::validate_device_bounds(aggregate, device_size)
}

fn find_osd_binding(conn: &Connection, inventory_id: &str) -> DbResult<(String, u64)> {
    conn.query_row(
        "SELECT data_source_id, device_size
         FROM ceph_osd_inventory
         WHERE id = ?1",
        params![inventory_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| DbError::System("BlueStore semantic snapshot has no OSD inventory".to_string()))
}
