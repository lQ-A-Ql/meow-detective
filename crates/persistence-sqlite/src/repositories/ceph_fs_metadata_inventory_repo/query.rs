use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::DbError;

use super::{
    CephFsMetadataInventory, CephFsMetadataInventoryManifest, CephFsMetadataInventoryRepoResult,
    CephFsMetadataObjectProjection,
};

pub(super) fn find(
    conn: &Connection,
    filesystem_identity: &str,
    inventory_id: &str,
) -> CephFsMetadataInventoryRepoResult<Option<CephFsMetadataInventory>> {
    let Some(manifest) = find_manifest(conn, filesystem_identity, inventory_id)? else {
        return Ok(None);
    };
    let mut statement = conn
        .prepare(
            "SELECT object_identity_sha256, locator, candidate_mask,
                    classification_state, classifier_rule, record_sha256
             FROM ceph_fs_metadata_objects
             WHERE filesystem_identity = ?1 AND inventory_id = ?2
             ORDER BY locator, object_identity_sha256",
        )
        .map_err(DbError::from)?;
    let rows = statement
        .query_map(params![filesystem_identity, inventory_id], |row| {
            Ok(CephFsMetadataObjectProjection {
                object_identity_sha256: row.get(0)?,
                locator: row.get(1)?,
                candidate_mask: row.get(2)?,
                classification_state: row.get(3)?,
                classifier_rule: row.get(4)?,
                record_sha256: row.get(5)?,
            })
        })
        .map_err(DbError::from)?;
    let objects = rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)?;
    Ok(Some(CephFsMetadataInventory { manifest, objects }))
}

pub(super) fn find_manifest(
    conn: &Connection,
    filesystem_identity: &str,
    inventory_id: &str,
) -> CephFsMetadataInventoryRepoResult<Option<CephFsMetadataInventoryManifest>> {
    conn.query_row(
        "SELECT filesystem_identity, inventory_id, data_source_id, filesystem_id,
                fsmap_epoch, metadata_pool_id, schema_version, classifier_profile,
                source_semantic_sha256, inventory_sha256, object_count,
                unknown_object_count, complete
         FROM ceph_fs_metadata_inventories
         WHERE filesystem_identity = ?1 AND inventory_id = ?2",
        params![filesystem_identity, inventory_id],
        |row| {
            Ok(CephFsMetadataInventoryManifest {
                filesystem_identity: row.get(0)?,
                inventory_id: row.get(1)?,
                data_source_id: row.get(2)?,
                filesystem_id: row.get(3)?,
                fsmap_epoch: row.get(4)?,
                metadata_pool_id: row.get(5)?,
                schema_version: row.get(6)?,
                classifier_profile: row.get(7)?,
                source_semantic_sha256: row.get(8)?,
                inventory_sha256: row.get(9)?,
                object_count: row.get(10)?,
                unknown_object_count: row.get(11)?,
                complete: row.get(12)?,
            })
        },
    )
    .optional()
    .map_err(DbError::from)
    .map_err(Into::into)
}

pub(super) fn find_object_by_locator(
    conn: &Connection,
    filesystem_identity: &str,
    inventory_id: &str,
    locator: &str,
) -> CephFsMetadataInventoryRepoResult<Option<CephFsMetadataObjectProjection>> {
    conn.query_row(
        "SELECT object_identity_sha256, locator, candidate_mask,
                classification_state, classifier_rule, record_sha256
         FROM ceph_fs_metadata_objects
         WHERE filesystem_identity = ?1 AND inventory_id = ?2 AND locator = ?3",
        params![filesystem_identity, inventory_id, locator],
        |row| {
            Ok(CephFsMetadataObjectProjection {
                object_identity_sha256: row.get(0)?,
                locator: row.get(1)?,
                candidate_mask: row.get(2)?,
                classification_state: row.get(3)?,
                classifier_rule: row.get(4)?,
                record_sha256: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(DbError::from)
    .map_err(Into::into)
}
