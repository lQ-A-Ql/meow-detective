use std::cmp::min;

use crate::{
    codec::{decode_string, CephDecode, CephStructEnvelope},
    CephCursor, CephWireError, Result,
};

const MAX_POOL_NAMESPACE_BYTES: usize = 4 * 1024;
const FILE_LAYOUT_DECODER_VERSION: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsFileLayout {
    pub stripe_unit: u32,
    pub stripe_count: u32,
    pub object_size: u32,
    pub pool_id: i64,
    pub pool_namespace: String,
}

impl CephFsFileLayout {
    pub fn new(
        stripe_unit: u32,
        stripe_count: u32,
        object_size: u32,
        pool_id: i64,
        pool_namespace: impl Into<String>,
    ) -> Result<Self> {
        let layout = Self {
            stripe_unit,
            stripe_count,
            object_size,
            pool_id,
            pool_namespace: pool_namespace.into(),
        };
        layout.validate()?;
        Ok(layout)
    }

    pub fn is_empty(&self) -> bool {
        self.stripe_unit == 0 && self.stripe_count == 0 && self.object_size == 0
    }

    pub(crate) fn stripe_period(&self) -> Result<u64> {
        self.validate()?;
        u64::from(self.stripe_unit)
            .checked_mul(u64::from(self.stripe_count))
            .ok_or(CephWireError::CephFsLayoutRangeOverflow)
    }

    pub(crate) fn object_set_size(&self) -> Result<u64> {
        self.validate()?;
        u64::from(self.object_size)
            .checked_mul(u64::from(self.stripe_count))
            .ok_or(CephWireError::CephFsLayoutRangeOverflow)
    }

    fn validate(&self) -> Result<()> {
        if self.is_empty() {
            if self.pool_id < -1 || !self.pool_namespace.is_empty() {
                return Err(CephWireError::InvalidCephFsLayout {
                    field: "empty_layout",
                    reason: "an empty layout may only use pool -1 and an empty namespace",
                });
            }
            return Ok(());
        }
        if self.stripe_unit < 65_536 || !self.stripe_unit.is_multiple_of(65_536) {
            return Err(CephWireError::InvalidCephFsLayout {
                field: "stripe_unit",
                reason: "must be a multiple of the 64 KiB CephFS minimum",
            });
        }
        if self.stripe_count == 0 {
            return Err(CephWireError::InvalidCephFsLayout {
                field: "stripe_count",
                reason: "must be non-zero",
            });
        }
        if self.object_size < self.stripe_unit || !self.object_size.is_multiple_of(self.stripe_unit)
        {
            return Err(CephWireError::InvalidCephFsLayout {
                field: "object_size",
                reason: "must be at least stripe_unit and a multiple of stripe_unit",
            });
        }
        if self.pool_id < -1 {
            return Err(CephWireError::InvalidCephFsLayout {
                field: "pool_id",
                reason: "must be -1 or a non-negative pool id",
            });
        }
        if self.pool_namespace.len() > MAX_POOL_NAMESPACE_BYTES
            || self.pool_namespace.contains('\0')
        {
            return Err(CephWireError::InvalidCephFsLayout {
                field: "pool_namespace",
                reason: "is too long or contains a NUL byte",
            });
        }
        Ok(())
    }

    pub fn plan_range(
        &self,
        file_size: u64,
        offset: u64,
        length: usize,
    ) -> Result<Vec<CephFsLayoutSegment>> {
        self.validate()?;
        let length = u64::try_from(length).map_err(|_| CephWireError::CephFsLayoutRangeOverflow)?;
        let end = offset
            .checked_add(length)
            .ok_or(CephWireError::CephFsLayoutRangeOverflow)?;
        if offset > file_size || end > file_size {
            return Err(CephWireError::CephFsLayoutRangeOutOfBounds {
                offset,
                length,
                file_size,
            });
        }
        if length == 0 {
            return Ok(Vec::new());
        }
        if self.is_empty() {
            return Err(CephWireError::InvalidCephFsLayout {
                field: "layout",
                reason: "file has no data layout",
            });
        }
        let object_set_size = self.object_set_size()?;
        let stripe_period = self.stripe_period()?;
        let mut logical = offset;
        let mut remaining = length;
        let mut segments = Vec::new();
        while remaining > 0 {
            let set_number = logical / object_set_size;
            let within_set = logical % object_set_size;
            let stripe_round = within_set / stripe_period;
            let within_stripe = within_set % stripe_period;
            let stripe_index = within_stripe / u64::from(self.stripe_unit);
            let within_unit = within_stripe % u64::from(self.stripe_unit);
            let object_number = set_number
                .checked_mul(u64::from(self.stripe_count))
                .and_then(|value| value.checked_add(stripe_index))
                .ok_or(CephWireError::CephFsLayoutRangeOverflow)?;
            let object_offset = stripe_round
                .checked_mul(u64::from(self.stripe_unit))
                .and_then(|value| value.checked_add(within_unit))
                .ok_or(CephWireError::CephFsLayoutRangeOverflow)?;
            let object_remaining = u64::from(self.object_size)
                .checked_sub(object_offset)
                .ok_or(CephWireError::CephFsLayoutRangeOverflow)?;
            let segment_length = min(
                remaining,
                min(u64::from(self.stripe_unit) - within_unit, object_remaining),
            );
            if segment_length == 0 {
                return Err(CephWireError::CephFsLayoutRangeOverflow);
            }
            segments.push(CephFsLayoutSegment {
                logical_offset: logical,
                object_number: u32::try_from(object_number)
                    .map_err(|_| CephWireError::CephFsObjectIndexOverflow)?,
                object_offset,
                length: segment_length,
            });
            logical = logical
                .checked_add(segment_length)
                .ok_or(CephWireError::CephFsLayoutRangeOverflow)?;
            remaining -= segment_length;
        }
        Ok(segments)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CephFsLayoutSegment {
    pub logical_offset: u64,
    pub object_number: u32,
    pub object_offset: u64,
    pub length: u64,
}

pub fn format_cephfs_data_object_name(inode: u64, object_number: u32) -> Result<String> {
    if inode == 0 {
        return Err(CephWireError::InvalidCephFsInode {
            field: "ino",
            reason: "must be non-zero",
        });
    }
    Ok(format!("{inode:x}.{object_number:08x}"))
}

pub fn decode_cephfs_file_layout(input: &[u8]) -> Result<CephFsFileLayout> {
    let mut cursor = CephCursor::new(input);
    let layout = decode_file_layout(&mut cursor)?;
    if !cursor.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: "file_layout",
            remaining: cursor.remaining(),
        });
    }
    Ok(layout)
}

pub(crate) fn decode_file_layout(cursor: &mut CephCursor<'_>) -> Result<CephFsFileLayout> {
    let first = *cursor
        .input()
        .get(cursor.position())
        .ok_or(CephWireError::UnexpectedEof {
            offset: cursor.position(),
            needed: 1,
            remaining: cursor.remaining(),
        })?;
    if first == 0 {
        return decode_legacy_layout(cursor);
    }
    let (envelope, mut payload) = CephStructEnvelope::decode_payload(cursor, 2)?;
    if !(1..=FILE_LAYOUT_DECODER_VERSION).contains(&envelope.version) {
        return Err(CephWireError::UnsupportedCephFsLayoutVersion {
            encoded_version: envelope.version,
            compat_version: envelope.compat_version,
        });
    }
    let stripe_unit = u32::decode(&mut payload)?;
    let stripe_count = u32::decode(&mut payload)?;
    let object_size = u32::decode(&mut payload)?;
    let pool_id = i64::decode(&mut payload)?;
    let pool_namespace = decode_string(
        &mut payload,
        MAX_POOL_NAMESPACE_BYTES,
        "CephFS pool namespace",
    )?;
    if !payload.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: "file_layout",
            remaining: payload.remaining(),
        });
    }
    CephFsFileLayout::new(
        stripe_unit,
        stripe_count,
        object_size,
        pool_id,
        pool_namespace,
    )
}

fn decode_legacy_layout(cursor: &mut CephCursor<'_>) -> Result<CephFsFileLayout> {
    let stripe_unit = u32::decode(cursor)?;
    let stripe_count = u32::decode(cursor)?;
    let object_size = u32::decode(cursor)?;
    u32::decode(cursor)?; // legacy CAS hash
    u32::decode(cursor)?; // legacy object stripe unit
    u32::decode(cursor)?; // legacy unused field
    let pool_id = i64::from(i32::from_le_bytes(u32::decode(cursor)?.to_le_bytes()));
    let pool_id = if stripe_unit == 0 && stripe_count == 0 && object_size == 0 && pool_id == 0 {
        -1
    } else {
        pool_id
    };
    CephFsFileLayout::new(stripe_unit, stripe_count, object_size, pool_id, "")
}
