mod checks;
mod queries;

use rusqlite::Connection;

use crate::connection::{DbError, DbResult};

use super::{
    mapping, CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord,
    CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord, CephBluestorePhysicalExtentRecord,
};
use checks::{object_validation_aggregate, read_context, validate_plan_rows, ObjectRows};
use queries::{
    find_blobs, find_checksum_chunks, find_logical_extents, find_object, find_object_ordinal,
    find_onode_shards, find_physical_extents, find_shared_blob_refs, find_shared_blobs,
};

const OBJECT_COLUMNS: &str = "
    inventory_id, object_identity_sha256, decoded_shard, decoded_pool,
    decoded_hash, decoded_bitwise_hash, object_namespace, object_key,
    object_name, snap_hex, generation_hex, onode_denc_version, nid, size,
    flags_raw, flag_omap, flag_pgmeta_omap, flag_per_pool_omap,
    flag_per_pg_omap, flags_unknown_bits, attribute_count,
    attribute_value_bytes, attributes_sha256, expected_object_size,
    expected_write_size, allocation_hint_flags, zone_ref_count,
    extent_storage, spanning_blob_version, declared_spanning_blob_count,
    decode_status, deferred_reason, onode_shard_count, blob_count,
    logical_extent_count, physical_extent_count";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreObjectReadPlan {
    pub inventory_id: String,
    pub object_identity_sha256: String,
    pub object_ordinal: u32,
    pub object: CephBluestoreObjectRecord,
    pub blobs: Vec<CephBluestoreBlobRecord>,
    pub logical_extents: Vec<CephBluestoreLogicalExtentRecord>,
    pub physical_extents: Vec<CephBluestorePhysicalExtentRecord>,
    pub checksum_chunks: Vec<CephBluestoreChecksumChunkRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreObjectCandidate {
    pub inventory_id: String,
    pub object_identity_sha256: String,
    pub object_name: Vec<u8>,
    pub decoded_pool: i64,
    pub object_namespace: Vec<u8>,
    pub snap_hex: String,
}

pub(super) struct ReadContext {
    pub(super) scan: super::CephBluestoreSemanticScanRecord,
    pub(super) super_record: super::CephBluestoreSuperRecord,
    pub(super) device_size: u64,
}

pub(super) fn find_object_read_plan(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Option<CephBluestoreObjectReadPlan>> {
    validate_identity_input(object_identity_sha256)?;
    let transaction = conn.unchecked_transaction()?;
    let Some(context) = read_context(&transaction, inventory_id)? else {
        transaction.commit()?;
        return Ok(None);
    };
    let Some(object) = find_object(&transaction, inventory_id, object_identity_sha256)? else {
        transaction.commit()?;
        return Ok(None);
    };
    let object_ordinal = find_object_ordinal(&transaction, inventory_id, object_identity_sha256)?;
    let mut aggregate = object_validation_aggregate(
        &context,
        ObjectRows {
            object: object.clone(),
            onode_shards: find_onode_shards(&transaction, inventory_id, object_identity_sha256)?,
            blobs: find_blobs(&transaction, inventory_id, object_identity_sha256)?,
            logical_extents: find_logical_extents(
                &transaction,
                inventory_id,
                object_identity_sha256,
            )?,
            physical_extents: find_physical_extents(
                &transaction,
                inventory_id,
                object_identity_sha256,
            )?,
            checksum_chunks: find_checksum_chunks(
                &transaction,
                inventory_id,
                object_identity_sha256,
                object_ordinal,
            )?,
            shared_blobs: find_shared_blobs(&transaction, inventory_id, object_identity_sha256)?,
            shared_blob_refs: find_shared_blob_refs(
                &transaction,
                inventory_id,
                object_identity_sha256,
            )?,
        },
    );
    validate_plan_rows(&context, object_ordinal, &object, &mut aggregate)?;
    let plan = CephBluestoreObjectReadPlan {
        inventory_id: inventory_id.to_string(),
        object_identity_sha256: object_identity_sha256.to_string(),
        object_ordinal,
        object,
        blobs: aggregate.blobs,
        logical_extents: aggregate.logical_extents,
        physical_extents: aggregate.physical_extents,
        checksum_chunks: aggregate.checksum_chunks,
    };
    transaction.commit()?;
    Ok(Some(plan))
}

pub(super) fn find_object_candidate(
    conn: &Connection,
    inventory_id: &str,
    object_name: &[u8],
    pool: i64,
    namespace: &[u8],
    snap_hex: &str,
) -> DbResult<Option<CephBluestoreObjectCandidate>> {
    validate_snap_hex(snap_hex)?;
    let transaction = conn.unchecked_transaction()?;
    let Some(_context) = read_context(&transaction, inventory_id)? else {
        transaction.commit()?;
        return Ok(None);
    };
    let sql = format!(
        "SELECT {OBJECT_COLUMNS}
         FROM ceph_bluestore_objects
         WHERE inventory_id = ?1
           AND object_name = ?2
           AND decoded_pool = ?3
           AND object_namespace = ?4
           AND snap_hex = ?5
         ORDER BY object_identity_sha256
         LIMIT 2"
    );
    let mut statement = transaction.prepare(&sql)?;
    let rows = statement.query_map(
        rusqlite::params![inventory_id, object_name, pool, namespace, snap_hex],
        mapping::map_object,
    )?;
    let objects = rows.collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let candidate = match objects.as_slice() {
        [] => None,
        [_first, _second, ..] => {
            return Err(DbError::System(
                "BlueStore object lookup is ambiguous".to_string(),
            ))
        }
        [object] => {
            super::validation::validate_object_for_read(inventory_id, object)?;
            if object.object_name != object_name
                || object.decoded_pool != pool
                || object.object_namespace != namespace
                || object.snap_hex != snap_hex
            {
                return Err(DbError::System(
                    "BlueStore object lookup binding is inconsistent".to_string(),
                ));
            }
            Some(CephBluestoreObjectCandidate {
                inventory_id: object.inventory_id.clone(),
                object_identity_sha256: object.object_identity_sha256.clone(),
                object_name: object.object_name.clone(),
                decoded_pool: object.decoded_pool,
                object_namespace: object.object_namespace.clone(),
                snap_hex: object.snap_hex.clone(),
            })
        }
    };
    transaction.commit()?;
    Ok(candidate)
}

fn validate_snap_hex(value: &str) -> DbResult<()> {
    if value.len() == 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(DbError::System(
            "BlueStore object snap id is not canonical hex".to_string(),
        ))
    }
}

fn validate_identity_input(value: &str) -> DbResult<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(DbError::System(
            "BlueStore object identity must be canonical lowercase SHA-256".to_string(),
        ))
    }
}
