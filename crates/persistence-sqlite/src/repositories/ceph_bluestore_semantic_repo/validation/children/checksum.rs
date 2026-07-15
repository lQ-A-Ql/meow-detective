use std::{cmp::Ordering, collections::HashMap};

use crate::connection::{DbError, DbResult};

use super::{
    super::{
        super::{CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord},
        primitives::{fits_sqlite, semantic_error},
    },
    BlobKey,
};

pub(super) fn validate_checksum_chunks<'a>(
    inventory_id: &str,
    blobs: &'a [CephBluestoreBlobRecord],
    records: &[CephBluestoreChecksumChunkRecord],
) -> DbResult<HashMap<BlobKey<'a>, u64>> {
    ensure_order(records)?;
    let mut counts = HashMap::with_capacity(blobs.len());
    let mut record_index = 0usize;
    for blob in blobs {
        let count = validate_blob_checksum(inventory_id, blob, records, &mut record_index)?;
        if count > 0 {
            counts.insert(
                (blob.object_identity_sha256.as_str(), blob.blob_ordinal),
                count,
            );
        }
    }
    if record_index != records.len() {
        return semantic_error("BlueStore checksum chunk references an unknown blob");
    }
    Ok(counts)
}

fn validate_blob_checksum(
    inventory_id: &str,
    blob: &CephBluestoreBlobRecord,
    records: &[CephBluestoreChecksumChunkRecord],
    record_index: &mut usize,
) -> DbResult<u64> {
    let value_bytes = checksum_value_bytes(blob);
    let mut next_ordinal = 0u32;
    let mut chunk_end = 0u64;
    while let Some(record) = records.get(*record_index) {
        match compare_record_to_blob(record, blob) {
            Ordering::Less => {
                return semantic_error("BlueStore checksum chunk references an unknown blob")
            }
            Ordering::Greater => break,
            Ordering::Equal => {}
        }
        let expected_length = blob.checksum_chunk_size.and_then(|size| {
            blob.on_disk_length
                .checked_sub(record.chunk_offset)
                .map(|left| left.min(size))
        });
        let end = record.chunk_offset.checked_add(record.chunk_length);
        if record.inventory_id.as_ref() != inventory_id
            || !blob.flag_checksum
            || record.checksum_ordinal != next_ordinal
            || record.chunk_offset != chunk_end
            || record.chunk_length == 0
            || !fits_sqlite(record.chunk_offset)
            || !fits_sqlite(record.chunk_length)
            || end.is_none_or(|end| end > blob.on_disk_length)
            || expected_length != Some(record.chunk_length)
            || value_bytes
                .is_none_or(|width| record.checksum_value_hex.len() != width.saturating_mul(2))
            || !valid_lower_hex(&record.checksum_value_hex)
        {
            return semantic_error("BlueStore checksum chunk row is inconsistent");
        }
        next_ordinal = next_ordinal
            .checked_add(1)
            .ok_or_else(|| DbError::System("BlueStore checksum ordinal overflow".to_string()))?;
        chunk_end = end.unwrap_or_default();
        *record_index += 1;
    }
    if u64::from(next_ordinal) != blob.checksum_value_count
        || (blob.flag_checksum && chunk_end != blob.on_disk_length)
        || (!blob.flag_checksum && chunk_end != 0)
    {
        return semantic_error("BlueStore checksum chunks do not close the blob checksum");
    }
    Ok(u64::from(next_ordinal))
}

fn compare_record_to_blob(
    record: &CephBluestoreChecksumChunkRecord,
    blob: &CephBluestoreBlobRecord,
) -> Ordering {
    (record.object_identity_sha256.as_ref(), record.blob_ordinal)
        .cmp(&(blob.object_identity_sha256.as_str(), blob.blob_ordinal))
}

fn ensure_order(records: &[CephBluestoreChecksumChunkRecord]) -> DbResult<()> {
    if records.windows(2).all(|rows| {
        (
            rows[0].object_identity_sha256.as_ref(),
            rows[0].blob_ordinal,
            rows[0].checksum_ordinal,
        ) < (
            rows[1].object_identity_sha256.as_ref(),
            rows[1].blob_ordinal,
            rows[1].checksum_ordinal,
        )
    }) {
        Ok(())
    } else {
        semantic_error("BlueStore checksum chunks are not in canonical order")
    }
}

fn checksum_value_bytes(blob: &CephBluestoreBlobRecord) -> Option<usize> {
    let encoded = blob.checksum_encoded_length?;
    let count = blob.checksum_value_count;
    if count == 0 || encoded == 0 || encoded % count != 0 {
        return None;
    }
    usize::try_from(encoded / count)
        .ok()
        .filter(|width| *width > 0)
}

fn valid_lower_hex(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
