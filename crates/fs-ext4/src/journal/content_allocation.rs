use super::content::RecoveryAllocationState;
use super::error::{require_len, JournalError, JournalResult};
use crate::Ext4Reader;

const EXT4_BG_INODE_UNINIT: u16 = 0x0001;
const EXT4_BG_BLOCK_UNINIT: u16 = 0x0002;

pub(super) fn inode_allocation(
    filesystem: &Ext4Reader,
    inode_number: u32,
) -> JournalResult<RecoveryAllocationState> {
    if inode_number == 0 || inode_number > filesystem.inodes_count {
        return Err(JournalError::Invalid(format!(
            "inode {inode_number} exceeds filesystem inode bounds"
        )));
    }
    let zero_based = u64::from(inode_number - 1);
    let group = u32::try_from(zero_based / u64::from(filesystem.inodes_per_group))
        .map_err(|_| JournalError::Invalid("inode group exceeds u32".into()))?;
    let bit = zero_based % u64::from(filesystem.inodes_per_group);
    let descriptor = filesystem.read_bg_descriptor(group)?;
    verify_group_descriptor(filesystem, group, &descriptor)?;
    if group_flags(&descriptor)? & EXT4_BG_INODE_UNINIT != 0 {
        return Err(JournalError::Unsupported(format!(
            "inode bitmap for block group {group} is marked uninitialized"
        )));
    }
    let bitmap_block = descriptor_block(&descriptor, 0x04, 0x24, filesystem.has_64bit)?;
    bitmap_allocation(filesystem, group, &descriptor, bitmap_block, bit, "inode")
}

pub(super) fn block_allocation(
    filesystem: &Ext4Reader,
    block: u64,
) -> JournalResult<RecoveryAllocationState> {
    if block < filesystem.first_data_block || block >= filesystem.blocks_count {
        return Err(JournalError::Invalid(format!(
            "block {block} exceeds filesystem block bounds"
        )));
    }
    let relative = block - filesystem.first_data_block;
    let group = u32::try_from(relative / u64::from(filesystem.blocks_per_group))
        .map_err(|_| JournalError::Invalid("block group exceeds u32".into()))?;
    if group >= filesystem.num_block_groups {
        return Err(JournalError::Invalid(format!(
            "block {block} belongs to non-existent group {group}"
        )));
    }
    let bit = relative % u64::from(filesystem.blocks_per_group);
    let descriptor = filesystem.read_bg_descriptor(group)?;
    verify_group_descriptor(filesystem, group, &descriptor)?;
    if group_flags(&descriptor)? & EXT4_BG_BLOCK_UNINIT != 0 {
        return Err(JournalError::Unsupported(format!(
            "block bitmap for block group {group} is marked uninitialized"
        )));
    }
    let bitmap_block = descriptor_block(&descriptor, 0x00, 0x20, filesystem.has_64bit)?;
    bitmap_allocation(filesystem, group, &descriptor, bitmap_block, bit, "block")
}

fn bitmap_allocation(
    filesystem: &Ext4Reader,
    group: u32,
    descriptor: &[u8],
    bitmap_block: u64,
    bit: u64,
    label: &str,
) -> JournalResult<RecoveryAllocationState> {
    let bitmap = filesystem.read_metadata_block(bitmap_block)?;
    verify_bitmap_checksum(filesystem, group, descriptor, &bitmap, label)?;
    let byte_index = usize::try_from(bit / 8)
        .map_err(|_| JournalError::Invalid(format!("{label} bitmap index exceeds usize")))?;
    let byte = bitmap.get(byte_index).ok_or_else(|| {
        JournalError::Invalid(format!("{label} bitmap bit {bit} exceeds bitmap block"))
    })?;
    Ok(if byte & (1 << (bit % 8)) == 0 {
        RecoveryAllocationState::Free
    } else {
        RecoveryAllocationState::Allocated
    })
}

fn verify_group_descriptor(
    filesystem: &Ext4Reader,
    group: u32,
    descriptor: &[u8],
) -> JournalResult<()> {
    if filesystem.has_gdt_csum && !filesystem.has_metadata_csum {
        return Err(JournalError::Unsupported(
            "deleted-content validation does not yet support legacy GDT_CSUM descriptors".into(),
        ));
    }
    if !filesystem.has_metadata_csum {
        return Ok(());
    }
    require_len(descriptor, 0x20, "ext4 group descriptor checksum")?;
    let expected = read_le_u16(descriptor, 0x1E, "group descriptor checksum")?;
    let mut checksum = super::checksum::crc32c(filesystem.checksum_seed, &group.to_le_bytes());
    checksum = super::checksum::crc32c(checksum, &descriptor[..0x1E]);
    checksum = super::checksum::crc32c(checksum, &[0, 0]);
    checksum = super::checksum::crc32c(checksum, &descriptor[0x20..]);
    if checksum as u16 != expected {
        return Err(JournalError::Invalid(format!(
            "group descriptor {group} checksum mismatch"
        )));
    }
    Ok(())
}

fn verify_bitmap_checksum(
    filesystem: &Ext4Reader,
    group: u32,
    descriptor: &[u8],
    bitmap: &[u8],
    label: &str,
) -> JournalResult<()> {
    if !filesystem.has_metadata_csum {
        return Ok(());
    }
    let (bit_count, low_offset, high_offset) = match label {
        "block" => (u64::from(filesystem.blocks_per_group), 0x18, 0x38),
        "inode" => (u64::from(filesystem.inodes_per_group), 0x1A, 0x3A),
        _ => return Err(JournalError::Invalid("unknown bitmap checksum kind".into())),
    };
    let byte_count = usize::try_from(bit_count / 8)
        .map_err(|_| JournalError::Invalid("bitmap checksum length exceeds usize".into()))?;
    let bytes = bitmap.get(..byte_count).ok_or(JournalError::Truncated {
        context: "ext4 allocation bitmap checksum",
        needed: byte_count,
        available: bitmap.len(),
    })?;
    // Linux ext4 bitmap checksums cover only bitmap bytes. Group identity is
    // included in the group-descriptor checksum, not in either bitmap CRC.
    let checksum = super::checksum::crc32c(filesystem.checksum_seed, bytes);
    let low = u32::from(read_le_u16(descriptor, low_offset, "bitmap checksum low")?);
    let expected = if descriptor.len() >= 64 {
        low | (u32::from(read_le_u16(
            descriptor,
            high_offset,
            "bitmap checksum high",
        )?) << 16)
    } else {
        low
    };
    let actual = if descriptor.len() >= 64 {
        checksum
    } else {
        checksum & 0xFFFF
    };
    if actual != expected {
        return Err(JournalError::Invalid(format!(
            "{label} bitmap checksum mismatch for group {group}"
        )));
    }
    Ok(())
}

fn descriptor_block(
    descriptor: &[u8],
    low_offset: usize,
    high_offset: usize,
    has_64bit: bool,
) -> JournalResult<u64> {
    let low = u64::from(read_le_u32(
        descriptor,
        low_offset,
        "group descriptor bitmap block",
    )?);
    let high = if has_64bit {
        u64::from(read_le_u32(
            descriptor,
            high_offset,
            "64-bit group descriptor bitmap block",
        )?)
    } else {
        0
    };
    let block = low | (high << 32);
    if block == 0 {
        return Err(JournalError::Invalid(
            "group descriptor bitmap block is zero".into(),
        ));
    }
    Ok(block)
}

fn group_flags(descriptor: &[u8]) -> JournalResult<u16> {
    read_le_u16(descriptor, 0x12, "group descriptor flags")
}

fn read_le_u16(data: &[u8], offset: usize, context: &'static str) -> JournalResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| JournalError::Invalid(format!("{context} offset overflows")))?;
    let bytes = data.get(offset..end).ok_or(JournalError::Truncated {
        context,
        needed: end,
        available: data.len(),
    })?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_le_u32(data: &[u8], offset: usize, context: &'static str) -> JournalResult<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| JournalError::Invalid(format!("{context} offset overflows")))?;
    let bytes = data.get(offset..end).ok_or(JournalError::Truncated {
        context,
        needed: end,
        available: data.len(),
    })?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
        JournalError::Invalid(format!("{context} is invalid"))
    })?))
}
