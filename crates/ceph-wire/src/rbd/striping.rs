use crate::{
    error::{CephWireError, Result},
    rbd::metadata::{RbdImageMetadata, RBD_MAX_ORDER, RBD_MIN_ORDER},
};

const MAX_RBD_READ_PLAN_ENTRIES: usize = 1_048_576;

/// A single logical range to fetch from one RBD data object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbdReadPlan {
    pub object_no: u64,
    pub object_offset: u64,
    pub length: u64,
    pub destination_offset: u64,
}

impl RbdReadPlan {
    pub fn data_object_name(&self, object_prefix: &str) -> Result<String> {
        format_rbd_data_object_name(object_prefix, self.object_no)
    }
}

/// Validated, read-only layout for a normalized RBD head image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbdHeadImageLayout {
    pub image_size: u64,
    pub order: u8,
    pub object_size: u64,
    pub features: u64,
    pub object_prefix: String,
    pub stripe_unit: u64,
    pub stripe_count: u64,
}

impl RbdHeadImageLayout {
    pub fn new(
        image_size: u64,
        order: u8,
        object_prefix: impl Into<String>,
        stripe_unit: u64,
        stripe_count: u64,
    ) -> Result<Self> {
        Self::new_with_features(
            image_size,
            order,
            0,
            object_prefix,
            stripe_unit,
            stripe_count,
        )
    }

    pub fn new_with_features(
        image_size: u64,
        order: u8,
        features: u64,
        object_prefix: impl Into<String>,
        stripe_unit: u64,
        stripe_count: u64,
    ) -> Result<Self> {
        let object_prefix = object_prefix.into();
        let object_size = validate_order(order)?;
        let (stripe_unit, stripe_count) =
            normalize_striping(object_size, stripe_unit, stripe_count)?;
        validate_prefix(&object_prefix)?;
        object_size
            .checked_mul(stripe_count)
            .ok_or(CephWireError::InvalidRbdLayout {
                reason: "stripe period overflows u64",
            })?;

        Ok(Self {
            image_size,
            order,
            object_size,
            features,
            object_prefix,
            stripe_unit,
            stripe_count,
        })
    }

    pub fn from_metadata(metadata: &RbdImageMetadata) -> Result<Self> {
        Self::new_with_features(
            metadata.image_size,
            metadata.order,
            metadata.features,
            metadata.object_prefix.clone(),
            metadata.stripe_unit,
            metadata.stripe_count,
        )
    }

    pub fn data_object_name(&self, object_no: u64) -> String {
        format_rbd_data_object_name_unchecked(&self.object_prefix, object_no)
    }

    pub fn map_range(&self, offset: u64, length: u64) -> Result<Vec<RbdReadPlan>> {
        if length == 0 {
            return Ok(Vec::new());
        }
        let end = offset
            .checked_add(length)
            .ok_or(CephWireError::RbdRangeOverflow { offset, length })?;
        if offset >= self.image_size {
            return Err(CephWireError::RbdRangeOutOfBounds {
                offset,
                length,
                image_size: self.image_size,
            });
        }

        let clipped_end = end.min(self.image_size);
        let stripe_unit = if self.stripe_count == 1 {
            self.object_size
        } else {
            self.stripe_unit
        };
        let stripes_per_object = self.object_size / stripe_unit;
        let mut current = offset;
        let mut plans = Vec::new();
        while current < clipped_end {
            let block_no = current / stripe_unit;
            let stripe_no = block_no / self.stripe_count;
            let stripe_pos = block_no % self.stripe_count;
            let object_set_no = stripe_no / stripes_per_object;
            let object_no = object_set_no
                .checked_mul(self.stripe_count)
                .and_then(|value| value.checked_add(stripe_pos))
                .ok_or(CephWireError::RbdRangeOverflow { offset, length })?;
            let block_start = (stripe_no % stripes_per_object)
                .checked_mul(stripe_unit)
                .ok_or(CephWireError::RbdRangeOverflow { offset, length })?;
            let block_offset = current % stripe_unit;
            let object_offset = block_start
                .checked_add(block_offset)
                .ok_or(CephWireError::RbdRangeOverflow { offset, length })?;
            let available = stripe_unit - block_offset;
            let chunk_length = available.min(clipped_end - current);

            plans.push(RbdReadPlan {
                object_no,
                object_offset,
                length: chunk_length,
                destination_offset: current - offset,
            });
            if plans.len() > MAX_RBD_READ_PLAN_ENTRIES {
                return Err(CephWireError::InvalidRbdLayout {
                    reason: "range requires too many object extents",
                });
            }
            current = current
                .checked_add(chunk_length)
                .ok_or(CephWireError::RbdRangeOverflow { offset, length })?;
        }
        Ok(plans)
    }

    pub fn plan_range(&self, offset: u64, length: u64) -> Result<Vec<RbdReadPlan>> {
        self.map_range(offset, length)
    }
}

pub fn format_rbd_data_object_name(object_prefix: &str, object_no: u64) -> Result<String> {
    validate_prefix(object_prefix)?;
    Ok(format_rbd_data_object_name_unchecked(
        object_prefix,
        object_no,
    ))
}

fn validate_order(order: u8) -> Result<u64> {
    if !(RBD_MIN_ORDER..=RBD_MAX_ORDER).contains(&order) {
        return Err(CephWireError::InvalidRbdLayout {
            reason: "order must be in the Ceph-supported range [12, 25]",
        });
    }
    1u64.checked_shl(u32::from(order))
        .ok_or(CephWireError::InvalidRbdLayout {
            reason: "order shift overflows u64",
        })
}

fn normalize_striping(object_size: u64, stripe_unit: u64, stripe_count: u64) -> Result<(u64, u64)> {
    if stripe_unit == 0 && stripe_count == 0 {
        return Ok((object_size, 1));
    }
    if stripe_unit == 0 || stripe_count == 0 {
        return Err(CephWireError::InvalidRbdLayout {
            reason: "stripe_unit and stripe_count must both be set",
        });
    }
    if stripe_unit > object_size || !object_size.is_multiple_of(stripe_unit) {
        return Err(CephWireError::InvalidRbdLayout {
            reason: "stripe_unit must be nonzero and divide object_size",
        });
    }
    object_size
        .checked_mul(stripe_count)
        .ok_or(CephWireError::InvalidRbdLayout {
            reason: "stripe period overflows u64",
        })?;
    Ok((stripe_unit, stripe_count))
}

fn validate_prefix(object_prefix: &str) -> Result<()> {
    if object_prefix.is_empty() {
        return Err(CephWireError::InvalidRbdLayout {
            reason: "object_prefix must not be empty",
        });
    }
    if object_prefix.len() > crate::rbd::metadata::RBD_MAX_OBJECT_PREFIX_LENGTH {
        return Err(CephWireError::InvalidRbdLayout {
            reason: "object_prefix exceeds the Ceph maximum length",
        });
    }
    if object_prefix.contains('\0') {
        return Err(CephWireError::InvalidRbdLayout {
            reason: "object_prefix contains a NUL byte",
        });
    }
    Ok(())
}

fn format_rbd_data_object_name_unchecked(object_prefix: &str, object_no: u64) -> String {
    format!("{object_prefix}.{object_no:016x}")
}
