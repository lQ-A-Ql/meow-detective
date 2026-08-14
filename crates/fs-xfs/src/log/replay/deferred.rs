//! Validation of recovery items that do not directly produce host patches.

use super::super::{XfsLogError, XfsLogFormat};
use super::assemble::AssembledItem;

const QUOTAOFF_LOG_SIZE: usize = 20;
const DEFERRED_LOG_HEADER_SIZE: usize = 16;
const DEFERRED_EXTENT_32_SIZE: usize = 12;
const DEFERRED_EXTENT_64_SIZE: usize = 16;

/// Validate an EFI/EFD wire region and return its native-endian intent ID.
pub(super) fn parse_id(
    format: XfsLogFormat,
    item: &AssembledItem,
    name: &str,
) -> Result<u64, XfsLogError> {
    if item.regions.len() != 1 {
        return Err(XfsLogError::InvalidData(format!(
            "{name} item must contain exactly one region"
        )));
    }
    let region = &item.regions[0];
    let nextents = format
        .native_u32(region, 4)
        .ok_or_else(|| XfsLogError::InvalidData(format!("{name} header is truncated")))?;
    let id = format
        .native_u64(region, 8)
        .ok_or_else(|| XfsLogError::InvalidData(format!("{name} id is truncated")))?;
    if nextents == 0 || id == 0 {
        return Err(XfsLogError::InvalidData(format!(
            "{name} has an invalid extent count or id"
        )));
    }
    if !extent_array_length_is_valid(region.len(), nextents) {
        return Err(XfsLogError::InvalidData(format!(
            "{name} extent array length is inconsistent"
        )));
    }
    Ok(id)
}

fn extent_array_length_is_valid(region_length: usize, nextents: u32) -> bool {
    let Ok(count) = usize::try_from(nextents) else {
        return false;
    };
    [DEFERRED_EXTENT_32_SIZE, DEFERRED_EXTENT_64_SIZE]
        .into_iter()
        .filter_map(|size| count.checked_mul(size))
        .filter_map(|bytes| DEFERRED_LOG_HEADER_SIZE.checked_add(bytes))
        .any(|length| length == region_length)
}

/// QUOTAOFF is a no-op during pass 2, but its fixed wire shape must still be
/// valid before the log can be discarded.
pub(super) fn validate_quotaoff(
    format: XfsLogFormat,
    item: &AssembledItem,
) -> Result<(), XfsLogError> {
    let valid = item.regions.len() == 1
        && item.regions[0].len() == QUOTAOFF_LOG_SIZE
        && format.native_u16(&item.regions[0], 2) == Some(1);
    if !valid {
        return Err(XfsLogError::InvalidData(
            "malformed QUOTAOFF item".to_string(),
        ));
    }
    Ok(())
}
