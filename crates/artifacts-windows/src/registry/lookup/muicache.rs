use super::reader::RegistryHiveReader;
use super::types::{MuiCacheEntry, RegistryValue};
use super::windows_filetime_to_rfc3339;
use crate::registry::RegistryError;

const MUICACHE_PATH: &[&str] = &[
    "Local Settings",
    "Software",
    "Microsoft",
    "Windows",
    "Shell",
    "MuiCache",
];

/// Extract friendly program names from the `MuiCache` key in a `UsrClass.dat` hive.
pub fn extract_muicache_from_usrclass_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<MuiCacheEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let nk = match hive.navigate_to(MUICACHE_PATH) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Ok(Vec::new()),
        Err(err) => return Err(RegistryError::other(format!("MuiCache parse error: {err}"))),
    };
    let source_key_path = MUICACHE_PATH.join("\\");
    let last_write = nk.last_write_time.and_then(windows_filetime_to_rfc3339);
    let values = match hive.read_all_values_from_nk(&nk) {
        Ok(values) => values,
        Err(err) => {
            return Err(RegistryError::other(format!(
                "MuiCache values error: {err}"
            )))
        }
    };

    let mut entries = Vec::new();
    for (value_name, value) in values {
        if let RegistryValue::String(friendly_name) = value {
            entries.push(MuiCacheEntry {
                program_path: value_name,
                friendly_name,
                source_key_path: source_key_path.clone(),
                last_write: last_write.clone(),
            });
        }
    }
    Ok(entries)
}

#[cfg(test)]
#[path = "../../../tests/unit/registry/lookup/muicache.rs"]
mod tests;
