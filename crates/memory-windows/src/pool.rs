use crate::{MemoryWindowsError, RawMemoryImage, Result};

const POOL_HEADER_LEN: u64 = 0x10;
const POOL_TAG_OFFSET: u64 = 4;
const MAX_POOL_ALLOCATION_BYTES: u64 = 16 * 1024 * 1024;

/// A bounded inventory record for a tagged kernel-pool allocation.
///
/// It deliberately contains offsets and lengths only. The allocation contents
/// stay in the raw image until a matching, reviewed parser profile requests a
/// precisely bounded field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolAllocation {
    pub header_physical_address: u64,
    pub body_physical_address: u64,
    pub allocation_bytes: u64,
}

/// Locates pool allocations by their four-byte tag, with header and size checks.
pub fn scan_pool_tag(
    image: &mut RawMemoryImage,
    tag: [u8; 4],
    maximum_matches: usize,
) -> Result<Vec<PoolAllocation>> {
    let tag_positions = image.scan_tag(tag, maximum_matches.saturating_mul(8))?;
    let mut allocations = Vec::new();
    for tag_position in tag_positions {
        let Some(header_physical_address) = tag_position.checked_sub(POOL_TAG_OFFSET) else {
            continue;
        };
        let mut header = [0u8; POOL_HEADER_LEN as usize];
        if image
            .read_exact_at(header_physical_address, &mut header)
            .is_err()
            || header[POOL_TAG_OFFSET as usize..POOL_TAG_OFFSET as usize + 4] != tag
        {
            continue;
        }
        let allocation_bytes = u64::from(header[2]) * 16;
        let body_physical_address = match header_physical_address.checked_add(POOL_HEADER_LEN) {
            Some(value) => value,
            None => continue,
        };
        let end = match header_physical_address.checked_add(allocation_bytes) {
            Some(value) => value,
            None => continue,
        };
        if !(POOL_HEADER_LEN..=MAX_POOL_ALLOCATION_BYTES).contains(&allocation_bytes)
            || end > image.len()
        {
            continue;
        }
        allocations.push(PoolAllocation {
            header_physical_address,
            body_physical_address,
            allocation_bytes,
        });
        if allocations.len() == maximum_matches {
            break;
        }
    }
    if maximum_matches == 0 && !allocations.is_empty() {
        return Err(MemoryWindowsError::MalformedModuleList);
    }
    Ok(allocations)
}

/// Locates several pool tags with one sequential image pass.
pub(crate) fn scan_pool_tags(
    image: &mut RawMemoryImage,
    tags: &[[u8; 4]],
    maximum_matches_per_tag: usize,
) -> Result<Vec<(usize, PoolAllocation)>> {
    let tag_positions = image.scan_tags(tags, maximum_matches_per_tag.saturating_mul(8))?;
    let mut counts = vec![0usize; tags.len()];
    let mut allocations = Vec::new();
    for (tag_index, tag_position) in tag_positions {
        if counts[tag_index] == maximum_matches_per_tag {
            continue;
        }
        let Some(header_physical_address) = tag_position.checked_sub(POOL_TAG_OFFSET) else {
            continue;
        };
        let mut header = [0u8; POOL_HEADER_LEN as usize];
        if image
            .read_exact_at(header_physical_address, &mut header)
            .is_err()
            || header[POOL_TAG_OFFSET as usize..POOL_TAG_OFFSET as usize + 4] != tags[tag_index]
        {
            continue;
        }
        let allocation_bytes = u64::from(header[2]) * 16;
        let Some(body_physical_address) = header_physical_address.checked_add(POOL_HEADER_LEN)
        else {
            continue;
        };
        let Some(end) = header_physical_address.checked_add(allocation_bytes) else {
            continue;
        };
        if !(POOL_HEADER_LEN..=MAX_POOL_ALLOCATION_BYTES).contains(&allocation_bytes)
            || end > image.len()
        {
            continue;
        }
        allocations.push((
            tag_index,
            PoolAllocation {
                header_physical_address,
                body_physical_address,
                allocation_bytes,
            },
        ));
        counts[tag_index] += 1;
    }
    Ok(allocations)
}
