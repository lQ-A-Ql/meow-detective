use rusqlite::{params, Connection};

use crate::connection::DbResult;

use super::{
    CephBluestoreOmapAggregate, CephBluestoreOmapScanRecord, CephBluestoreOmapScopeRecord,
    CephBluestoreRbdDirectoryRecord, CephBluestoreRbdHeaderRecord,
};

pub(super) fn replace_for_inventory_on(
    conn: &Connection,
    aggregate: &CephBluestoreOmapAggregate,
) -> DbResult<()> {
    conn.execute(
        "DELETE FROM ceph_bluestore_omap_scans WHERE inventory_id = ?1",
        params![aggregate.scan.inventory_id],
    )?;
    insert_scan(conn, &aggregate.scan)?;
    insert_scopes(conn, &aggregate.scopes)?;
    insert_directory_mappings(conn, &aggregate.directory_mappings)?;
    insert_headers(conn, &aggregate.rbd_headers)?;
    tracing::info!(
        inventory_id = aggregate.scan.inventory_id,
        scope_rows = aggregate.scopes.len(),
        directory_rows = aggregate.directory_mappings.len(),
        header_rows = aggregate.rbd_headers.len(),
        "Persisted normalized BlueStore OMAP snapshot"
    );
    Ok(())
}

fn insert_scan(conn: &Connection, record: &CephBluestoreOmapScanRecord) -> DbResult<()> {
    conn.execute(
        "INSERT INTO ceph_bluestore_omap_scans (
            inventory_id, data_source_id, schema_version, decode_profile,
            sharding_sha256, latest_state_sha256, semantic_sha256, omap_sha256,
            scope_count, directory_mapping_count, rbd_header_count, profile_complete
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            record.inventory_id,
            record.data_source_id,
            record.schema_version,
            record.decode_profile,
            record.sharding_sha256,
            record.latest_state_sha256,
            record.semantic_sha256,
            record.omap_sha256,
            record.scope_count,
            record.directory_mapping_count,
            record.rbd_header_count,
            record.profile_complete,
        ],
    )?;
    Ok(())
}

fn insert_scopes(conn: &Connection, records: &[CephBluestoreOmapScopeRecord]) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_bluestore_omap_scopes (
            inventory_id, scope_identity, key_family, pool_kind, pool_value_i64,
            pool_value_hex, hash, nid_hex, owner_nid_hex, owner_family, owner_kind,
            owner_image_id, entry_count, recognized_entry_count
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
         )",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.scope_identity,
            record.key_family,
            record.pool_kind,
            record.pool_value_i64,
            record.pool_value_hex,
            record.hash,
            record.nid_hex,
            record.owner_nid_hex,
            record.owner_family,
            record.owner_kind,
            record.owner_image_id,
            record.entry_count,
            record.recognized_entry_count,
        ])?;
    }
    Ok(())
}

fn insert_directory_mappings(
    conn: &Connection,
    records: &[CephBluestoreRbdDirectoryRecord],
) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_bluestore_rbd_directory (
            inventory_id, scope_identity, owner_nid_hex, image_name, image_id,
            bidirectional
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.scope_identity,
            record.owner_nid_hex,
            record.image_name,
            record.image_id,
            record.bidirectional,
        ])?;
    }
    Ok(())
}

fn insert_headers(conn: &Connection, records: &[CephBluestoreRbdHeaderRecord]) -> DbResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_bluestore_rbd_headers (
            inventory_id, scope_identity, owner_nid_hex, image_id, size_hex,
            object_order, features_hex, operation_features_hex,
            parent_key_present, object_prefix, stripe_unit_hex,
            stripe_count_hex, data_pool_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )?;
    for record in records {
        statement.execute(params![
            record.inventory_id,
            record.scope_identity,
            record.owner_nid_hex,
            record.image_id,
            record.size_hex,
            record.object_order,
            record.features_hex,
            record.operation_features_hex,
            record.parent_key_present,
            record.object_prefix,
            record.stripe_unit_hex,
            record.stripe_count_hex,
            record.data_pool_id,
        ])?;
    }
    Ok(())
}
