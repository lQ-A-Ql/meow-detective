use super::RegistryHiveReader;
use crate::registry::lookup::parse_value_data;
use crate::registry::lookup::types::{NkRecord, RegistryValue, NK_SIGNATURE, VK_SIGNATURE};
use crate::registry::lookup::utf16::{decode_name, read_i32, read_le_array, read_u16, read_u32};

impl RegistryHiveReader<'_> {
    pub(crate) fn parse_nk(&self, cell_offset: u32) -> Result<NkRecord, String> {
        let absolute = self.abs(cell_offset)?;
        let cell_length = allocated_cell_length(self, absolute, "registry cell")?;
        self.require(absolute, cell_length)?;
        if self.bytes.get(absolute + 4..absolute + 6) != Some(NK_SIGNATURE) {
            return Err(format!("cell at {cell_offset:#x} is not nk"));
        }
        let flags = read_u16(self.bytes, absolute + 6)?;
        let name_length = read_u16(self.bytes, absolute + 0x4c)? as usize;
        let name_start = absolute + 0x50;
        self.require(name_start, name_length)?;
        Ok(NkRecord {
            name: decode_name(
                &self.bytes[name_start..name_start + name_length],
                flags & 0x20 != 0,
            )?,
            last_write_time: read_le_array::<8>(&self.bytes[absolute + 0x08..])
                .map(u64::from_le_bytes)
                .filter(|value| *value != 0),
            num_subkeys: read_u32(self.bytes, absolute + 0x18)?,
            subkeys_list_offset: read_u32(self.bytes, absolute + 0x20)?,
            num_values: read_u32(self.bytes, absolute + 0x28)?,
            values_list_offset: read_u32(self.bytes, absolute + 0x2c)?,
        })
    }

    pub(crate) fn parse_vk(
        &self,
        cell_offset: u32,
    ) -> Result<Option<(String, RegistryValue)>, String> {
        let absolute = self.abs(cell_offset)?;
        let cell_length = allocated_cell_length(self, absolute, "registry value cell")?;
        self.require(absolute, cell_length)?;
        if self.bytes.get(absolute + 4..absolute + 6) != Some(VK_SIGNATURE) {
            return Ok(None);
        }
        let name_length = read_u16(self.bytes, absolute + 6)? as usize;
        let data_length_raw = read_u32(self.bytes, absolute + 8)?;
        let data_offset = read_u32(self.bytes, absolute + 0x0c)?;
        let data_type = read_u32(self.bytes, absolute + 0x10)?;
        let flags = read_u16(self.bytes, absolute + 0x14)?;
        let name_start = absolute + 0x18;
        self.require(name_start, name_length)?;
        let name = decode_name(
            &self.bytes[name_start..name_start + name_length],
            flags & 0x01 != 0,
        )?;
        let data = self.read_vk_data(cell_offset, data_length_raw, data_offset)?;
        Ok(Some((name, parse_value_data(data_type, &data)?)))
    }

    fn read_vk_data(
        &self,
        cell_offset: u32,
        length_raw: u32,
        data_offset: u32,
    ) -> Result<Vec<u8>, String> {
        let length = (length_raw & 0x7fff_ffff) as usize;
        if length_raw & 0x8000_0000 != 0 {
            if length > 4 {
                return Err(format!(
                    "inline value at {cell_offset:#x} length {length:#x} exceeds 4 bytes"
                ));
            }
            return Ok(data_offset.to_le_bytes()[..length].to_vec());
        }
        if length == 0 {
            return Ok(Vec::new());
        }
        let absolute = self.abs(data_offset)?;
        let cell_length = allocated_cell_length(self, absolute, "registry value data cell")?;
        let data_start = absolute + 4;
        self.require(data_start, length)?;
        if length > cell_length.saturating_sub(4) {
            return Err(format!(
                "value data at {data_offset:#x} length {length:#x} exceeds cell"
            ));
        }
        Ok(self.bytes[data_start..data_start + length].to_vec())
    }
}

pub(super) fn allocated_cell_length(
    reader: &RegistryHiveReader<'_>,
    absolute: usize,
    kind: &str,
) -> Result<usize, String> {
    let size = read_i32(reader.bytes, absolute)?;
    if size >= 0 {
        return Err(format!("{kind} at {absolute:#x} is free"));
    }
    size.checked_abs()
        .ok_or_else(|| format!("invalid {kind} size"))
        .map(|value| value as usize)
}
