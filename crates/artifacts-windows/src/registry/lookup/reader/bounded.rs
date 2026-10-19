use super::{cells::allocated_cell_length, RegistryHiveReader};
use crate::registry::{
    lookup::{
        types::{NkRecord, RegistryValue, VK_SIGNATURE},
        utf16::{decode_name, read_u16, read_u32},
    },
    RegistryError,
};

impl RegistryHiveReader<'_> {
    /// Inspect the VK name/declared length before materializing any data payload.
    pub(crate) fn read_bounded_value(
        &self,
        key: &NkRecord,
        requested: &str,
        max_bytes: usize,
    ) -> Result<Option<RegistryValue>, RegistryError> {
        if key.num_values > 256 {
            return Err(RegistryError::other("hardware key value limit exceeded"));
        }
        for offset in self.value_offsets(key)? {
            let absolute = self.abs(offset)?;
            let length = allocated_cell_length(self, absolute, "hardware value")?;
            self.require(absolute, length)?;
            if length < 24 || self.bytes.get(absolute + 4..absolute + 6) != Some(VK_SIGNATURE) {
                return Err(RegistryError::other("invalid hardware VK cell"));
            }
            let name_length = read_u16(self.bytes, absolute + 6)? as usize;
            if name_length > 1024 || name_length > length - 24 {
                return Err(RegistryError::other("invalid hardware value name"));
            }
            let flags = read_u16(self.bytes, absolute + 0x14)?;
            let name = decode_name(
                &self.bytes[absolute + 24..absolute + 24 + name_length],
                flags & 1 != 0,
            )?;
            if !name.eq_ignore_ascii_case(requested) {
                continue;
            }
            let data_length = (read_u32(self.bytes, absolute + 8)? & 0x7fff_ffff) as usize;
            if data_length > max_bytes {
                return Err(RegistryError::other("hardware value byte limit exceeded"));
            }
            return Ok(self.parse_vk(offset)?.map(|(_, value)| value));
        }
        Ok(None)
    }
}
