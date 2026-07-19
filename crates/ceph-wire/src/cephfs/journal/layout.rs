use crate::error::{CephWireError, Result};

use super::{CephFsJournalLayout, CephFsJournalObjectExtent};

const MAX_PLANNED_EXTENTS: usize = 1_000_000;

pub fn plan_cephfs_journal_range(
    layout: CephFsJournalLayout,
    offset: u64,
    length: usize,
) -> Result<Vec<CephFsJournalObjectExtent>> {
    validate_layout(layout)?;
    if length == 0 {
        return Ok(Vec::new());
    }
    let mut extents = Vec::new();
    let mut current = offset;
    let mut remaining =
        u64::try_from(length).map_err(|_| CephWireError::CephFsJournalRangeOverflow)?;
    current
        .checked_add(remaining)
        .ok_or(CephWireError::CephFsJournalRangeOverflow)?;
    let object_size = u64::from(layout.object_size);
    let stripe_count = u64::from(layout.stripe_count);
    let stripe_unit = if layout.stripe_count == 1 {
        object_size
    } else {
        u64::from(layout.stripe_unit)
    };
    let stripes_per_object = object_size / stripe_unit;
    while remaining != 0 {
        if extents.len() == MAX_PLANNED_EXTENTS {
            return Err(CephWireError::LengthLimit {
                context: "CephFS journal extents",
                length: extents.len() + 1,
                limit: MAX_PLANNED_EXTENTS,
            });
        }
        let block_number = current / stripe_unit;
        let stripe_number = block_number / stripe_count;
        let stripe_position = block_number % stripe_count;
        let object_set = stripe_number / stripes_per_object;
        let object_number = object_set
            .checked_mul(stripe_count)
            .and_then(|base| base.checked_add(stripe_position))
            .ok_or(CephWireError::CephFsJournalRangeOverflow)?;
        let object_index =
            u32::try_from(object_number).map_err(|_| CephWireError::CephFsJournalRangeOverflow)?;
        let block_start = (stripe_number % stripes_per_object) * stripe_unit;
        let block_offset = current % stripe_unit;
        let extent_length = remaining.min(stripe_unit - block_offset);
        extents.push(CephFsJournalObjectExtent {
            logical_offset: current,
            object_index,
            object_offset: block_start + block_offset,
            length: usize::try_from(extent_length)
                .map_err(|_| CephWireError::CephFsJournalRangeOverflow)?,
        });
        current = current
            .checked_add(extent_length)
            .ok_or(CephWireError::CephFsJournalRangeOverflow)?;
        remaining -= extent_length;
    }
    Ok(extents)
}

fn validate_layout(layout: CephFsJournalLayout) -> Result<()> {
    if layout.stripe_unit == 0
        || layout.stripe_count == 0
        || layout.object_size < layout.stripe_unit
        || !layout.object_size.is_multiple_of(layout.stripe_unit)
    {
        return Err(CephWireError::InvalidCephFsJournal {
            context: "layout",
            reason: "invalid stripe geometry",
        });
    }
    Ok(())
}
