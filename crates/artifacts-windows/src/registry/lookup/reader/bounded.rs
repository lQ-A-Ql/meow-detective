use super::RegistryHiveReader;
use crate::registry::{
    lookup::types::{NkRecord, RegistryValue},
    RegistryError,
};

impl RegistryHiveReader<'_> {
    pub(crate) fn read_bounded_value(
        &self,
        key: &NkRecord,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<RegistryValue>, RegistryError> {
        if key.num_values > 256 {
            return Err(RegistryError::other("hardware key value limit exceeded"));
        }
        for offset in self.value_offsets(key)? {
            let value = self.view.value_metadata(offset)?;
            if value.name.len() > 1024 {
                return Err(RegistryError::other("invalid hardware value name"));
            }
            if !value.name.eq_ignore_ascii_case(name) {
                continue;
            }
            if value.byte_length as usize > max_bytes {
                return Err(RegistryError::other("hardware value byte limit exceeded"));
            }
            return Ok(self.parse_vk(offset)?.map(|(_, value)| value));
        }
        Ok(None)
    }
}
