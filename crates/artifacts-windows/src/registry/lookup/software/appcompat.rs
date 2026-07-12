use super::super::{
    windows_filetime_to_rfc3339, AppCompatLayerEntry, RegistryHiveReader, RegistryValue,
};
use crate::registry::RegistryError;

pub fn extract_appcompat_layers_from_software_hive(
    bytes: &[u8],
    hive_path: &str,
) -> Result<Vec<AppCompatLayerEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let key_path: &[&str] = &[
        "Microsoft",
        "Windows NT",
        "CurrentVersion",
        "AppCompatFlags",
        "Layers",
    ];
    let node = match hive.navigate_to(key_path) {
        Ok(Some(node)) => node,
        Ok(None) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let last_write = node.last_write_time.and_then(windows_filetime_to_rfc3339);
    let source_key_path = key_path.join("\\");
    let mut entries = Vec::new();
    for (name, value) in hive.read_all_values_from_nk(&node)? {
        if let RegistryValue::String(layer_string) = value {
            if !name.trim().is_empty() || !layer_string.trim().is_empty() {
                entries.push(AppCompatLayerEntry {
                    executable_path: name,
                    layer_string,
                    source_hive_path: hive_path.to_string(),
                    source_key_path: source_key_path.clone(),
                    last_write: last_write.clone(),
                });
            }
        }
    }
    Ok(entries)
}
