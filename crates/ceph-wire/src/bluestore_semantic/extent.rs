mod decoder;
mod validation;

use decoder::ExtentDecoder;

use crate::{
    bluestore_semantic::{
        blob::decode_blob,
        budget::SemanticBudget,
        denc::{ensure_limit, read_varint_u32, read_varint_u64},
        types::{
            BlueStoreBlob, BlueStoreBlobIdentity, BlueStoreExtentPayload, BlueStoreSemanticLimits,
        },
    },
    codec::CephDecode,
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub fn decode_bluestore_extent_payload(
    value: &[u8],
    spanning_blobs: &[BlueStoreBlob],
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreExtentPayload> {
    let mut budget = SemanticBudget::new(limits);
    budget.claim_input(value.len())?;
    decode_extent_payload_with_budget(value, spanning_blobs, limits, &mut budget)
}

pub(crate) fn decode_extent_payload_with_budget(
    value: &[u8],
    spanning_blobs: &[BlueStoreBlob],
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<BlueStoreExtentPayload> {
    decode_extent_payload(value, spanning_blobs, limits, budget, false).map(|(payload, _)| payload)
}

pub(crate) fn decode_extent_payload_without_spanning_context_with_budget(
    value: &[u8],
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<(BlueStoreExtentPayload, bool)> {
    decode_extent_payload(value, &[], limits, budget, true)
}

fn decode_extent_payload(
    value: &[u8],
    spanning_blobs: &[BlueStoreBlob],
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
    allow_missing_spanning: bool,
) -> Result<(BlueStoreExtentPayload, bool)> {
    ensure_limit(
        value.len(),
        limits.max_extent_payload_bytes,
        "BlueStore extent payload",
    )?;
    let mut cursor = CephCursor::new(value);
    let version = u8::decode(&mut cursor)?;
    validate_extent_version(version, "BlueStore extent map")?;
    let declared = read_varint_u32(&mut cursor, "BlueStore extent count")?;
    ensure_limit(
        declared as usize,
        limits.max_extent_records,
        "BlueStore extent records",
    )?;
    let mut decoder = ExtentDecoder::new(version);
    while !cursor.is_empty() {
        ensure_limit(
            decoder.extent_count() + 1,
            limits.max_extent_records,
            "BlueStore extent records",
        )?;
        budget.claim_extent_records(1)?;
        decoder.decode_next(&mut cursor, budget)?;
    }
    let decoded =
        u32::try_from(decoder.extent_count()).map_err(|_| CephWireError::IntegerOverflow {
            context: "BlueStore decoded extent count",
        })?;
    if decoded != declared {
        return Err(CephWireError::BlueStoreExtentCountMismatch { declared, decoded });
    }
    let (blobs, extents) = decoder.into_parts();
    let payload = BlueStoreExtentPayload {
        version,
        declared_extent_count: declared,
        encoded_length: value.len(),
        blobs,
        extents,
    };
    let validation_entries = payload
        .blobs
        .len()
        .checked_add(spanning_blobs.len())
        .and_then(|count| count.checked_add(payload.extents.len()))
        .ok_or(CephWireError::LengthOverflow {
            context: "BlueStore validation entries",
        })?;
    budget.claim_validation_entries(validation_entries)?;
    let missing_spanning =
        payload.validate_with_optional_spanning_blobs(spanning_blobs, allow_missing_spanning)?;
    Ok((payload, missing_spanning))
}

pub(crate) fn decode_spanning_blobs(
    cursor: &mut CephCursor<'_>,
    limits: BlueStoreSemanticLimits,
    budget: &mut SemanticBudget,
) -> Result<(u8, Vec<BlueStoreBlob>)> {
    let version = u8::decode(cursor)?;
    validate_extent_version(version, "BlueStore spanning blob map")?;
    let count = read_varint_u32(cursor, "BlueStore spanning blob count")? as usize;
    ensure_limit(count, limits.max_spanning_blobs, "BlueStore spanning blobs")?;
    budget.claim_blobs(count)?;
    let mut blobs = Vec::new();
    let mut previous_id = None;
    for _ in 0..count {
        let id = read_varint_u64(cursor, "BlueStore spanning blob id")?;
        validate_spanning_id(previous_id, id)?;
        blobs.push(decode_blob(
            cursor,
            version,
            BlueStoreBlobIdentity::Spanning(id),
            true,
            budget,
        )?);
        previous_id = Some(id);
    }
    Ok((version, blobs))
}

pub(crate) fn validate_extent_version(version: u8, context: &'static str) -> Result<()> {
    if matches!(version, 1 | 2) {
        Ok(())
    } else {
        Err(CephWireError::UnsupportedBlueStoreDencVersion {
            context,
            encoded_version: version,
            supported_versions: "1 or 2",
        })
    }
}

fn validate_spanning_id(previous: Option<u64>, id: u64) -> Result<()> {
    if id > i16::MAX as u64 {
        return Err(invalid_payload(
            "spanning blob id exceeds Ceph's int16 range",
        ));
    }
    if previous.is_some_and(|previous| previous >= id) {
        return Err(CephWireError::DuplicateBlueStoreBlob {
            kind: "spanning",
            blob_id: id,
        });
    }
    Ok(())
}

fn missing_blob(record_index: u32, kind: &'static str, blob_id: u64) -> CephWireError {
    CephWireError::MissingBlueStoreBlobReference {
        record_index,
        kind,
        blob_id,
    }
}

fn invalid_extent(record_index: u32, reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreExtent {
        record_index,
        reason,
    }
}

fn invalid_payload(reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreSemanticValue {
        context: "BlueStore extent payload",
        reason,
    }
}
