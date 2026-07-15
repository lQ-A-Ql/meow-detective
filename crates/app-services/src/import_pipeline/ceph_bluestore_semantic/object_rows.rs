use std::collections::BTreeMap;

use ceph_wire::{
    BlueStoreBlobIdentity, BlueStoreExtentPayload, BlueStoreExtentStorage, BlueStoreObjectId,
    BlueStoreOnodeHeader, BlueStoreOnodeTail,
};
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    object_identity_sha256, CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord,
    CephBluestoreOnodeShardRecord,
};
use transport::CommandError;

use super::{
    blob_rows::{payload_scope, write_blobs, PersistedBlobKey},
    digest::attributes_sha256,
    object::FinalizedObjects,
};

pub(super) struct PayloadRef {
    pub(super) shard: Option<(u32, u32, u32)>,
    pub(super) payload: BlueStoreExtentPayload,
}

pub(super) fn finish_object(
    inventory_id: &str,
    object_id: BlueStoreObjectId,
    onode: BlueStoreOnodeHeader,
    tail: BlueStoreOnodeTail,
    shards: BTreeMap<u32, BlueStoreExtentPayload>,
    device_size: u64,
    result: &mut FinalizedObjects,
) -> Result<(), CommandError> {
    let BlueStoreOnodeTail::Decoded {
        spanning_blob_version,
        spanning_blobs,
        extents,
    } = tail;
    let payloads = collect_payloads(&onode, extents, shards)?;
    let mut object = base_object_row(
        inventory_id,
        &object_id,
        &onode,
        spanning_blob_version,
        spanning_blobs.len(),
        &payloads,
    )?;
    object.object_identity_sha256 = object_identity_sha256(&object);
    let identity = object.object_identity_sha256.clone();
    let blob_map = write_blobs(
        inventory_id,
        &identity,
        &spanning_blobs,
        &payloads,
        device_size,
        result,
    )?;
    write_shards_and_extents(
        inventory_id,
        &identity,
        &onode,
        &payloads,
        &blob_map,
        object.size,
        result,
    )?;
    object.onode_shard_count = payloads.iter().filter(|item| item.shard.is_some()).count() as u64;
    object.blob_count = count_rows(&result.blobs, &identity, |row| {
        row.object_identity_sha256.as_str()
    });
    object.logical_extent_count = count_rows(&result.logical_extents, &identity, |row| {
        row.object_identity_sha256.as_str()
    });
    object.physical_extent_count = count_rows(&result.physical_extents, &identity, |row| {
        row.object_identity_sha256.as_str()
    });
    result.objects.push(object);
    Ok(())
}

fn collect_payloads(
    onode: &BlueStoreOnodeHeader,
    extents: BlueStoreExtentStorage,
    mut shards: BTreeMap<u32, BlueStoreExtentPayload>,
) -> Result<Vec<PayloadRef>, CommandError> {
    match extents {
        BlueStoreExtentStorage::Inline(payload) => {
            if !onode.extent_shards.is_empty() || !shards.is_empty() {
                return Err(row_error(
                    "inline extent map is accompanied by shard metadata",
                ));
            }
            Ok(vec![PayloadRef {
                shard: None,
                payload,
            }])
        }
        BlueStoreExtentStorage::Sharded => {
            if onode.extent_shards.len() != shards.len() {
                return Err(row_error("object extent-shard descriptors do not close"));
            }
            onode
                .extent_shards
                .iter()
                .enumerate()
                .map(|(ordinal, descriptor)| {
                    let payload = shards
                        .remove(&descriptor.offset)
                        .ok_or_else(|| row_error("object is missing a declared extent shard"))?;
                    if payload.encoded_length > descriptor.bytes as usize {
                        return Err(row_error(
                            "extent-shard payload exceeds its descriptor byte count",
                        ));
                    }
                    Ok(PayloadRef {
                        shard: Some((ordinal as u32, descriptor.offset, descriptor.bytes)),
                        payload,
                    })
                })
                .collect()
        }
    }
}

fn base_object_row(
    inventory_id: &str,
    object: &BlueStoreObjectId,
    onode: &BlueStoreOnodeHeader,
    spanning_blob_version: u8,
    spanning_count: usize,
    payloads: &[PayloadRef],
) -> Result<CephBluestoreObjectRecord, CommandError> {
    let attribute_value_bytes = onode
        .attributes
        .iter()
        .try_fold(0u64, |total, attribute| {
            total.checked_add(u64::from(attribute.value_length))
        })
        .ok_or_else(|| row_error("attribute byte count overflow"))?;
    Ok(CephBluestoreObjectRecord {
        inventory_id: inventory_id.to_string(),
        object_identity_sha256: String::new(),
        decoded_shard: object.shard,
        decoded_pool: object.pool,
        decoded_hash: object.hash,
        decoded_bitwise_hash: object.bitwise_hash,
        object_namespace: object.namespace.clone(),
        object_key: object.object_key.clone(),
        object_name: object.object_name.clone(),
        snap_hex: format!("{:016x}", object.snap),
        generation_hex: format!("{:016x}", object.generation),
        onode_denc_version: onode.denc_version,
        nid: onode.nid,
        size: onode.size,
        flags_raw: onode.flags.raw,
        flag_omap: onode.flags.omap,
        flag_pgmeta_omap: onode.flags.pgmeta_omap,
        flag_per_pool_omap: onode.flags.per_pool_omap,
        flag_per_pg_omap: onode.flags.per_pg_omap,
        flags_unknown_bits: onode.flags.unknown_bits,
        attribute_count: onode.attributes.len() as u64,
        attribute_value_bytes,
        attributes_sha256: attributes_sha256(&onode.attributes),
        expected_object_size: u64::from(onode.allocation_hints.expected_object_size),
        expected_write_size: u64::from(onode.allocation_hints.expected_write_size),
        allocation_hint_flags: onode.allocation_hints.flags,
        zone_ref_count: onode.zone_offset_refs.len() as u64,
        extent_storage: if payloads.iter().any(|item| item.shard.is_some()) {
            "sharded".to_string()
        } else {
            "inline".to_string()
        },
        spanning_blob_version,
        declared_spanning_blob_count: spanning_count as u64,
        decode_status: "parsed".to_string(),
        deferred_reason: None,
        onode_shard_count: 0,
        blob_count: 0,
        logical_extent_count: 0,
        physical_extent_count: 0,
    })
}

fn write_shards_and_extents(
    inventory_id: &str,
    object_id: &str,
    onode: &BlueStoreOnodeHeader,
    payloads: &[PayloadRef],
    blobs: &BTreeMap<PersistedBlobKey, u32>,
    object_size: u64,
    result: &mut FinalizedObjects,
) -> Result<(), CommandError> {
    let mut extents = Vec::new();
    for payload in payloads {
        write_shard(inventory_id, object_id, payload, result);
        let scope = payload_scope(payload);
        for extent in &payload.payload.extents {
            let key = match extent.blob {
                BlueStoreBlobIdentity::Spanning(id) => PersistedBlobKey::Spanning(id),
                BlueStoreBlobIdentity::Local(id) => PersistedBlobKey::Local { scope, id },
            };
            let blob_ordinal = blobs
                .get(&key)
                .copied()
                .ok_or_else(|| row_error("logical extent references an unknown blob"))?;
            extents.push((extent, payload.shard.map(|value| value.0), blob_ordinal));
        }
    }
    extents.sort_by_key(|(extent, _, _)| extent.logical_offset);
    write_logical_extents(inventory_id, object_id, extents, object_size, result)?;
    update_blob_logical_counts(object_id, result);
    let has_shards = result
        .onode_shards
        .iter()
        .any(|row| row.object_identity_sha256 == object_id);
    if onode.extent_shards.is_empty() == has_shards {
        return Err(row_error("object shard rows do not match onode storage"));
    }
    Ok(())
}

fn write_shard(
    inventory_id: &str,
    object_id: &str,
    payload: &PayloadRef,
    result: &mut FinalizedObjects,
) {
    if let Some((ordinal, offset, bytes)) = payload.shard {
        result.onode_shards.push(CephBluestoreOnodeShardRecord {
            inventory_id: inventory_id.to_string(),
            object_identity_sha256: object_id.to_string(),
            shard_ordinal: ordinal,
            shard_offset: offset,
            descriptor_bytes: bytes,
            payload_version: Some(payload.payload.version),
            declared_extent_count: Some(u64::from(payload.payload.declared_extent_count)),
            payload_encoded_length: Some(payload.payload.encoded_length as u64),
            decode_status: "parsed".to_string(),
            deferred_reason: None,
            logical_extent_count: payload.payload.extents.len() as u64,
        });
    }
}

fn write_logical_extents(
    inventory_id: &str,
    object_id: &str,
    extents: Vec<(&ceph_wire::BlueStoreLogicalExtent, Option<u32>, u32)>,
    object_size: u64,
    result: &mut FinalizedObjects,
) -> Result<(), CommandError> {
    let mut previous_end = 0u64;
    for (ordinal, (extent, shard_ordinal, blob_ordinal)) in extents.into_iter().enumerate() {
        let logical_offset = u64::from(extent.logical_offset);
        let end = logical_offset
            .checked_add(u64::from(extent.length))
            .ok_or_else(|| row_error("logical extent end overflow"))?;
        if logical_offset < previous_end || end > object_size {
            return Err(row_error(
                "logical extents overlap or exceed the object size",
            ));
        }
        previous_end = end;
        result
            .logical_extents
            .push(CephBluestoreLogicalExtentRecord {
                inventory_id: inventory_id.to_string(),
                object_identity_sha256: object_id.to_string(),
                extent_ordinal: ordinal as u32,
                logical_offset,
                length: u64::from(extent.length),
                blob_ordinal,
                blob_offset: u64::from(extent.blob_offset),
                shard_ordinal,
                defines_blob: extent.defines_blob,
                flags_raw: extent.flags.raw,
                flag_contiguous: extent.flags.contiguous,
                flag_zero_blob_offset: extent.flags.zero_blob_offset,
                flag_same_length: extent.flags.same_length,
                flag_spanning: extent.flags.spanning,
            });
    }
    Ok(())
}

fn update_blob_logical_counts(object_id: &str, result: &mut FinalizedObjects) {
    for blob in result
        .blobs
        .iter_mut()
        .filter(|row| row.object_identity_sha256 == object_id)
    {
        blob.logical_extent_count = result
            .logical_extents
            .iter()
            .filter(|extent| {
                extent.object_identity_sha256 == object_id
                    && extent.blob_ordinal == blob.blob_ordinal
            })
            .count() as u64;
    }
}

fn count_rows<T>(rows: &[T], object_id: &str, identity: impl Fn(&T) -> &str) -> u64 {
    rows.iter().filter(|row| identity(row) == object_id).count() as u64
}

fn row_error(message: impl Into<String>) -> CommandError {
    CommandError::parser(format!(
        "BlueStore object row closure failed: {}",
        message.into()
    ))
}
