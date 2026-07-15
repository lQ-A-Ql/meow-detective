use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

use super::CephBluestoreOmapAggregate;
use crate::repositories::{
    ceph_bluestore_semantic_repo::{latest_state_set_sha256, CephBluestoreSemanticAggregate},
    ceph_rocksdb_latest_state_repo::{CephRocksdbLatestStateRecord, CephRocksdbLatestStateRepo},
    ceph_rocksdb_repo::CephRocksdbAggregate,
};

pub(crate) fn validate_recovery_binding(
    rocksdb: &CephRocksdbAggregate,
    latest_state: &[CephRocksdbLatestStateRecord],
    semantic: &CephBluestoreSemanticAggregate,
    omap: &CephBluestoreOmapAggregate,
) -> DbResult<()> {
    let inventory_id = rocksdb.manifest.inventory_id.as_str();
    let latest_state_sha256 = latest_state_set_sha256(latest_state);
    if omap.scan.inventory_id != inventory_id
        || omap.scan.data_source_id != rocksdb.manifest.data_source_id
        || semantic.scan.inventory_id != inventory_id
        || omap.scan.sharding_sha256 != semantic.scan.sharding_sha256
        || omap.scan.latest_state_sha256 != latest_state_sha256
        || omap.scan.latest_state_sha256 != semantic.scan.latest_state_sha256
        || omap.scan.semantic_sha256 != semantic.scan.semantic_sha256
    {
        return Err(DbError::System(
            "BlueStore OMAP snapshot does not match its OSD latest-state and semantic identity"
                .to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_persisted_binding(
    conn: &Connection,
    aggregate: &CephBluestoreOmapAggregate,
) -> DbResult<()> {
    let inventory_id = aggregate.scan.inventory_id.as_str();
    let parent = find_parent_binding(conn, inventory_id)?.ok_or_else(|| {
        DbError::System(
            "BlueStore OMAP snapshot references an incomplete OSD semantic parent".to_string(),
        )
    })?;
    let latest_state = CephRocksdbLatestStateRepo::new(conn).find(inventory_id)?;
    if latest_state.is_empty()
        || aggregate.scan.data_source_id != parent.osd_data_source_id
        || aggregate.scan.data_source_id != parent.manifest_data_source_id
        || aggregate.scan.sharding_sha256 != parent.sharding_sha256
        || aggregate.scan.latest_state_sha256 != parent.latest_state_sha256
        || aggregate.scan.latest_state_sha256 != latest_state_set_sha256(&latest_state)
        || aggregate.scan.semantic_sha256 != parent.semantic_sha256
    {
        return Err(DbError::System(
            "BlueStore OMAP snapshot crosses or mismatches its persisted OSD recovery identity"
                .to_string(),
        ));
    }
    Ok(())
}

struct ParentBinding {
    osd_data_source_id: String,
    manifest_data_source_id: String,
    sharding_sha256: String,
    latest_state_sha256: String,
    semantic_sha256: String,
}

fn find_parent_binding(conn: &Connection, inventory_id: &str) -> DbResult<Option<ParentBinding>> {
    conn.query_row(
        "SELECT osd.data_source_id, manifest.data_source_id,
                semantic.sharding_sha256, semantic.latest_state_sha256,
                semantic.semantic_sha256
         FROM ceph_osd_inventory osd
         JOIN ceph_rocksdb_manifests manifest
           ON manifest.inventory_id = osd.id
         JOIN ceph_bluestore_semantic_scans semantic
           ON semantic.inventory_id = manifest.inventory_id
         WHERE osd.id = ?1",
        params![inventory_id],
        |row| {
            Ok(ParentBinding {
                osd_data_source_id: row.get(0)?,
                manifest_data_source_id: row.get(1)?,
                sharding_sha256: row.get(2)?,
                latest_state_sha256: row.get(3)?,
                semantic_sha256: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}
