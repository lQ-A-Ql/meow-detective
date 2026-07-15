use std::collections::BTreeMap;
use std::sync::Arc;

use ceph_wire::{BlueStoreBlob, BlueStoreBlobIdentity, BlueStoreBlobUseTracker};
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestorePhysicalExtentRecord,
};
use transport::CommandError;

use super::{
    digest::{checksum_type_name, checksum_word_size, use_tracker_sha256},
    object::FinalizedObjects,
    object_rows::PayloadRef,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PersistedBlobKey {
    Spanning(u64),
    Local { scope: u32, id: u32 },
}

pub(super) fn write_blobs(
    inventory_id: &str,
    object_id: &str,
    spanning: &[BlueStoreBlob],
    payloads: &[PayloadRef],
    device_size: u64,
    result: &mut FinalizedObjects,
) -> Result<BTreeMap<PersistedBlobKey, u32>, CommandError> {
    let mut inputs = spanning
        .iter()
        .map(|blob| (PersistedBlobKey::Spanning(spanning_id(blob)), blob))
        .collect::<Vec<_>>();
    for payload in payloads {
        let scope = payload_scope(payload);
        inputs.extend(payload.payload.blobs.iter().map(|blob| {
            (
                PersistedBlobKey::Local {
                    scope,
                    id: local_id(blob),
                },
                blob,
            )
        }));
    }
    let mut map = BTreeMap::new();
    for (ordinal, (key, blob)) in inputs.into_iter().enumerate() {
        let ordinal = u32::try_from(ordinal).map_err(|_| blob_error("blob ordinal exceeds u32"))?;
        if map.insert(key, ordinal).is_some() {
            return Err(blob_error("duplicate persisted blob identity"));
        }
        result
            .blobs
            .push(blob_row(inventory_id, object_id, ordinal, key, blob));
        write_checksum_chunks(inventory_id, object_id, ordinal, blob, result)?;
        write_physical_extents(inventory_id, object_id, ordinal, blob, device_size, result)?;
    }
    Ok(map)
}

fn blob_row(
    inventory_id: &str,
    object_id: &str,
    ordinal: u32,
    key: PersistedBlobKey,
    blob: &BlueStoreBlob,
) -> CephBluestoreBlobRecord {
    let (
        checksum_type,
        checksum_order,
        checksum_chunk_size,
        checksum_encoded_length,
        checksum_value_count,
        checksum_data_crc32c,
        checksum_digest_sha256,
    ) = blob
        .checksum
        .map_or((None, None, None, None, 0, None, None), |checksum| {
            let word_size = checksum_word_size(checksum.checksum_type);
            (
                Some(checksum_type_name(checksum.checksum_type).to_string()),
                Some(checksum.chunk_order),
                Some(1u64 << checksum.chunk_order),
                Some(checksum.encoded_length as u64),
                checksum.encoded_length as u64 / word_size,
                Some(checksum.data_ceph_crc32c),
                Some(hex::encode(checksum.data_sha256)),
            )
        });
    let (tracker_kind, tracker_au, tracker_units, tracker_count, tracker_digest) =
        tracker_fields(blob.use_tracker.as_ref());
    CephBluestoreBlobRecord {
        inventory_id: inventory_id.to_string(),
        object_identity_sha256: object_id.to_string(),
        blob_ordinal: ordinal,
        blob_kind: match key {
            PersistedBlobKey::Spanning(_) => "spanning",
            PersistedBlobKey::Local { .. } => "local",
        }
        .to_string(),
        blob_id_hex: persisted_blob_id(key),
        shared_blob_id_hex: blob.shared_blob_id.map(|id| format!("{id:016x}")),
        logical_length: u64::from(blob.logical_length),
        on_disk_length: u64::from(blob.on_disk_length),
        compressed_length: blob.compressed_length.map(u64::from),
        flags_raw: blob.flags.raw,
        flag_legacy_mutable: blob.flags.legacy_mutable,
        flag_compressed: blob.flags.compressed,
        flag_checksum: blob.flags.checksum,
        flag_has_unused: blob.flags.has_unused,
        flag_shared: blob.flags.shared,
        flags_unknown_bits: blob.flags.unknown_bits,
        unused_bitmap: blob.unused_bitmap,
        checksum_type,
        checksum_order,
        checksum_chunk_size,
        checksum_encoded_length,
        checksum_value_count,
        checksum_data_crc32c,
        checksum_digest_sha256,
        use_tracker_kind: tracker_kind,
        use_tracker_allocation_unit_size: tracker_au,
        use_tracker_declared_allocation_units: tracker_units,
        use_tracker_entry_count: tracker_count,
        use_tracker_sha256: tracker_digest,
        logical_extent_count: 0,
        physical_extent_count: blob.physical_extents.len() as u64,
    }
}

fn write_checksum_chunks(
    inventory_id: &str,
    object_id: &str,
    blob_ordinal: u32,
    blob: &BlueStoreBlob,
    result: &mut FinalizedObjects,
) -> Result<(), CommandError> {
    let Some(checksum) = blob.checksum else {
        if !blob.checksum_words.is_empty() {
            return Err(blob_error("checksum words exist without checksum metadata"));
        }
        return Ok(());
    };
    let chunk_size = 1u64
        .checked_shl(u32::from(checksum.chunk_order))
        .ok_or_else(|| blob_error("checksum chunk order exceeds u64"))?;
    let word_width = usize::try_from(checksum_word_size(checksum.checksum_type))
        .map_err(|_| blob_error("checksum word width exceeds usize"))?
        .checked_mul(2)
        .ok_or_else(|| blob_error("checksum word width overflow"))?;
    let shared_inventory_id = Arc::<str>::from(inventory_id);
    let shared_object_id = Arc::<str>::from(object_id);
    for (ordinal, value) in blob.checksum_words.iter().copied().enumerate() {
        let checksum_ordinal =
            u32::try_from(ordinal).map_err(|_| blob_error("checksum ordinal exceeds u32"))?;
        let chunk_offset = u64::from(checksum_ordinal)
            .checked_mul(chunk_size)
            .ok_or_else(|| blob_error("checksum chunk offset overflow"))?;
        result
            .checksum_chunks
            .push(CephBluestoreChecksumChunkRecord {
                inventory_id: Arc::clone(&shared_inventory_id),
                object_identity_sha256: Arc::clone(&shared_object_id),
                blob_ordinal,
                checksum_ordinal,
                chunk_offset,
                chunk_length: chunk_size,
                checksum_value_hex: format!("{value:0word_width$x}").into_boxed_str(),
            });
    }
    Ok(())
}

fn tracker_fields(
    tracker: Option<&BlueStoreBlobUseTracker>,
) -> (
    Option<String>,
    Option<u64>,
    Option<u64>,
    u64,
    Option<String>,
) {
    match tracker {
        None => (None, None, None, 0, None),
        Some(value @ BlueStoreBlobUseTracker::V1LegacyRefMap { entries }) => (
            Some("v1LegacyRefMap".to_string()),
            None,
            None,
            entries.len() as u64,
            Some(use_tracker_sha256(value)),
        ),
        Some(
            value @ BlueStoreBlobUseTracker::V2 {
                allocation_unit_size,
                declared_allocation_units,
                referenced_bytes,
            },
        ) => (
            Some("v2".to_string()),
            Some(u64::from(*allocation_unit_size)),
            Some(u64::from(*declared_allocation_units)),
            referenced_bytes.len() as u64,
            Some(use_tracker_sha256(value)),
        ),
    }
}

fn write_physical_extents(
    inventory_id: &str,
    object_id: &str,
    blob_ordinal: u32,
    blob: &BlueStoreBlob,
    device_size: u64,
    result: &mut FinalizedObjects,
) -> Result<(), CommandError> {
    let mut blob_offset = 0u64;
    for (ordinal, extent) in blob.physical_extents.iter().enumerate() {
        if extent.offset.is_some_and(|offset| {
            offset
                .checked_add(u64::from(extent.length))
                .is_none_or(|end| end > device_size)
        }) {
            return Err(blob_error(
                "physical extent lies outside the selected BlueStore device",
            ));
        }
        result
            .physical_extents
            .push(CephBluestorePhysicalExtentRecord {
                inventory_id: inventory_id.to_string(),
                object_identity_sha256: object_id.to_string(),
                blob_ordinal,
                extent_ordinal: ordinal as u32,
                blob_offset,
                device_id: 1,
                physical_offset_hex: extent.offset.map(|value| format!("{value:016x}")),
                length: u64::from(extent.length),
            });
        blob_offset = blob_offset
            .checked_add(u64::from(extent.length))
            .ok_or_else(|| blob_error("physical extent blob offset overflow"))?;
    }
    Ok(())
}

pub(super) fn payload_scope(payload: &PayloadRef) -> u32 {
    payload.shard.map_or(0, |(ordinal, _, _)| ordinal + 1)
}

fn spanning_id(blob: &BlueStoreBlob) -> u64 {
    match blob.identity {
        BlueStoreBlobIdentity::Spanning(id) => id,
        BlueStoreBlobIdentity::Local(_) => 0,
    }
}

fn local_id(blob: &BlueStoreBlob) -> u32 {
    match blob.identity {
        BlueStoreBlobIdentity::Local(id) => id,
        BlueStoreBlobIdentity::Spanning(_) => 0,
    }
}

fn persisted_blob_id(key: PersistedBlobKey) -> String {
    match key {
        PersistedBlobKey::Spanning(id) => format!("{id:016x}"),
        PersistedBlobKey::Local { scope, id } => {
            format!("{:016x}", (u64::from(scope) << 32) | u64::from(id))
        }
    }
}

fn blob_error(message: impl Into<String>) -> CommandError {
    CommandError::parser(format!("BlueStore blob closure failed: {}", message.into()))
}

#[cfg(test)]
#[path = "../../../tests/unit/import_pipeline/ceph_bluestore_semantic_blob_rows.rs"]
mod tests;
