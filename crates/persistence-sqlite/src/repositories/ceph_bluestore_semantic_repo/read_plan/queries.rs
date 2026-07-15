use rusqlite::{params, types::Type, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

use super::super::{
    mapping::{map_blob, map_logical_extent, map_object, map_onode_shard, map_physical_extent},
    query::parse_checksum_value,
    CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestoreLogicalExtentRecord,
    CephBluestoreObjectRecord, CephBluestoreOnodeShardRecord, CephBluestorePhysicalExtentRecord,
    CephBluestoreSharedBlobRecord, CephBluestoreSharedBlobRefRecord,
};
use super::OBJECT_COLUMNS;

pub(super) fn find_object(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Option<CephBluestoreObjectRecord>> {
    let sql = format!(
        "SELECT {OBJECT_COLUMNS}
         FROM ceph_bluestore_objects
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2"
    );
    conn.query_row(
        &sql,
        params![inventory_id, object_identity_sha256],
        map_object,
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn find_object_ordinal(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<u32> {
    let count = conn.query_row(
        "SELECT COUNT(*)
         FROM ceph_bluestore_objects
         WHERE inventory_id = ?1 AND object_identity_sha256 < ?2",
        params![inventory_id, object_identity_sha256],
        |row| row.get::<_, i64>(0),
    )?;
    let count = u64::try_from(count)
        .map_err(|_| DbError::System("BlueStore object ordinal is negative".to_string()))?;
    u32::try_from(count)
        .map_err(|_| DbError::System("BlueStore object ordinal exceeds u32".to_string()))
}

pub(super) fn find_onode_shards(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreOnodeShardRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, object_identity_sha256, shard_ordinal, shard_offset,
                descriptor_bytes, payload_version, declared_extent_count,
                payload_encoded_length, decode_status, deferred_reason,
                logical_extent_count
         FROM ceph_bluestore_onode_shards
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2
         ORDER BY shard_ordinal",
    )?;
    let rows = statement.query_map(
        params![inventory_id, object_identity_sha256],
        map_onode_shard,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_blobs(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreBlobRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, object_identity_sha256, blob_ordinal, blob_kind,
                blob_id_hex, shared_blob_id_hex, logical_length, on_disk_length,
                compressed_length, flags_raw, flag_legacy_mutable, flag_compressed,
                flag_checksum, flag_has_unused, flag_shared, flags_unknown_bits,
                unused_bitmap, checksum_type, checksum_order, checksum_chunk_size,
                checksum_encoded_length, checksum_value_count,
                checksum_data_crc32c, checksum_digest_sha256, use_tracker_kind,
                use_tracker_allocation_unit_size,
                use_tracker_declared_allocation_units, use_tracker_entry_count,
                use_tracker_sha256, logical_extent_count, physical_extent_count
         FROM ceph_bluestore_blobs
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2
         ORDER BY blob_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id, object_identity_sha256], map_blob)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_logical_extents(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreLogicalExtentRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, object_identity_sha256, extent_ordinal,
                logical_offset, length, blob_ordinal, blob_offset, shard_ordinal,
                defines_blob, flags_raw, flag_contiguous, flag_zero_blob_offset,
                flag_same_length, flag_spanning
         FROM ceph_bluestore_logical_extents
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2
         ORDER BY extent_ordinal",
    )?;
    let rows = statement.query_map(
        params![inventory_id, object_identity_sha256],
        map_logical_extent,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_physical_extents(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestorePhysicalExtentRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, object_identity_sha256, blob_ordinal, extent_ordinal,
                blob_offset, device_id, physical_offset_hex, length
         FROM ceph_bluestore_physical_extents
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2
         ORDER BY blob_ordinal, extent_ordinal",
    )?;
    let rows = statement.query_map(
        params![inventory_id, object_identity_sha256],
        map_physical_extent,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_checksum_chunks(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
    object_ordinal: u32,
) -> DbResult<Vec<CephBluestoreChecksumChunkRecord>> {
    let mut statement = conn.prepare(
        "SELECT object_identity_sha256, blob_ordinal,
                checksum_ordinal, chunk_offset, chunk_length, checksum_value_hex
         FROM ceph_bluestore_checksum_chunks
         WHERE inventory_id = ?1 AND object_identity_sha256 = ?2
         ORDER BY blob_ordinal, checksum_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id, object_identity_sha256], |row| {
        if row.get_ref(0)?.as_str()? != object_identity_sha256 {
            return Err(invalid_checksum_binding());
        }
        let (checksum_value, checksum_value_bytes) =
            parse_checksum_value(row.get_ref(5)?.as_str()?)?;
        Ok(CephBluestoreChecksumChunkRecord {
            object_ordinal,
            blob_ordinal: row.get(1)?,
            checksum_ordinal: row.get(2)?,
            chunk_offset: row.get(3)?,
            chunk_length: row.get(4)?,
            checksum_value,
            checksum_value_bytes,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_shared_blobs(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreSharedBlobRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, shared_blob_id_hex, denc_version, decode_status,
                deferred_reason, ref_extent_count, total_ref_bytes, total_refs
         FROM ceph_bluestore_shared_blobs
         WHERE inventory_id = ?1
           AND shared_blob_id_hex IN (
               SELECT shared_blob_id_hex
               FROM ceph_bluestore_blobs
               WHERE inventory_id = ?1
                 AND object_identity_sha256 = ?2
                 AND shared_blob_id_hex IS NOT NULL
           )
         ORDER BY shared_blob_id_hex",
    )?;
    let rows = statement.query_map(params![inventory_id, object_identity_sha256], |row| {
        Ok(CephBluestoreSharedBlobRecord {
            inventory_id: row.get(0)?,
            shared_blob_id_hex: row.get(1)?,
            denc_version: row.get(2)?,
            decode_status: row.get(3)?,
            deferred_reason: row.get(4)?,
            ref_extent_count: row.get(5)?,
            total_ref_bytes: row.get(6)?,
            total_refs: row.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_shared_blob_refs(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CephBluestoreSharedBlobRefRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, shared_blob_id_hex, ref_ordinal, ref_offset_hex,
                length, refs
         FROM ceph_bluestore_shared_blob_refs
         WHERE inventory_id = ?1
           AND shared_blob_id_hex IN (
               SELECT shared_blob_id_hex
               FROM ceph_bluestore_blobs
               WHERE inventory_id = ?1
                 AND object_identity_sha256 = ?2
                 AND shared_blob_id_hex IS NOT NULL
           )
         ORDER BY shared_blob_id_hex, ref_ordinal",
    )?;
    let rows = statement.query_map(params![inventory_id, object_identity_sha256], |row| {
        Ok(CephBluestoreSharedBlobRefRecord {
            inventory_id: row.get(0)?,
            shared_blob_id_hex: row.get(1)?,
            ref_ordinal: row.get(2)?,
            ref_offset_hex: row.get(3)?,
            length: row.get(4)?,
            refs: row.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn invalid_checksum_binding() -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "BlueStore checksum row crosses object identity binding",
        )),
    )
}
