use super::RegistryHiveReader;
use crate::registry::lookup::types::{INVALID_OFFSET, MAX_KEY_LOOKUP_DEPTH, NK_SIGNATURE};
use crate::registry::lookup::utf16::{read_i32, read_u16, read_u32};

impl RegistryHiveReader<'_> {
    pub(crate) fn read_class_name_at(&self, key_path: &[&str]) -> Result<Option<String>, String> {
        if key_path.len() > MAX_KEY_LOOKUP_DEPTH {
            return Err(format!(
                "registry key path depth {} exceeds limit {}",
                key_path.len(),
                MAX_KEY_LOOKUP_DEPTH
            ));
        }
        let mut offset = self.root_cell_offset;
        let mut key = self.parse_nk(offset)?;
        for segment in key_path {
            let Some(next_offset) = self.find_subkey_offset(&key, segment)? else {
                return Ok(None);
            };
            offset = next_offset;
            key = self.parse_nk(offset)?;
        }
        self.read_nk_class_name(offset)
    }

    fn read_nk_class_name(&self, offset: u32) -> Result<Option<String>, String> {
        let absolute = self.abs(offset)?;
        if read_i32(self.bytes, absolute)? >= 0 {
            return Err(format!("NK cell at {offset:#x} is free"));
        }
        if self.bytes.get(absolute + 4..absolute + 6) != Some(NK_SIGNATURE) {
            return Err("class name read target is not an NK cell".to_string());
        }
        let length = read_u16(self.bytes, absolute + 0x4e)? as usize;
        if length == 0 {
            return Ok(None);
        }
        if length > 4096 {
            return Err(format!(
                "class name length {length} at {offset:#x} is implausibly large"
            ));
        }
        let data = self.read_class_data(absolute, length)?;
        decode_class_name(data)
    }

    fn read_class_data(&self, key_absolute: usize, length: usize) -> Result<&[u8], String> {
        let data_offset = read_u32(self.bytes, key_absolute + 0x34)?;
        if data_offset == INVALID_OFFSET || data_offset == 0 {
            let name_length = read_u16(self.bytes, key_absolute + 0x4c)? as usize;
            let start = key_absolute + 0x50 + name_length;
            self.require(start, length)?;
            return Ok(&self.bytes[start..start + length]);
        }
        let absolute = self.abs(data_offset)?;
        let cell_size = read_i32(self.bytes, absolute)?;
        if cell_size >= 0 {
            return Err(format!("class name data cell at {data_offset:#x} is free"));
        }
        let cell_length = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid class name data cell size".to_string())?
            as usize;
        if length > cell_length.saturating_sub(4) {
            return Err(format!(
                "class name at {data_offset:#x} length {length:#x} exceeds cell"
            ));
        }
        let start = absolute + 4;
        self.require(start, length)?;
        Ok(&self.bytes[start..start + length])
    }
}

fn decode_class_name(data: &[u8]) -> Result<Option<String>, String> {
    if data.len() < 2 || !data.len().is_multiple_of(2) {
        return Ok(None);
    }
    let units = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    let value = String::from_utf16_lossy(&units);
    let trimmed = value.trim_end_matches('\0');
    Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
}
