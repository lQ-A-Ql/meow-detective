use super::cells::allocated_cell_length;
use super::RegistryHiveReader;
use crate::registry::lookup::types::{NkRecord, RegistryValue, INVALID_OFFSET, VK_SIGNATURE};
use crate::registry::lookup::utf16::{decode_name, read_i32, read_u16, read_u32};

impl RegistryHiveReader<'_> {
    pub(crate) fn read_value(
        &self,
        key: &NkRecord,
        value_name: &str,
    ) -> Result<Option<RegistryValue>, String> {
        for offset in self.value_offsets(key)? {
            if let Some((name, value)) = self.parse_vk(offset)? {
                if name.eq_ignore_ascii_case(value_name) {
                    return Ok(Some(value));
                }
            }
        }
        Ok(None)
    }

    pub(crate) fn read_all_values_from_nk(
        &self,
        key: &NkRecord,
    ) -> Result<Vec<(String, RegistryValue)>, String> {
        let mut values = Vec::with_capacity(key.num_values as usize);
        for offset in self.value_offsets(key)? {
            if let Some(value) = self.parse_vk(offset)? {
                values.push(value);
            }
        }
        Ok(values)
    }

    pub(crate) fn read_raw_vk_data_offsets(&self, key: &NkRecord) -> Result<Vec<u32>, String> {
        self.raw_value_offsets(key)
    }

    pub(crate) fn read_raw_value_bytes(
        &self,
        key: &NkRecord,
        value_name: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        for offset in self.raw_value_offsets(key)? {
            if let Some((name, data)) = self.read_raw_vk(offset)? {
                if name.eq_ignore_ascii_case(value_name) {
                    return Ok(Some(data));
                }
            }
        }
        Ok(None)
    }

    fn value_offsets(&self, key: &NkRecord) -> Result<Vec<u32>, String> {
        if key.num_values == 0 || key.values_list_offset == INVALID_OFFSET {
            return Ok(Vec::new());
        }
        let absolute = self.abs(key.values_list_offset)?;
        let cell_length = allocated_cell_length(self, absolute, "registry value list cell")?;
        self.require(absolute, cell_length)?;
        let list_length = (key.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "registry value list size overflow".to_string())?;
        if list_length > cell_length.saturating_sub(4) {
            return Err(format!(
                "value list at {:#x} length {list_length:#x} exceeds cell",
                key.values_list_offset
            ));
        }
        let list_start = absolute + 4;
        self.require(list_start, list_length)?;
        (0..key.num_values as usize)
            .map(|index| {
                read_u32(self.bytes, list_start + index * 4).map_err(|error| error.to_string())
            })
            .filter(|offset| !matches!(offset, Ok(INVALID_OFFSET)))
            .collect()
    }

    fn raw_value_offsets(&self, key: &NkRecord) -> Result<Vec<u32>, String> {
        if key.num_values == 0 || key.values_list_offset == INVALID_OFFSET {
            return Ok(Vec::new());
        }
        let absolute = self.abs(key.values_list_offset)?;
        let cell_size = read_i32(self.bytes, absolute)?;
        if cell_size >= 0 {
            return Ok(Vec::new());
        }
        let cell_length = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid value list cell".to_string())?
            as usize;
        self.require(absolute, cell_length)?;
        let list_length = (key.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "overflow".to_string())?;
        if list_length > cell_length.saturating_sub(4) {
            return Ok(Vec::new());
        }
        let list_start = absolute + 4;
        self.require(list_start, list_length)?;
        (0..key.num_values as usize)
            .map(|index| {
                read_u32(self.bytes, list_start + index * 4).map_err(|error| error.to_string())
            })
            .filter(|offset| !matches!(offset, Ok(INVALID_OFFSET)))
            .collect()
    }

    /// Locate a value's raw data payload as an absolute file offset and
    /// length. Inline values (≤4 bytes stored in the VK cell itself) return
    /// `None` — they have no writable data cell.
    pub(crate) fn value_data_location(
        &self,
        key_path: &[&str],
        value_name: &str,
    ) -> Result<Option<(usize, usize)>, String> {
        let Some(key) = self.navigate_to(key_path)? else {
            return Ok(None);
        };
        for offset in self.raw_value_offsets(&key)? {
            let absolute = self.abs(offset)?;
            if absolute + 0x18 > self.bytes.len()
                || &self.bytes[absolute + 4..absolute + 6] != VK_SIGNATURE
            {
                continue;
            }
            let name_length = read_u16(self.bytes, absolute + 6)? as usize;
            let data_length_raw = read_u32(self.bytes, absolute + 8)?;
            let data_offset = read_u32(self.bytes, absolute + 0x0c)?;
            let flags = read_u16(self.bytes, absolute + 0x14)?;
            let name_start = absolute + 0x18;
            self.require(name_start, name_length)?;
            let name = decode_name(
                &self.bytes[name_start..name_start + name_length],
                flags & 0x01 != 0,
            )?;
            if !name.eq_ignore_ascii_case(value_name) {
                continue;
            }
            if data_length_raw & 0x8000_0000 != 0 {
                return Ok(None);
            }
            let length = (data_length_raw & 0x7fff_ffff) as usize;
            let data_start = self.abs(data_offset)? + 4;
            self.require(data_start, length)?;
            return Ok(Some((data_start, length)));
        }
        Ok(None)
    }

    fn read_raw_vk(&self, offset: u32) -> Result<Option<(String, Vec<u8>)>, String> {
        let absolute = self.abs(offset)?;
        if absolute + 0x18 > self.bytes.len()
            || &self.bytes[absolute + 4..absolute + 6] != VK_SIGNATURE
        {
            return Ok(None);
        }
        let name_length = read_u16(self.bytes, absolute + 6)? as usize;
        let data_length_raw = read_u32(self.bytes, absolute + 8)?;
        let data_offset = read_u32(self.bytes, absolute + 0x0c)?;
        let flags = read_u16(self.bytes, absolute + 0x14)?;
        let name_start = absolute + 0x18;
        self.require(name_start, name_length)?;
        let name = decode_name(
            &self.bytes[name_start..name_start + name_length],
            flags & 0x01 != 0,
        )?;
        let Some(data) = self.read_raw_data(data_length_raw, data_offset)? else {
            return Ok(None);
        };
        Ok(Some((name, data)))
    }

    fn read_raw_data(&self, length_raw: u32, data_offset: u32) -> Result<Option<Vec<u8>>, String> {
        let length = (length_raw & 0x7fff_ffff) as usize;
        if length_raw & 0x8000_0000 != 0 {
            if length > 4 {
                return Err("inline value >4 bytes".to_string());
            }
            return Ok(Some(data_offset.to_le_bytes()[..length].to_vec()));
        }
        if length == 0 {
            return Ok(Some(data_offset.to_le_bytes().to_vec()));
        }
        let absolute = self.abs(data_offset)?;
        let cell_size = read_i32(self.bytes, absolute)?;
        if cell_size >= 0 {
            return Ok(None);
        }
        let cell_length = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid data cell".to_string())? as usize;
        let data_start = absolute + 4;
        if length > cell_length.saturating_sub(4) {
            return Ok(None);
        }
        self.require(data_start, length)?;
        Ok(Some(self.bytes[data_start..data_start + length].to_vec()))
    }
}
