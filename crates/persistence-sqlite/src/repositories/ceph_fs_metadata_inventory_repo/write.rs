use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::DbError;

use super::{
    CephFsMetadataInventory, CephFsMetadataInventoryRepoError, CephFsMetadataInventoryRepoResult,
    CephFsMetadataWriteOutcome,
};

pub(super) fn replace(
    conn: &Connection,
    inventory: &CephFsMetadataInventory,
) -> CephFsMetadataInventoryRepoResult<CephFsMetadataWriteOutcome> {
    let transaction = conn.unchecked_transaction().map_err(DbError::from)?;
    validate_source_binding(&transaction, inventory)?;
    if let Some(outcome) = unchanged_or_conflicting(&transaction, inventory)? {
        transaction.commit().map_err(DbError::from)?;
        return Ok(outcome);
    }
    let manifest = &inventory.manifest;
    transaction
        .execute(
            "DELETE FROM ceph_fs_metadata_inventories
             WHERE filesystem_identity = ?1 AND inventory_id = ?2",
            params![manifest.filesystem_identity, manifest.inventory_id],
        )
        .map_err(DbError::from)?;
    insert_manifest(&transaction, inventory)?;
    insert_objects(&transaction, inventory)?;
    validate_persisted_pool_binding(&transaction, inventory)?;
    transaction.commit().map_err(DbError::from)?;
    Ok(CephFsMetadataWriteOutcome::Replaced)
}

fn unchanged_or_conflicting(
    conn: &Connection,
    inventory: &CephFsMetadataInventory,
) -> CephFsMetadataInventoryRepoResult<Option<CephFsMetadataWriteOutcome>> {
    let manifest = &inventory.manifest;
    let existing = conn
        .query_row(
            "SELECT source_semantic_sha256, inventory_sha256
             FROM ceph_fs_metadata_inventories
             WHERE filesystem_identity = ?1 AND inventory_id = ?2",
            params![manifest.filesystem_identity, manifest.inventory_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(DbError::from)?;
    let Some((source_digest, inventory_digest)) = existing else {
        return Ok(None);
    };
    if source_digest != manifest.source_semantic_sha256 {
        return Ok(None);
    }
    if inventory_digest == manifest.inventory_sha256 {
        Ok(Some(CephFsMetadataWriteOutcome::Unchanged))
    } else {
        Err(CephFsMetadataInventoryRepoError::DeterminismConflict)
    }
}

fn validate_source_binding(
    conn: &Connection,
    inventory: &CephFsMetadataInventory,
) -> CephFsMetadataInventoryRepoResult<()> {
    let manifest = &inventory.manifest;
    let binding = conn
        .query_row(
            "SELECT semantic.semantic_sha256, osd.data_source_id
             FROM ceph_bluestore_semantic_scans semantic
             JOIN ceph_osd_inventory osd ON osd.id = semantic.inventory_id
             WHERE semantic.inventory_id = ?1",
            [manifest.inventory_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(DbError::from)?;
    if binding.as_ref().is_none_or(|(semantic, source)| {
        semantic != &manifest.source_semantic_sha256 || source != &manifest.data_source_id
    }) {
        return Err(CephFsMetadataInventoryRepoError::Invalid(
            "manifest is not bound to its BlueStore source snapshot",
        ));
    }
    Ok(())
}

fn insert_manifest(
    conn: &Connection,
    inventory: &CephFsMetadataInventory,
) -> CephFsMetadataInventoryRepoResult<()> {
    let row = &inventory.manifest;
    conn.execute(
        "INSERT INTO ceph_fs_metadata_inventories (
            filesystem_identity, inventory_id, data_source_id, filesystem_id,
            fsmap_epoch, metadata_pool_id, schema_version, classifier_profile,
            source_semantic_sha256, inventory_sha256, object_count,
            unknown_object_count, complete
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            row.filesystem_identity,
            row.inventory_id,
            row.data_source_id,
            row.filesystem_id,
            row.fsmap_epoch,
            row.metadata_pool_id,
            row.schema_version,
            row.classifier_profile,
            row.source_semantic_sha256,
            row.inventory_sha256,
            row.object_count,
            row.unknown_object_count,
            row.complete,
        ],
    )
    .map_err(DbError::from)?;
    Ok(())
}

fn insert_objects(
    conn: &Connection,
    inventory: &CephFsMetadataInventory,
) -> CephFsMetadataInventoryRepoResult<()> {
    let manifest = &inventory.manifest;
    let mut statement = conn
        .prepare_cached(
            "INSERT INTO ceph_fs_metadata_objects (
                filesystem_identity, inventory_id, object_identity_sha256,
                locator, candidate_mask, classification_state, classifier_rule,
                record_sha256
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .map_err(DbError::from)?;
    for object in &inventory.objects {
        statement
            .execute(params![
                manifest.filesystem_identity,
                manifest.inventory_id,
                object.object_identity_sha256,
                object.locator,
                object.candidate_mask,
                object.classification_state,
                object.classifier_rule,
                object.record_sha256,
            ])
            .map_err(DbError::from)?;
    }
    Ok(())
}

fn validate_persisted_pool_binding(
    conn: &Connection,
    inventory: &CephFsMetadataInventory,
) -> CephFsMetadataInventoryRepoResult<()> {
    let manifest = &inventory.manifest;
    let cross_pool_count: u64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM ceph_fs_metadata_objects projection
             JOIN ceph_bluestore_objects object
               ON object.inventory_id = projection.inventory_id
              AND object.object_identity_sha256 = projection.object_identity_sha256
             WHERE projection.filesystem_identity = ?1
               AND projection.inventory_id = ?2
               AND object.decoded_pool <> ?3",
            params![
                manifest.filesystem_identity,
                manifest.inventory_id,
                manifest.metadata_pool_id,
            ],
            |row| row.get(0),
        )
        .map_err(DbError::from)?;
    if cross_pool_count != 0 {
        return Err(CephFsMetadataInventoryRepoError::CrossPoolReference);
    }
    Ok(())
}
