use super::RegistryHiveReader;
use crate::registry::lookup::{
    browser::RegistryBrowserValue,
    parse_value_data,
    types::{NkRecord, RegistryValue},
};

impl RegistryHiveReader<'_> {
    pub(crate) fn read_browser_values(
        &self,
        key: &NkRecord,
    ) -> Result<Vec<RegistryBrowserValue>, String> {
        let mut values = Vec::new();
        for offset in self.value_offsets(key)? {
            let value = self
                .view
                .value_metadata(offset)
                .map_err(|error| error.to_string())?;
            if value.byte_length > 64 * 1024 {
                continue;
            }
            let bytes = self
                .view
                .value_bytes(&value)
                .map_err(|error| error.to_string())?;
            let decoded = parse_value_data(value.type_code, std::borrow::Cow::Borrowed(&bytes))?;
            let (value_type, decoded) = super::super::browser::value_text(&decoded);
            values.push(RegistryBrowserValue {
                name: value.name,
                value_type: value_type.into(),
                decoded,
                raw_hex: hex::encode(bytes),
                cell_offset: offset,
            });
        }
        Ok(values)
    }

    pub(crate) fn read_value(
        &self,
        key: &NkRecord,
        name: &str,
    ) -> Result<Option<RegistryValue>, String> {
        for offset in self.value_offsets(key)? {
            if let Some((candidate, value)) = self.parse_vk(offset)? {
                if candidate.eq_ignore_ascii_case(name) {
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
        self.value_offsets(key)?
            .into_iter()
            .map(|offset| {
                self.parse_vk(offset)
                    .and_then(|value| value.ok_or_else(|| "registry value is missing".into()))
            })
            .collect()
    }

    pub(crate) fn read_raw_vk_data_offsets(&self, key: &NkRecord) -> Result<Vec<u32>, String> {
        self.value_offsets(key)
    }

    pub(crate) fn read_raw_value_bytes(
        &self,
        key: &NkRecord,
        name: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        for offset in self.value_offsets(key)? {
            let value = self
                .view
                .value_metadata(offset)
                .map_err(|error| error.to_string())?;
            if value.name.eq_ignore_ascii_case(name) {
                return self
                    .view
                    .value_bytes(&value)
                    .map(|bytes| Some(bytes.into_owned()))
                    .map_err(|error| error.to_string());
            }
        }
        Ok(None)
    }

    pub(super) fn value_offsets(&self, key: &NkRecord) -> Result<Vec<u32>, String> {
        let key = self
            .view
            .key(key.cell_offset, String::new())
            .map_err(|error| error.to_string())?;
        self.view
            .value_offsets(&key)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn value_data_location(
        &self,
        path: &[&str],
        name: &str,
    ) -> Result<Option<(usize, usize)>, String> {
        let Some(key) = self.navigate_to(path)? else {
            return Ok(None);
        };
        for offset in self.value_offsets(&key)? {
            let value = self
                .view
                .value_metadata(offset)
                .map_err(|error| error.to_string())?;
            if !value.name.eq_ignore_ascii_case(name) {
                continue;
            }
            if value.inline || value.byte_length == 0 {
                return Ok(None);
            }
            if self.view.minor > 3 && value.byte_length > 16_344 {
                return Err("segmented registry value cannot be rewritten as a single cell".into());
            }
            let cell = self
                .view
                .cell(value.data)
                .map_err(|error| error.to_string())?;
            let length = value.byte_length as usize;
            if length > cell.len() {
                return Err("registry value exceeds its data cell".into());
            }
            return Ok(Some((self.abs(value.data)? + 4, length)));
        }
        Ok(None)
    }
}
