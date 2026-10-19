use std::collections::BTreeMap;

pub(super) const MAX_DEVICES: usize = 1024;
pub(super) const MAX_TEXT_BYTES: usize = 512;

#[derive(Debug, Clone, Default)]
pub struct RegistryHardwareInfo {
    pub devices: Vec<RegistryHardwareRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RegistryHardwareRecord {
    pub kind: String,
    pub device_id: String,
    pub name: String,
    pub manufacturer: Option<String>,
    /// Machine-readable property names; numeric values retain exact decimal text.
    pub properties: BTreeMap<String, String>,
    pub key_path: String,
    pub value_name: Option<String>,
    /// Offset in SMBiosData, including its eight-byte Windows header.
    pub source_offset: Option<u32>,
}

impl RegistryHardwareRecord {
    pub(super) fn new(kind: &str, device_id: String, key_path: String) -> Self {
        Self {
            kind: kind.into(),
            device_id,
            name: String::new(),
            manufacturer: None,
            properties: BTreeMap::new(),
            key_path,
            value_name: None,
            source_offset: None,
        }
    }

    pub(super) fn property(&mut self, key: &str, value: Option<impl ToString>) {
        if let Some(value) = value {
            let value = value.to_string();
            if !value.trim().is_empty() && value.len() <= MAX_TEXT_BYTES {
                self.properties.insert(key.into(), value.trim().into());
            }
        }
    }
}
