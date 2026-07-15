use std::collections::BTreeMap;

use ceph_wire::{
    decode_bluestore_latest_value, decode_bluestore_latest_value_with_spanning_blobs,
    decode_bluestore_object_key, BlueStoreBlob, BlueStoreDecodedRecord, BlueStoreExtentPayload,
    BlueStoreKeySpace, BlueStoreObjectId, BlueStoreObjectKey, BlueStoreObjectRecord,
    BlueStoreOnodeHeader, BlueStoreOnodeTail, BlueStoreSemanticLimits,
};
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestoreLogicalExtentRecord,
    CephBluestoreObjectRecord, CephBluestoreOnodeShardRecord, CephBluestorePhysicalExtentRecord,
};
use transport::CommandError;

pub(super) struct PendingObject {
    object: BlueStoreObjectId,
    onode: Option<(BlueStoreOnodeHeader, BlueStoreOnodeTail)>,
    shards: BTreeMap<u32, BlueStoreExtentPayload>,
}

pub(super) struct FinalizedObjects {
    pub(super) objects: Vec<CephBluestoreObjectRecord>,
    pub(super) onode_shards: Vec<CephBluestoreOnodeShardRecord>,
    pub(super) blobs: Vec<CephBluestoreBlobRecord>,
    pub(super) checksum_chunks: Vec<CephBluestoreChecksumChunkRecord>,
    pub(super) logical_extents: Vec<CephBluestoreLogicalExtentRecord>,
    pub(super) physical_extents: Vec<CephBluestorePhysicalExtentRecord>,
}

pub(super) fn observe_object(
    objects: &mut BTreeMap<BlueStoreObjectId, PendingObject>,
    logical_key: &[u8],
    value: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<(), CommandError> {
    match decode_bluestore_object_key(logical_key, limits).map_err(map_decode_error)? {
        BlueStoreObjectKey::Onode(object) => {
            let decoded = decode_bluestore_latest_value(
                BlueStoreKeySpace::Object,
                logical_key,
                value,
                limits,
            )
            .map_err(map_decode_error)?;
            let BlueStoreDecodedRecord::Object(record) = decoded else {
                return Err(object_error("onode decoder returned a non-object record"));
            };
            let BlueStoreObjectRecord::Onode {
                object: decoded_object,
                onode,
                tail,
            } = *record
            else {
                return Err(object_error("onode key decoded as an extent shard"));
            };
            if decoded_object != object {
                return Err(object_error("object key identity changed during decode"));
            }
            let pending = objects
                .entry(object.clone())
                .or_insert_with(|| PendingObject::new(object));
            pending.observe_onode(onode, tail)
        }
        BlueStoreObjectKey::ExtentShard {
            object,
            shard_offset,
        } => objects
            .get_mut(&object)
            .ok_or_else(|| object_error("extent shard sorted before its owning onode"))?
            .observe_shard(logical_key, shard_offset, value, limits),
    }
}

pub(super) fn finalize_objects(
    inventory_id: &str,
    objects: BTreeMap<BlueStoreObjectId, PendingObject>,
    device_size: u64,
) -> Result<FinalizedObjects, CommandError> {
    let mut result = FinalizedObjects {
        objects: Vec::with_capacity(objects.len()),
        onode_shards: Vec::new(),
        blobs: Vec::new(),
        checksum_chunks: Vec::new(),
        logical_extents: Vec::new(),
        physical_extents: Vec::new(),
    };
    for pending in objects.into_values() {
        pending.finish(inventory_id, device_size, &mut result)?;
    }
    result.objects.sort_by(|left, right| {
        left.object_identity_sha256
            .cmp(&right.object_identity_sha256)
    });
    result.onode_shards.sort_by(|left, right| {
        (&left.object_identity_sha256, left.shard_ordinal)
            .cmp(&(&right.object_identity_sha256, right.shard_ordinal))
    });
    result.blobs.sort_by(|left, right| {
        (&left.object_identity_sha256, left.blob_ordinal)
            .cmp(&(&right.object_identity_sha256, right.blob_ordinal))
    });
    result.checksum_chunks.sort_by(|left, right| {
        (
            &left.object_identity_sha256,
            left.blob_ordinal,
            left.checksum_ordinal,
        )
            .cmp(&(
                &right.object_identity_sha256,
                right.blob_ordinal,
                right.checksum_ordinal,
            ))
    });
    result.logical_extents.sort_by(|left, right| {
        (&left.object_identity_sha256, left.extent_ordinal)
            .cmp(&(&right.object_identity_sha256, right.extent_ordinal))
    });
    result.physical_extents.sort_by(|left, right| {
        (
            &left.object_identity_sha256,
            left.blob_ordinal,
            left.extent_ordinal,
        )
            .cmp(&(
                &right.object_identity_sha256,
                right.blob_ordinal,
                right.extent_ordinal,
            ))
    });
    Ok(result)
}

impl PendingObject {
    fn new(object: BlueStoreObjectId) -> Self {
        Self {
            object,
            onode: None,
            shards: BTreeMap::new(),
        }
    }

    fn observe_onode(
        &mut self,
        onode: BlueStoreOnodeHeader,
        tail: BlueStoreOnodeTail,
    ) -> Result<(), CommandError> {
        if self.onode.replace((onode, tail)).is_some() {
            return Err(object_error("duplicate object onode"));
        }
        Ok(())
    }

    fn observe_shard(
        &mut self,
        logical_key: &[u8],
        shard_offset: u32,
        value: &[u8],
        limits: BlueStoreSemanticLimits,
    ) -> Result<(), CommandError> {
        let (onode, tail) = self
            .onode
            .as_ref()
            .ok_or_else(|| object_error("extent shard has no owning onode"))?;
        if !onode
            .extent_shards
            .iter()
            .any(|descriptor| descriptor.offset == shard_offset)
        {
            return Err(object_error(
                "extent shard has no matching onode descriptor",
            ));
        }
        let decoded = decode_bluestore_latest_value_with_spanning_blobs(
            BlueStoreKeySpace::Object,
            logical_key,
            value,
            spanning_blobs(tail),
            limits,
        )
        .map_err(map_decode_error)?;
        let BlueStoreDecodedRecord::Object(record) = decoded else {
            return Err(object_error("extent shard decoded as a non-object record"));
        };
        let BlueStoreObjectRecord::ExtentShard {
            object,
            shard_offset: decoded_offset,
            payload,
        } = *record
        else {
            return Err(object_error("extent shard key decoded as an onode"));
        };
        if object != self.object || decoded_offset != shard_offset {
            return Err(object_error("extent shard identity changed during decode"));
        }
        if self.shards.insert(shard_offset, payload).is_some() {
            return Err(object_error("duplicate object extent shard"));
        }
        Ok(())
    }

    fn finish(
        self,
        inventory_id: &str,
        device_size: u64,
        result: &mut FinalizedObjects,
    ) -> Result<(), CommandError> {
        let (onode, tail) = self
            .onode
            .ok_or_else(|| object_error("object has extent shards but no onode"))?;
        super::object_rows::finish_object(
            inventory_id,
            self.object,
            onode,
            tail,
            self.shards,
            device_size,
            result,
        )
    }
}

fn spanning_blobs(tail: &BlueStoreOnodeTail) -> &[BlueStoreBlob] {
    match tail {
        BlueStoreOnodeTail::Decoded { spanning_blobs, .. } => spanning_blobs,
    }
}

fn map_decode_error(error: ceph_wire::CephWireError) -> CommandError {
    let message = format!("BlueStore object semantic decode failed: {error}");
    if matches!(
        error,
        ceph_wire::CephWireError::LengthLimit { .. }
            | ceph_wire::CephWireError::UnsupportedBlueStoreDencVersion { .. }
            | ceph_wire::CephWireError::UnknownBlueStoreBlobFlags { .. }
            | ceph_wire::CephWireError::UnknownBlueStoreChecksumType { .. }
    ) {
        CommandError::unsupported(message)
    } else {
        CommandError::parser(message)
    }
}

fn object_error(message: impl Into<String>) -> CommandError {
    CommandError::parser(format!(
        "BlueStore object closure failed: {}",
        message.into()
    ))
}
