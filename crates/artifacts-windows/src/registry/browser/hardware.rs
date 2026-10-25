use super::{BrowserError, HiveSnapshot};
use crate::registry::{
    lookup::hardware::{extract_system_hardware, RegistryHardwareInfo},
    RegistryError,
};

impl HiveSnapshot {
    /// Analyze exactly this session's immutable base/recovered evidence bytes.
    pub fn hardware_inventory(&self) -> Result<RegistryHardwareInfo, RegistryError> {
        extract_system_hardware(self, |path| self.read_smbios_cache(path))
    }

    fn read_smbios_cache(&self, path: &str) -> Result<Option<Vec<u8>>, RegistryError> {
        let key = match self.resolve(path) {
            Ok(key) => key,
            Err(BrowserError::NotFound) => return Ok(None),
            Err(error) => return Err(RegistryError::other(error.to_string())),
        };
        if key.value_count > 256 {
            return Err(RegistryError::other("SMBIOS key value limit exceeded"));
        }
        let page = self
            .values(&key, 0, 256)
            .map_err(|error| RegistryError::other(error.to_string()))?;
        for (_, value) in page.items {
            let value = value.map_err(|error| RegistryError::other(error.to_string()))?;
            if !value.name.eq_ignore_ascii_case("SMBiosData") {
                continue;
            }
            if value.type_code != 3 || value.byte_length > 8 * 1024 * 1024 + 8 {
                return Err(RegistryError::other(
                    "SMBiosData type or byte limit is invalid",
                ));
            }
            let mut bytes = Vec::with_capacity(value.byte_length as usize);
            while bytes.len() < value.byte_length as usize {
                let length = (value.byte_length as usize - bytes.len()).min(65536);
                let range = self
                    .value_range(&value, bytes.len() as u64, length)
                    .map_err(|error| RegistryError::other(error.to_string()))?;
                if range.bytes.len() != length {
                    return Err(RegistryError::other("incomplete SMBIOS range"));
                }
                bytes.extend(range.bytes);
            }
            return Ok(Some(bytes));
        }
        Ok(None)
    }
}
