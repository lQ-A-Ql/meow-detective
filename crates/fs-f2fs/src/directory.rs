use crate::{F2fsError, Result, F2FS_BLOCK_SIZE};

const DENTRY_COUNT: usize = 214;
const DENTRY_BITMAP_BYTES: usize = 27;
const DENTRY_TABLE_OFFSET: usize = 30;
const DENTRY_SIZE: usize = 11;
const NAME_TABLE_OFFSET: usize = DENTRY_TABLE_OFFSET + DENTRY_COUNT * DENTRY_SIZE;
const NAME_SLOT_SIZE: usize = 8;

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
    let mut entries = Vec::new();
    let mut slot = 0usize;
    while slot < DENTRY_COUNT {
        if !bitmap_bit(bytes, slot) {
            slot += 1;
            continue;
        }
        let offset = DENTRY_TABLE_OFFSET + slot * DENTRY_SIZE;
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
        let name_start = NAME_TABLE_OFFSET + slot * NAME_SLOT_SIZE;
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

fn bitmap_bit(bytes: &[u8], index: usize) -> bool {
    index < DENTRY_COUNT
        && index / 8 < DENTRY_BITMAP_BYTES
        && bytes[index / 8] & (1 << (index % 8)) != 0
}
