use std::collections::BTreeMap;

use super::{invalid_extent, missing_blob};
use crate::{
    bluestore_semantic::{
        blob::decode_blob,
        budget::SemanticBudget,
        denc::{read_varint_lowz_u64, read_varint_u64},
        types::{
            BlueStoreBlob, BlueStoreBlobIdentity, BlueStoreExtentFlags, BlueStoreLogicalExtent,
        },
    },
    cursor::CephCursor,
    error::{CephWireError, Result},
};

const FLAG_CONTIGUOUS: u8 = 1;
const FLAG_ZERO_OFFSET: u8 = 2;
const FLAG_SAME_LENGTH: u8 = 4;
const FLAG_SPANNING: u8 = 8;
const BLOB_ID_SHIFT: u32 = 4;

pub(super) struct ExtentDecoder {
    version: u8,
    position: u64,
    previous_length: u32,
    local_blobs: BTreeMap<u32, usize>,
    blobs: Vec<BlueStoreBlob>,
    extents: Vec<BlueStoreLogicalExtent>,
}

impl ExtentDecoder {
    pub(super) fn new(version: u8) -> Self {
        Self {
            version,
            position: 0,
            previous_length: 0,
            local_blobs: BTreeMap::new(),
            blobs: Vec::new(),
            extents: Vec::new(),
        }
    }

    pub(super) fn extent_count(&self) -> usize {
        self.extents.len()
    }

    pub(super) fn into_parts(self) -> (Vec<BlueStoreBlob>, Vec<BlueStoreLogicalExtent>) {
        (self.blobs, self.extents)
    }

    pub(super) fn decode_next(
        &mut self,
        cursor: &mut CephCursor<'_>,
        budget: &mut SemanticBudget,
    ) -> Result<()> {
        let record_index =
            u32::try_from(self.extents.len()).map_err(|_| CephWireError::IntegerOverflow {
                context: "BlueStore extent record index",
            })?;
        let encoded_blob_id = read_varint_u64(cursor, "BlueStore extent blob id")?;
        let flags = decode_extent_flags(encoded_blob_id as u8);
        let logical_offset =
            u32::try_from(self.decode_logical_offset(cursor, flags, record_index)?).map_err(
                |_| CephWireError::IntegerOverflow {
                    context: "BlueStore logical extent offset",
                },
            )?;
        let blob_offset = decode_blob_offset(cursor, flags, record_index)?;
        let length = self.decode_length(cursor, flags, record_index)?;
        let (blob, defines_blob) =
            self.decode_blob_reference(cursor, encoded_blob_id, flags, record_index, budget)?;
        let logical_end = u64::from(logical_offset)
            .checked_add(u64::from(length))
            .ok_or(CephWireError::IntegerOverflow {
                context: "BlueStore logical extent end",
            })?;
        if logical_end > u64::from(u32::MAX) {
            return Err(CephWireError::IntegerOverflow {
                context: "BlueStore logical extent end",
            });
        }
        self.position = logical_end;
        self.extents.push(BlueStoreLogicalExtent {
            record_index,
            logical_offset,
            blob_offset,
            length,
            blob,
            defines_blob,
            flags,
        });
        Ok(())
    }

    fn decode_logical_offset(
        &self,
        cursor: &mut CephCursor<'_>,
        flags: BlueStoreExtentFlags,
        record_index: u32,
    ) -> Result<u64> {
        if flags.contiguous {
            return Ok(self.position);
        }
        let gap = read_varint_lowz_u64(cursor, "BlueStore extent logical gap")?;
        if gap == 0 {
            return Err(invalid_extent(
                record_index,
                "zero logical gap must use CONTIGUOUS",
            ));
        }
        self.position
            .checked_add(gap)
            .ok_or(CephWireError::IntegerOverflow {
                context: "BlueStore extent logical offset",
            })
    }

    fn decode_length(
        &mut self,
        cursor: &mut CephCursor<'_>,
        flags: BlueStoreExtentFlags,
        record_index: u32,
    ) -> Result<u32> {
        let length = if flags.same_length {
            self.previous_length
        } else {
            lowz_u32(cursor, "BlueStore extent length")?
        };
        if length == 0 {
            return Err(invalid_extent(record_index, "length must be non-zero"));
        }
        if !flags.same_length && record_index != 0 && length == self.previous_length {
            return Err(invalid_extent(
                record_index,
                "unchanged length must use SAMELENGTH",
            ));
        }
        self.previous_length = length;
        Ok(length)
    }

    fn decode_blob_reference(
        &mut self,
        cursor: &mut CephCursor<'_>,
        encoded: u64,
        flags: BlueStoreExtentFlags,
        record_index: u32,
        budget: &mut SemanticBudget,
    ) -> Result<(BlueStoreBlobIdentity, bool)> {
        let raw_id = encoded >> BLOB_ID_SHIFT;
        if flags.spanning {
            return Ok((BlueStoreBlobIdentity::Spanning(raw_id), false));
        }
        if raw_id != 0 {
            let local_id =
                u32::try_from(raw_id - 1).map_err(|_| CephWireError::IntegerOverflow {
                    context: "BlueStore local blob id",
                })?;
            if !self.local_blobs.contains_key(&local_id) {
                return Err(missing_blob(record_index, "local", u64::from(local_id)));
            }
            return Ok((BlueStoreBlobIdentity::Local(local_id), false));
        }
        budget.claim_blobs(1)?;
        let identity = BlueStoreBlobIdentity::Local(record_index);
        let blob = decode_blob(cursor, self.version, identity, false, budget)?;
        let blob_index = self.blobs.len();
        if self.local_blobs.insert(record_index, blob_index).is_some() {
            return Err(CephWireError::DuplicateBlueStoreBlob {
                kind: "local",
                blob_id: u64::from(record_index),
            });
        }
        self.blobs.push(blob);
        Ok((identity, true))
    }
}

fn decode_extent_flags(encoded_blob_id: u8) -> BlueStoreExtentFlags {
    let raw = encoded_blob_id & 0x0f;
    BlueStoreExtentFlags {
        raw,
        contiguous: raw & FLAG_CONTIGUOUS != 0,
        zero_blob_offset: raw & FLAG_ZERO_OFFSET != 0,
        same_length: raw & FLAG_SAME_LENGTH != 0,
        spanning: raw & FLAG_SPANNING != 0,
    }
}

fn decode_blob_offset(
    cursor: &mut CephCursor<'_>,
    flags: BlueStoreExtentFlags,
    record_index: u32,
) -> Result<u32> {
    if flags.zero_blob_offset {
        return Ok(0);
    }
    let offset = lowz_u32(cursor, "BlueStore extent blob offset")?;
    if offset == 0 {
        return Err(invalid_extent(
            record_index,
            "zero blob offset must use ZEROOFFSET",
        ));
    }
    Ok(offset)
}

fn lowz_u32(cursor: &mut CephCursor<'_>, context: &'static str) -> Result<u32> {
    let value = read_varint_lowz_u64(cursor, context)?;
    u32::try_from(value).map_err(|_| CephWireError::IntegerOverflow { context })
}
