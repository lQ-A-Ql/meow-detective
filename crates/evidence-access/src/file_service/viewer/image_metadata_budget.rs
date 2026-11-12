//! Bound EXIF decoded allocations even when many tags alias the same payload bytes.

use super::image_metadata_input::{MetadataInputError, MAX_METADATA_BYTES};

pub(super) fn validate_budget(bytes: &[u8]) -> Result<(), MetadataInputError> {
    let little = match bytes.get(..4) {
        Some(b"II*\0") => true,
        Some(b"MM\0*") => false,
        _ => return Err(MetadataInputError::Corrupt),
    };
    let u16_at = |offset: usize| -> Result<u16, MetadataInputError> {
        let value = bytes
            .get(offset..offset.checked_add(2).ok_or(MetadataInputError::Corrupt)?)
            .ok_or(MetadataInputError::Corrupt)?
            .try_into()
            .map_err(|_| MetadataInputError::Corrupt)?;
        Ok(if little {
            u16::from_le_bytes(value)
        } else {
            u16::from_be_bytes(value)
        })
    };
    let u32_at = |offset: usize| -> Result<u32, MetadataInputError> {
        let value = bytes
            .get(offset..offset.checked_add(4).ok_or(MetadataInputError::Corrupt)?)
            .ok_or(MetadataInputError::Corrupt)?
            .try_into()
            .map_err(|_| MetadataInputError::Corrupt)?;
        Ok(if little {
            u32::from_le_bytes(value)
        } else {
            u32::from_be_bytes(value)
        })
    };
    let mut pending = vec![u32_at(4)? as usize];
    let mut visited = std::collections::HashSet::new();
    let mut entries = 0usize;
    let mut allocation = 0usize;
    while let Some(offset) = pending.pop() {
        if offset == 0 {
            continue;
        }
        if !visited.insert(offset) {
            return Err(MetadataInputError::Corrupt);
        }
        let count = usize::from(u16_at(offset)?);
        let table_end = offset
            .checked_add(2 + count * 12 + 4)
            .ok_or(MetadataInputError::Corrupt)?;
        if table_end > bytes.len() {
            return Err(MetadataInputError::Corrupt);
        }
        entries += count;
        if entries > 2048 || visited.len() > 32 {
            return Err(MetadataInputError::Limit);
        }
        for index in 0..count {
            let position = offset + 2 + index * 12;
            let tag = u16_at(position)?;
            let width = match u16_at(position + 2)? {
                1 | 2 | 6 | 7 => 1,
                3 | 8 => 2,
                4 | 9 | 11 => 4,
                5 | 10 | 12 => 8,
                _ => 0,
            };
            let size = width * u64::from(u32_at(position + 4)?);
            if size > 4096 {
                return Err(MetadataInputError::Limit);
            }
            allocation += size as usize;
            if allocation > MAX_METADATA_BYTES {
                return Err(MetadataInputError::Limit);
            }
            let value = u32_at(position + 8)?;
            if size > 4 && (value as u64 + size) > bytes.len() as u64 {
                return Err(MetadataInputError::Corrupt);
            }
            if matches!(tag, 0x8769 | 0x8825 | 0xa005) {
                pending.push(value as usize);
            }
        }
        pending.push(u32_at(offset + 2 + count * 12)? as usize);
    }
    Ok(())
}
