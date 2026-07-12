use super::types::{BASE_BLOCK_SIZE, HBIN_MAGIC, INVALID_OFFSET};
use super::utf16::read_u32;

mod cells;
mod class_name;
mod navigation;
mod values;

pub(crate) struct RegistryHiveReader<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) root_cell_offset: u32,
}

impl<'a> RegistryHiveReader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < BASE_BLOCK_SIZE {
            return Err("registry hive shorter than base block".to_string());
        }
        if bytes.get(0..4) != Some(b"regf") {
            return Err("not a valid registry hive".to_string());
        }
        if bytes.len() < BASE_BLOCK_SIZE + 32 {
            return Err("registry hive too short for first hbin header".to_string());
        }
        if bytes.get(BASE_BLOCK_SIZE..BASE_BLOCK_SIZE + 4) != Some(HBIN_MAGIC) {
            return Err("first hbin header missing 'hbin' magic".to_string());
        }
        let hbin_size = read_u32(bytes, BASE_BLOCK_SIZE + 8)? as usize;
        if hbin_size == 0 || !hbin_size.is_multiple_of(4096) {
            return Err(format!(
                "first hbin size {hbin_size:#x} is not a valid page multiple"
            ));
        }
        let root_cell_offset = read_u32(bytes, 0x24)?;
        if root_cell_offset >= hbin_size as u32 {
            return Err(format!(
                "root cell offset {root_cell_offset:#x} exceeds first hbin size {hbin_size:#x}"
            ));
        }
        Ok(Self {
            bytes,
            root_cell_offset,
        })
    }

    pub(crate) fn abs(&self, hive_offset: u32) -> Result<usize, String> {
        if hive_offset == INVALID_OFFSET {
            return Err("invalid registry offset".to_string());
        }
        BASE_BLOCK_SIZE
            .checked_add(hive_offset as usize)
            .ok_or_else(|| "registry offset overflow".to_string())
            .and_then(|absolute| {
                (absolute < self.bytes.len())
                    .then_some(absolute)
                    .ok_or_else(|| format!("registry offset {hive_offset:#x} out of bounds"))
            })
    }

    pub(crate) fn require(&self, absolute: usize, length: usize) -> Result<(), String> {
        absolute
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .map(|_| ())
            .ok_or_else(|| format!("registry range {absolute:#x}+{length:#x} out of bounds"))
    }
}
