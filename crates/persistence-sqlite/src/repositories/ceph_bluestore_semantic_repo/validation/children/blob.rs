use std::collections::{HashMap, HashSet};

use crate::connection::DbResult;

use super::{
    super::{
        super::{
            CephBluestoreBlobRecord, CephBluestoreObjectRecord, CephBluestoreSharedBlobRecord,
        },
        primitives::{
            fits_sqlite, semantic_error, valid_hex_u64, valid_optional_text, valid_sha256,
        },
    },
    BlobKey,
};

pub(super) fn validate_blobs<'a>(
    inventory_id: &str,
    objects: &HashMap<&str, &CephBluestoreObjectRecord>,
    shared: &HashMap<&str, &CephBluestoreSharedBlobRecord>,
    records: &'a [CephBluestoreBlobRecord],
) -> DbResult<HashMap<BlobKey<'a>, &'a CephBluestoreBlobRecord>> {
    if !records.windows(2).all(|rows| {
        (
            rows[0].object_identity_sha256.as_str(),
            rows[0].blob_ordinal,
        ) < (
            rows[1].object_identity_sha256.as_str(),
            rows[1].blob_ordinal,
        )
    }) {
        return semantic_error("BlueStore blobs are not in canonical order");
    }
    let mut next = HashMap::new();
    let mut ids = HashSet::new();
    let mut indexed = HashMap::new();
    for record in records {
        let object_id = record.object_identity_sha256.as_str();
        if record.inventory_id != inventory_id
            || !objects.contains_key(object_id)
            || !valid_blob(record)
            || record
                .shared_blob_id_hex
                .as_deref()
                .is_some_and(|id| !shared.contains_key(id))
            || !take_ordinal(&mut next, object_id, record.blob_ordinal)
            || !ids.insert((
                object_id,
                record.blob_kind.as_str(),
                record.blob_id_hex.as_str(),
            ))
        {
            return semantic_error("BlueStore blob row is inconsistent");
        }
        indexed.insert((object_id, record.blob_ordinal), record);
    }
    Ok(indexed)
}

fn valid_blob(record: &CephBluestoreBlobRecord) -> bool {
    let counts = [
        record.logical_length,
        record.on_disk_length,
        record.compressed_length.unwrap_or(0),
        record.checksum_chunk_size.unwrap_or(0),
        record.checksum_encoded_length.unwrap_or(0),
        record.checksum_value_count,
        record.use_tracker_allocation_unit_size.unwrap_or(0),
        record.use_tracker_declared_allocation_units.unwrap_or(0),
        record.use_tracker_entry_count,
        record.logical_extent_count,
        record.physical_extent_count,
    ];
    matches!(record.blob_kind.as_str(), "local" | "spanning")
        && valid_hex_u64(&record.blob_id_hex)
        && record
            .shared_blob_id_hex
            .as_deref()
            .is_none_or(valid_hex_u64)
        && counts.into_iter().all(fits_sqlite)
        && valid_blob_flags(record)
        && valid_blob_lengths(record)
        && valid_checksum(record)
        && valid_use_tracker(record)
}

fn valid_blob_flags(record: &CephBluestoreBlobRecord) -> bool {
    record.flag_legacy_mutable == (record.flags_raw & 1 != 0)
        && record.flag_compressed == (record.flags_raw & 2 != 0)
        && record.flag_checksum == (record.flags_raw & 4 != 0)
        && record.flag_has_unused == (record.flags_raw & 8 != 0)
        && record.flag_shared == (record.flags_raw & 16 != 0)
        && record.flags_unknown_bits == record.flags_raw & !0x1f
        && record.flag_has_unused == record.unused_bitmap.is_some()
        && record.flag_shared == record.shared_blob_id_hex.is_some()
}

fn valid_blob_lengths(record: &CephBluestoreBlobRecord) -> bool {
    if record.flag_compressed {
        record.logical_length > 0
            && record
                .compressed_length
                .is_some_and(|length| length > 0 && length <= record.on_disk_length)
    } else {
        record.compressed_length.is_none() && record.logical_length == record.on_disk_length
    }
}

fn valid_checksum(record: &CephBluestoreBlobRecord) -> bool {
    if !record.flag_checksum {
        return record.checksum_type.is_none()
            && record.checksum_order.is_none()
            && record.checksum_chunk_size.is_none()
            && record.checksum_encoded_length.is_none()
            && record.checksum_value_count == 0
            && record.checksum_data_crc32c.is_none()
            && record.checksum_digest_sha256.is_none();
    }
    let chunk_size_matches_order = record
        .checksum_order
        .and_then(|order| 1u64.checked_shl(u32::from(order)))
        == record.checksum_chunk_size;
    let encoded_length_closes = record.checksum_value_count > 0
        && record
            .checksum_encoded_length
            .is_some_and(|length| length % record.checksum_value_count == 0);
    valid_optional_text(record.checksum_type.as_deref())
        && record.checksum_type.is_some()
        && record.checksum_order.is_some()
        && record.checksum_chunk_size.is_some_and(|size| size > 0)
        && chunk_size_matches_order
        && record
            .checksum_encoded_length
            .is_some_and(|length| length > 0)
        && record.checksum_value_count > 0
        && encoded_length_closes
        && record.checksum_data_crc32c.is_some()
        && record
            .checksum_digest_sha256
            .as_deref()
            .is_some_and(valid_sha256)
}

fn valid_use_tracker(record: &CephBluestoreBlobRecord) -> bool {
    let valid_digest = record
        .use_tracker_sha256
        .as_deref()
        .is_some_and(valid_sha256);
    match record.use_tracker_kind.as_deref() {
        None => {
            record.use_tracker_allocation_unit_size.is_none()
                && record.use_tracker_declared_allocation_units.is_none()
                && record.use_tracker_entry_count == 0
                && record.use_tracker_sha256.is_none()
        }
        Some("v1LegacyRefMap") => {
            record.use_tracker_allocation_unit_size.is_none()
                && record.use_tracker_declared_allocation_units.is_none()
                && valid_digest
        }
        Some("v2") => {
            record.use_tracker_allocation_unit_size.is_some()
                && record.use_tracker_declared_allocation_units.is_some()
                && valid_digest
        }
        _ => false,
    }
}

fn take_ordinal<K>(next: &mut HashMap<K, u32>, key: K, ordinal: u32) -> bool
where
    K: Eq + std::hash::Hash,
{
    let expected = next.entry(key).or_default();
    if *expected != ordinal {
        return false;
    }
    let Some(value) = expected.checked_add(1) else {
        return false;
    };
    *expected = value;
    true
}
