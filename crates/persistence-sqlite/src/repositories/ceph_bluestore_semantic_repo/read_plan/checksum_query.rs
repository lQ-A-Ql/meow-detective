use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::connection::{DbError, DbResult};

use super::super::{
    query::parse_checksum_value, CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord,
};

struct CompactChecksumMap {
    blob_ordinal: u32,
    count: u64,
    first_ordinal: u64,
    last_ordinal: u64,
    total_length: u64,
    first_offset: u64,
    final_end: u64,
    offset_mismatches: u64,
    length_mismatches: u64,
    value_hex_width: usize,
    packed_values: String,
}

pub(super) fn find_checksum_chunks_compact(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
    object_ordinal: u32,
    blobs: &[CephBluestoreBlobRecord],
) -> DbResult<Vec<CephBluestoreChecksumChunkRecord>> {
    let maps = load_compact_maps(conn, inventory_id, object_identity_sha256)?;
    let blobs_by_ordinal = blobs
        .iter()
        .map(|blob| (blob.blob_ordinal, blob))
        .collect::<HashMap<_, _>>();
    let expected_map_count = blobs
        .iter()
        .filter(|blob| blob.checksum_value_count > 0)
        .count();
    if maps.len() != expected_map_count {
        return invalid("BlueStore checksum maps do not match blob declarations");
    }
    let total_count = maps.iter().try_fold(0usize, |total, map| {
        usize::try_from(map.count)
            .ok()
            .and_then(|count| total.checked_add(count))
            .ok_or_else(|| DbError::System("BlueStore checksum count exceeds memory".to_string()))
    })?;
    let mut chunks = Vec::with_capacity(total_count);
    for map in maps {
        let blob = blobs_by_ordinal
            .get(&map.blob_ordinal)
            .copied()
            .ok_or_else(|| {
                DbError::System("BlueStore checksum map references a missing blob".to_string())
            })?;
        append_map_chunks(&mut chunks, object_ordinal, blob, map)?;
    }
    Ok(chunks)
}

fn load_compact_maps(
    conn: &Connection,
    inventory_id: &str,
    object_identity_sha256: &str,
) -> DbResult<Vec<CompactChecksumMap>> {
    let mut statement = conn.prepare_cached(
        "SELECT c.blob_ordinal,
                COUNT(*),
                MIN(c.checksum_ordinal),
                MAX(c.checksum_ordinal),
                SUM(c.chunk_length),
                MIN(c.chunk_offset),
                MAX(c.chunk_offset + c.chunk_length),
                SUM(CASE
                    WHEN c.chunk_offset <> c.checksum_ordinal * b.checksum_chunk_size
                    THEN 1 ELSE 0 END),
                SUM(CASE
                    WHEN c.chunk_length <> MIN(
                        b.checksum_chunk_size,
                        b.logical_length - c.chunk_offset
                    )
                    THEN 1 ELSE 0 END),
                MIN(length(c.checksum_value_hex)),
                MAX(length(c.checksum_value_hex)),
                group_concat(c.checksum_value_hex, '' ORDER BY c.checksum_ordinal)
         FROM ceph_bluestore_checksum_chunks AS c
         JOIN ceph_bluestore_blobs AS b
           ON b.inventory_id = c.inventory_id
          AND b.object_identity_sha256 = c.object_identity_sha256
          AND b.blob_ordinal = c.blob_ordinal
         WHERE c.inventory_id = ?1
           AND c.object_identity_sha256 = ?2
         GROUP BY c.blob_ordinal
         ORDER BY c.blob_ordinal",
    )?;
    let rows = statement.query_map(
        params![inventory_id, object_identity_sha256],
        read_compact_map,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn read_compact_map(row: &rusqlite::Row<'_>) -> rusqlite::Result<CompactChecksumMap> {
    let min_width: usize = row.get(9)?;
    let max_width: usize = row.get(10)?;
    if min_width != max_width {
        return Err(invalid_row("checksum values use inconsistent widths"));
    }
    Ok(CompactChecksumMap {
        blob_ordinal: row.get(0)?,
        count: row.get(1)?,
        first_ordinal: row.get(2)?,
        last_ordinal: row.get(3)?,
        total_length: row.get(4)?,
        first_offset: row.get(5)?,
        final_end: row.get(6)?,
        offset_mismatches: row.get(7)?,
        length_mismatches: row.get(8)?,
        value_hex_width: min_width,
        packed_values: row.get(11)?,
    })
}

fn append_map_chunks(
    output: &mut Vec<CephBluestoreChecksumChunkRecord>,
    object_ordinal: u32,
    blob: &CephBluestoreBlobRecord,
    map: CompactChecksumMap,
) -> DbResult<()> {
    validate_map(blob, &map)?;
    let chunk_size = blob
        .checksum_chunk_size
        .ok_or_else(|| DbError::System("BlueStore checksum chunk size is missing".to_string()))?;
    let count = usize::try_from(map.count)
        .map_err(|_| DbError::System("BlueStore checksum count exceeds memory".to_string()))?;
    for checksum_ordinal in 0..count {
        let value_start = checksum_ordinal
            .checked_mul(map.value_hex_width)
            .ok_or_else(|| DbError::System("BlueStore checksum offset overflow".to_string()))?;
        let value_end = value_start
            .checked_add(map.value_hex_width)
            .ok_or_else(|| DbError::System("BlueStore checksum end overflow".to_string()))?;
        let value_hex = map
            .packed_values
            .get(value_start..value_end)
            .ok_or_else(|| DbError::System("BlueStore checksum map is truncated".to_string()))?;
        let (checksum_value, checksum_value_bytes) =
            parse_checksum_value(value_hex).map_err(DbError::from)?;
        let checksum_ordinal = u32::try_from(checksum_ordinal)
            .map_err(|_| DbError::System("BlueStore checksum ordinal overflow".to_string()))?;
        let chunk_offset = u64::from(checksum_ordinal)
            .checked_mul(chunk_size)
            .ok_or_else(|| DbError::System("BlueStore checksum range overflow".to_string()))?;
        let remaining = blob
            .logical_length
            .checked_sub(chunk_offset)
            .ok_or_else(|| {
                DbError::System("BlueStore checksum range exceeds blob length".to_string())
            })?;
        let chunk_length = chunk_size.min(remaining);
        output.push(CephBluestoreChecksumChunkRecord {
            object_ordinal,
            blob_ordinal: blob.blob_ordinal,
            checksum_ordinal,
            chunk_offset,
            chunk_length,
            checksum_value,
            checksum_value_bytes,
        });
    }
    Ok(())
}

fn validate_map(blob: &CephBluestoreBlobRecord, map: &CompactChecksumMap) -> DbResult<()> {
    let expected_last = map
        .count
        .checked_sub(1)
        .ok_or_else(|| DbError::System("BlueStore checksum map cannot be empty".to_string()))?;
    let expected_packed_length = usize::try_from(map.count)
        .ok()
        .and_then(|count| count.checked_mul(map.value_hex_width))
        .ok_or_else(|| DbError::System("BlueStore checksum map length overflow".to_string()))?;
    if map.count != blob.checksum_value_count
        || map.first_ordinal != 0
        || map.last_ordinal != expected_last
        || map.total_length != blob.logical_length
        || map.first_offset != 0
        || map.final_end != blob.logical_length
        || map.offset_mismatches != 0
        || map.length_mismatches != 0
        || !(2..=16).contains(&map.value_hex_width)
        || !map.value_hex_width.is_multiple_of(2)
        || map.packed_values.len() != expected_packed_length
    {
        return invalid("BlueStore checksum map is not canonical");
    }
    Ok(())
}

fn invalid<T>(message: &str) -> DbResult<T> {
    Err(DbError::System(message.to_string()))
}

fn invalid_row(message: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        9,
        rusqlite::types::Type::Integer,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}
