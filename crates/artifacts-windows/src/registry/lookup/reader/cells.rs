use super::RegistryHiveReader;
use crate::registry::{
    lookup::{
        parse_value_data,
        types::{NkRecord, RegistryValue},
    },
    reader::BrowserKey,
};

pub(super) fn node(key: BrowserKey) -> NkRecord {
    NkRecord {
        cell_offset: key.offset,
        name: key.name,
        last_write_time: key.filetime,
        num_subkeys: key.subkey_count,
        num_values: key.value_count,
    }
}

impl RegistryHiveReader<'_> {
    pub(crate) fn parse_nk(&self, offset: u32) -> Result<NkRecord, String> {
        self.view
            .key(offset, String::new())
            .map(node)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn parse_vk(&self, offset: u32) -> Result<Option<(String, RegistryValue)>, String> {
        let value = self
            .view
            .value_metadata(offset)
            .map_err(|error| error.to_string())?;
        let bytes = self
            .view
            .value_bytes(&value)
            .map_err(|error| error.to_string())?;
        Ok(Some((
            value.name,
            parse_value_data(value.type_code, bytes)?,
        )))
    }
}
