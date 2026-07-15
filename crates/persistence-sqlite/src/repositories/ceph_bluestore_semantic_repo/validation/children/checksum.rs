use std::collections::HashMap;

use crate::connection::{DbError, DbResult};

use super::{
    super::{
        super::{
            CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestoreObjectRecord,
        },
        primitives::{fits_sqlite, semantic_error},
    },
    BlobKey,
};

pub(super) fn validate_checksum_chunks<'a>(
    objects: &[CephBluestoreObjectRecord],
    blobs: &'a [CephBluestoreBlobRecord],
    records: &[CephBluestoreChecksumChunkRecord],
) -> DbResult<HashMap<BlobKey<'a>, u64>> {
    let object_ordinals = objects
        .iter()
        .enumerate()
        .map(|(ordinal, record)| {
            u32::try_from(ordinal)
                .map(|ordinal| (record.object_identity_sha256.as_str(), ordinal))
                .map_err(|_| DbError::System("BlueStore object ordinal exceeds u32".to_string()))
        })
        .collect::<DbResult<HashMap<_, _>>>()?;
    let mut counts = HashMap::with_capacity(blobs.len());
    let mut record_index = 0usize;
    for blob in blobs {
        let object_ordinal = object_ordinals
            .get(blob.object_identity_sha256.as_str())
            .copied()
            .ok_or_else(|| {
                DbError::System("BlueStore blob references an unknown object".to_string())
            })?;
        let count = validate_blob_checksum(object_ordinal, blob, records, &mut record_index)?;
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
    object_ordinal: u32,
    blob: &CephBluestoreBlobRecord,
    records: &[CephBluestoreChecksumChunkRecord],
    record_index: &mut usize,
) -> DbResult<u64> {
    let value_bytes = checksum_value_bytes(blob);
    let expected_count = usize::try_from(blob.checksum_value_count)
        .map_err(|_| DbError::System("BlueStore checksum count exceeds usize".to_string()))?;
    let record_end = record_index
        .checked_add(expected_count)
        .ok_or_else(|| DbError::System("BlueStore checksum row range overflow".to_string()))?;
    let blob_records = records
        .get(*record_index..record_end)
        .ok_or_else(|| semantic_error_value("BlueStore checksum chunks do not close the blob"))?;
    let mut chunk_end = 0u64;
    for (ordinal, record) in blob_records.iter().enumerate() {
        let expected_ordinal = u32::try_from(ordinal)
            .map_err(|_| DbError::System("BlueStore checksum ordinal exceeds u32".to_string()))?;
        let expected_length = blob.checksum_chunk_size.and_then(|size| {
            blob.on_disk_length
                .checked_sub(record.chunk_offset)
                .map(|left| left.min(size))
        });
        let expected_value_bytes = value_bytes.and_then(|width| u8::try_from(width).ok());
        let end = record.chunk_offset.checked_add(record.chunk_length);
        if !blob.flag_checksum
            || record.object_ordinal != object_ordinal
            || record.blob_ordinal != blob.blob_ordinal
            || record.checksum_ordinal != expected_ordinal
            || record.chunk_offset != chunk_end
            || record.chunk_length == 0
            || !fits_sqlite(record.chunk_offset)
            || !fits_sqlite(record.chunk_length)
            || end.is_none_or(|end| end > blob.on_disk_length)
            || expected_length != Some(record.chunk_length)
            || expected_value_bytes != Some(record.checksum_value_bytes)
            || !checksum_value_fits(record.checksum_value, record.checksum_value_bytes)
        {
            return semantic_error("BlueStore checksum chunk row is inconsistent");
        }
        chunk_end = end.unwrap_or_default();
    }
    *record_index = record_end;
    if (blob.flag_checksum && chunk_end != blob.on_disk_length)
        || (!blob.flag_checksum && chunk_end != 0)
    {
        return semantic_error("BlueStore checksum chunks do not close the blob checksum");
    }
    Ok(blob.checksum_value_count)
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

fn checksum_value_fits(value: u64, width: u8) -> bool {
    match width {
        1..=7 => value < (1u64 << (u32::from(width) * 8)),
        8 => true,
        _ => false,
    }
}

fn semantic_error_value(message: &str) -> DbError {
    DbError::System(message.to_string())
}
