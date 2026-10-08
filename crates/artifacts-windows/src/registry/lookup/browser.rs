use super::reader::RegistryHiveReader;
use super::types::RegistryValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryBrowserValue {
    pub name: String,
    pub value_type: String,
    pub decoded: String,
    pub raw_hex: String,
    pub cell_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryBrowserKey {
    pub path: String,
    pub name: String,
    pub cell_offset: u32,
    pub last_write_time: Option<u64>,
    pub subkey_count: u32,
    pub value_count: u32,
    pub values: Vec<RegistryBrowserValue>,
    pub subkeys: Vec<String>,
}

pub fn browse_registry_hive(bytes: &[u8], key_path: &str) -> Result<RegistryBrowserKey, String> {
    let reader = RegistryHiveReader::new(bytes)?;
    let segments: Vec<&str> = key_path
        .split('\\')
        .filter(|segment| !segment.is_empty())
        .collect();
    let key = reader
        .navigate_to(&segments)?
        .ok_or_else(|| format!("registry key not found: {key_path}"))?;
    let values = reader.read_browser_values(&key)?;
    let subkeys = reader.read_subkey_names_from_nk(&key)?;
    Ok(RegistryBrowserKey {
        path: if key_path.is_empty() {
            "\\".to_string()
        } else {
            key_path.to_string()
        },
        name: key.name,
        cell_offset: key.cell_offset,
        last_write_time: key.last_write_time,
        subkey_count: key.num_subkeys,
        value_count: key.num_values,
        values,
        subkeys,
    })
}

pub(crate) fn value_text(value: &RegistryValue) -> (&'static str, String) {
    match value {
        RegistryValue::String(value) => ("string", value.clone()),
        RegistryValue::Dword(value) => ("dword", value.to_string()),
        RegistryValue::Qword(value) => ("qword", value.to_string()),
        RegistryValue::MultiString(values) => ("multiString", values.join("\\0")),
        RegistryValue::Binary(value) => ("binary", format!("{} bytes", value.len())),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/registry/browser.rs"]
mod tests;
