use crate::{F2fsError, Result, F2FS_BLOCK_SIZE};

const DENTRY_COUNT: usize = 214;
const DENTRY_BITMAP_BYTES: usize = 27;
const DENTRY_TABLE_OFFSET: usize = 30;
const DENTRY_SIZE: usize = 11;
const NAME_TABLE_OFFSET: usize = DENTRY_TABLE_OFFSET + DENTRY_COUNT * DENTRY_SIZE;
const NAME_SLOT_SIZE: usize = 8;
const DENTRY_LAYOUT_BITS: usize = (DENTRY_SIZE + NAME_SLOT_SIZE) * 8 + 1;

#[derive(Debug, Clone)]
pub(crate) struct DirectoryEntry {
    pub(crate) inode: u32,
    pub(crate) file_type: u8,
    pub(crate) name: String,
}

pub(crate) fn parse_directory_block(bytes: &[u8]) -> Result<Vec<DirectoryEntry>> {
    if bytes.len() != F2FS_BLOCK_SIZE {
        return Err(F2fsError::Invalid(
            "directory block is not one F2FS block".to_string(),
        ));
    }
    parse_directory(
        bytes,
        DENTRY_COUNT,
        DENTRY_BITMAP_BYTES,
        DENTRY_TABLE_OFFSET,
        NAME_TABLE_OFFSET,
    )
}

pub(crate) fn parse_inline_directory(bytes: &[u8]) -> Result<Vec<DirectoryEntry>> {
    let entry_count = bytes
        .len()
        .checked_mul(8)
        .map(|bits| bits / DENTRY_LAYOUT_BITS)
        .ok_or_else(|| F2fsError::Invalid("inline directory capacity overflows".to_string()))?;
    if entry_count == 0 {
        return Err(F2fsError::Invalid(
            "inline directory has no dentry capacity".to_string(),
        ));
    }
    let bitmap_bytes = entry_count.div_ceil(8);
    let table_bytes = entry_count
        .checked_mul(DENTRY_SIZE + NAME_SLOT_SIZE)
        .ok_or_else(|| F2fsError::Invalid("inline directory layout overflows".to_string()))?;
    let reserved_bytes = bytes
        .len()
        .checked_sub(bitmap_bytes + table_bytes)
        .ok_or_else(|| F2fsError::Invalid("inline directory layout underflows".to_string()))?;
    let table_offset = bitmap_bytes + reserved_bytes;
    let name_offset = table_offset + entry_count * DENTRY_SIZE;
    parse_directory(bytes, entry_count, bitmap_bytes, table_offset, name_offset)
}

fn parse_directory(
    bytes: &[u8],
    entry_count: usize,
    bitmap_bytes: usize,
    table_offset: usize,
    name_offset: usize,
) -> Result<Vec<DirectoryEntry>> {
    let mut entries = Vec::new();
    let mut slot = 0usize;
    while slot < entry_count {
        if !bitmap_bit(bytes, bitmap_bytes, slot) {
            slot += 1;
            continue;
        }
        let offset = table_offset + slot * DENTRY_SIZE;
        let inode = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| F2fsError::Invalid("truncated directory inode".to_string()))?,
        );
        let name_length = u16::from_le_bytes([bytes[offset + 8], bytes[offset + 9]]) as usize;
        if name_length == 0 || name_length > 255 {
            return Err(F2fsError::Invalid(format!(
                "directory slot {slot} has invalid name length {name_length}"
            )));
        }
        let slots = name_length.div_ceil(NAME_SLOT_SIZE);
        if slot + slots > entry_count {
            return Err(F2fsError::Invalid(format!(
                "directory name at slot {slot} exceeds its slot table"
            )));
        }
        let name_start = name_offset + slot * NAME_SLOT_SIZE;
        let name_end = name_start
            .checked_add(name_length)
            .ok_or_else(|| F2fsError::Invalid("directory name range overflows".to_string()))?;
        let name_bytes = bytes.get(name_start..name_end).ok_or_else(|| {
            F2fsError::Invalid(format!("directory name at slot {slot} exceeds its block"))
        })?;
        let name = String::from_utf8(name_bytes.to_vec()).map_err(|_| {
            F2fsError::Invalid(format!("directory name at slot {slot} is not valid UTF-8"))
        })?;
        if name != "." && name != ".." {
            entries.push(DirectoryEntry {
                inode,
                file_type: bytes[offset + 10],
                name,
            });
        }
        slot = slot
            .checked_add(slots)
            .ok_or_else(|| F2fsError::Invalid("directory slot index overflows".to_string()))?;
    }
    Ok(entries)
}

fn bitmap_bit(bytes: &[u8], bitmap_bytes: usize, index: usize) -> bool {
    index / 8 < bitmap_bytes && bytes[index / 8] & (1 << (index % 8)) != 0
}
