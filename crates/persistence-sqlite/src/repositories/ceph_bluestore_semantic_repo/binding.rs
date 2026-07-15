use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

use super::{validation, CephBluestoreSemanticAggregate, CephBluestoreSemanticScanRecord};
use crate::repositories::ceph_rocksdb_repo::{self, CephRocksdbRepo};

struct LatestStateBinding {
    sharding_sha256: String,
    latest_state_sha256: String,
}

pub(super) fn validate_persisted_binding(
    conn: &Connection,
    aggregate: &CephBluestoreSemanticAggregate,
) -> DbResult<()> {
    let device_size = validate_persisted_binding_for_read(conn, &aggregate.scan)?;
    validation::validate_device_bounds(aggregate, device_size)
}

pub(super) fn validate_persisted_binding_for_read(
    conn: &Connection,
    scan: &CephBluestoreSemanticScanRecord,
) -> DbResult<u64> {
    validation::validate_scan_for_read(scan)?;
    let inventory_id = scan.inventory_id.as_str();
    let manifest = CephRocksdbRepo::new(conn)
        .find_manifest(inventory_id)?
        .ok_or_else(|| {
            DbError::System(
                "BlueStore semantic snapshot references an unknown RocksDB inventory".to_string(),
            )
        })?;
    ceph_rocksdb_repo::validate_manifest_for_read(&manifest)?;
    let latest_state = find_latest_state_binding(conn, inventory_id)?.ok_or_else(|| {
        DbError::System(
            "BlueStore semantic snapshot references an incomplete RocksDB latest state".to_string(),
        )
    })?;
    let (data_source_id, device_size) = find_osd_binding(conn, inventory_id)?;
    if data_source_id != manifest.data_source_id {
        return Err(DbError::System(
            "BlueStore semantic snapshot crosses data-source ownership".to_string(),
        ));
    }
    validation::validate_recovery_binding_for_read_scalars(
        &manifest.inventory_id,
        &latest_state.sharding_sha256,
        &latest_state.latest_state_sha256,
        scan,
    )?;
    Ok(device_size)
}

fn find_latest_state_binding(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Option<LatestStateBinding>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, column_family_id, column_family_name,
                schema_version, sharding_sha256, latest_state_sha256,
                scan_complete
         FROM ceph_rocksdb_latest_state
         WHERE inventory_id = ?1
         ORDER BY column_family_id, column_family_name, latest_state_sha256",
    )?;
    let rows = statement.query_map(params![inventory_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, u32>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, bool>(6)?,
        ))
    })?;
    let mut scalar_rows = Vec::new();
    let mut column_family_ids = HashSet::new();
    let mut sharding_sha256 = None;
    for row in rows {
        let (
            row_inventory_id,
            column_family_id,
            column_family_name,
            schema_version,
            sharding,
            latest,
            scan_complete,
        ) = row?;
        if !column_family_ids.insert(column_family_id) {
            return Err(DbError::System(
                "RocksDB latest-state column family is duplicated".to_string(),
            ));
        }
        validate_latest_state_scalar(
            inventory_id,
            &row_inventory_id,
            &column_family_name,
            schema_version,
            &sharding,
            &latest,
            scan_complete,
        )?;
        if sharding_sha256
            .replace(sharding.clone())
            .is_some_and(|previous| previous != sharding)
        {
            return Err(DbError::System(
                "RocksDB latest-state rows do not share one sharding digest".to_string(),
            ));
        }
        scalar_rows.push((column_family_id, column_family_name, latest));
    }
    let Some(sharding_sha256) = sharding_sha256 else {
        return Ok(None);
    };
    Ok(Some(LatestStateBinding {
        sharding_sha256,
        latest_state_sha256: validation::latest_state_set_sha256_from_scalars(&scalar_rows),
    }))
}

fn validate_latest_state_scalar(
    inventory_id: &str,
    row_inventory_id: &str,
    column_family_name: &str,
    schema_version: u32,
    sharding_sha256: &str,
    latest_state_sha256: &str,
    scan_complete: bool,
) -> DbResult<()> {
    if row_inventory_id != inventory_id
        || column_family_name.is_empty()
        || column_family_name.contains('\0')
        || schema_version != 1
        || !is_sha256(sharding_sha256)
        || !is_sha256(latest_state_sha256)
        || !scan_complete
    {
        return Err(DbError::System(
            "RocksDB latest-state binding is incomplete or invalid".to_string(),
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
