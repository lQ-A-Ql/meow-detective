use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::DbResult;

use super::{
    validation, CephBluestoreOmapAggregate, CephBluestoreOmapScanRecord,
    CephBluestoreOmapScopeRecord, CephBluestoreRbdDirectoryRecord, CephBluestoreRbdHeaderRecord,
};

const SCOPE_COLUMNS: &str = "
    inventory_id, scope_identity, key_family, pool_kind, pool_value_i64,
    pool_value_hex, hash, nid_hex, owner_nid_hex, owner_family, owner_kind,
    owner_image_id, entry_count, recognized_entry_count";

pub(super) fn find_aggregate(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Option<CephBluestoreOmapAggregate>> {
    let Some(scan) = find_scan(conn, inventory_id)? else {
        return Ok(None);
    };
    Ok(Some(CephBluestoreOmapAggregate {
        scan,
        scopes: find_scopes(conn, inventory_id)?,
        directory_mappings: find_directory_mappings(conn, inventory_id)?,
        rbd_headers: find_headers(conn, inventory_id)?,
    }))
}

pub(super) fn find_scopes_by_family(
    conn: &Connection,
    inventory_id: &str,
    key_family: &str,
) -> DbResult<Vec<CephBluestoreOmapScopeRecord>> {
    if inventory_id.is_empty() || !validation::valid_family(key_family) {
        return Err(crate::connection::DbError::System(
            "BlueStore OMAP family query is invalid".to_string(),
        ));
    }
    query_scopes(
        conn,
        &format!(
            "SELECT {SCOPE_COLUMNS}
             FROM ceph_bluestore_omap_scopes
             WHERE inventory_id = ?1 AND key_family = ?2
             ORDER BY scope_identity"
        ),
        params![inventory_id, key_family],
    )
}

pub(super) fn find_scopes_by_owner(
    conn: &Connection,
    inventory_id: &str,
    owner_nid_hex: &str,
) -> DbResult<Vec<CephBluestoreOmapScopeRecord>> {
    if inventory_id.is_empty() || !validation::valid_hex_u64(owner_nid_hex) {
        return Err(crate::connection::DbError::System(
            "BlueStore OMAP owner query is invalid".to_string(),
        ));
    }
    query_scopes(
        conn,
        &format!(
            "SELECT {SCOPE_COLUMNS}
             FROM ceph_bluestore_omap_scopes
             WHERE inventory_id = ?1 AND owner_nid_hex = ?2
             ORDER BY key_family, scope_identity"
        ),
        params![inventory_id, owner_nid_hex],
    )
}

pub(super) fn find_rbd_header(
    conn: &Connection,
    inventory_id: &str,
    image_id: &str,
) -> DbResult<Option<CephBluestoreRbdHeaderRecord>> {
    if inventory_id.is_empty() || image_id.is_empty() || image_id.contains('\0') {
        return Err(crate::connection::DbError::System(
            "BlueStore RBD header query is invalid".to_string(),
        ));
    }
    conn.query_row(
        "SELECT inventory_id, scope_identity, owner_nid_hex, image_id, size_hex,
                object_order, features_hex, object_prefix, stripe_unit_hex,
                stripe_count_hex, data_pool_id
         FROM ceph_bluestore_rbd_headers
         WHERE inventory_id = ?1 AND image_id = ?2",
        params![inventory_id, image_id],
        map_header,
    )
    .optional()
    .map_err(Into::into)
}

fn find_scan(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Option<CephBluestoreOmapScanRecord>> {
    conn.query_row(
        "SELECT inventory_id, data_source_id, schema_version, decode_profile,
                sharding_sha256, latest_state_sha256, semantic_sha256, omap_sha256,
                scope_count, directory_mapping_count, rbd_header_count, profile_complete
         FROM ceph_bluestore_omap_scans
         WHERE inventory_id = ?1",
        params![inventory_id],
        map_scan,
    )
    .optional()
    .map_err(Into::into)
}

fn find_scopes(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephBluestoreOmapScopeRecord>> {
    query_scopes(
        conn,
        &format!(
            "SELECT {SCOPE_COLUMNS}
             FROM ceph_bluestore_omap_scopes
             WHERE inventory_id = ?1
             ORDER BY scope_identity"
        ),
        params![inventory_id],
    )
}

fn query_scopes(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> DbResult<Vec<CephBluestoreOmapScopeRecord>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map(params, map_scope)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn find_directory_mappings(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephBluestoreRbdDirectoryRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, scope_identity, owner_nid_hex, image_name,
                image_id, bidirectional
         FROM ceph_bluestore_rbd_directory
         WHERE inventory_id = ?1
         ORDER BY scope_identity, image_name, image_id",
    )?;
    let rows = statement.query_map(params![inventory_id], map_directory_mapping)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn find_headers(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephBluestoreRbdHeaderRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, scope_identity, owner_nid_hex, image_id, size_hex,
                object_order, features_hex, object_prefix, stripe_unit_hex,
                stripe_count_hex, data_pool_id
         FROM ceph_bluestore_rbd_headers
         WHERE inventory_id = ?1
         ORDER BY image_id",
    )?;
    let rows = statement.query_map(params![inventory_id], map_header)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn map_scan(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreOmapScanRecord> {
    Ok(CephBluestoreOmapScanRecord {
        inventory_id: row.get(0)?,
        data_source_id: row.get(1)?,
        schema_version: row.get(2)?,
        decode_profile: row.get(3)?,
        sharding_sha256: row.get(4)?,
        latest_state_sha256: row.get(5)?,
        semantic_sha256: row.get(6)?,
        omap_sha256: row.get(7)?,
        scope_count: row.get(8)?,
        directory_mapping_count: row.get(9)?,
        rbd_header_count: row.get(10)?,
        profile_complete: row.get(11)?,
    })
}

fn map_scope(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreOmapScopeRecord> {
    Ok(CephBluestoreOmapScopeRecord {
        inventory_id: row.get(0)?,
        scope_identity: row.get(1)?,
        key_family: row.get(2)?,
        pool_kind: row.get(3)?,
        pool_value_i64: row.get(4)?,
        pool_value_hex: row.get(5)?,
        hash: row.get(6)?,
        nid_hex: row.get(7)?,
        owner_nid_hex: row.get(8)?,
        owner_family: row.get(9)?,
        owner_kind: row.get(10)?,
        owner_image_id: row.get(11)?,
        entry_count: row.get(12)?,
        recognized_entry_count: row.get(13)?,
    })
}

fn map_directory_mapping(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CephBluestoreRbdDirectoryRecord> {
    Ok(CephBluestoreRbdDirectoryRecord {
        inventory_id: row.get(0)?,
        scope_identity: row.get(1)?,
        owner_nid_hex: row.get(2)?,
        image_name: row.get(3)?,
        image_id: row.get(4)?,
        bidirectional: row.get(5)?,
    })
}

fn map_header(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreRbdHeaderRecord> {
    Ok(CephBluestoreRbdHeaderRecord {
        inventory_id: row.get(0)?,
        scope_identity: row.get(1)?,
        owner_nid_hex: row.get(2)?,
        image_id: row.get(3)?,
        size_hex: row.get(4)?,
        object_order: row.get(5)?,
        features_hex: row.get(6)?,
        object_prefix: row.get(7)?,
        stripe_unit_hex: row.get(8)?,
        stripe_count_hex: row.get(9)?,
        data_pool_id: row.get(10)?,
    })
}
