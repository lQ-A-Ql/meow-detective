use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;
use crate::registry::RegistryError;

/// Extract program compatibility / elevation flags from
/// `Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers` in
/// an NTUSER.DAT hive.
pub fn extract_appcompat_layers_from_ntuser_hive(
    bytes: &[u8],
    hive_path: &str,
) -> Result<Vec<AppCompatLayerEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let key_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows NT",
        "CurrentVersion",
        "AppCompatFlags",
        "Layers",
    ];

    let nk = match hive.navigate_to(key_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };

    let last_write = nk.last_write_time.and_then(windows_filetime_to_rfc3339);
    let source_key_path = key_path.join("\\");
    let values = hive.read_all_values_from_nk(&nk)?;
    let mut entries = Vec::new();

    for (name, value) in values {
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
