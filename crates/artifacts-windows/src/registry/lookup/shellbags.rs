use super::ntuser::decode_pidl_path;
use super::reader::RegistryHiveReader;
use super::types::{NkRecord, RegistryValue, ShellbagEntry};
use super::windows_filetime_to_rfc3339;
use crate::registry::RegistryError;

const BAGMRU_MAX_DEPTH: usize = 16;
const BAGMRU_PATH: &[&str] = &[
    "Local Settings",
    "Software",
    "Microsoft",
    "Windows",
    "Shell",
    "BagMRU",
];

/// Extract Shellbag entries from the `BagMRU` tree in a `UsrClass.dat` hive.
pub fn extract_shellbags_from_usrclass_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<ShellbagEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let root_nk = match hive.navigate_to(BAGMRU_PATH) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Ok(Vec::new()),
        Err(err) => return Err(RegistryError::other(format!("BagMRU parse error: {err}"))),
    };
    let root_path = BAGMRU_PATH.join("\\");
    Ok(extract_bagmru_node(&hive, &root_nk, &root_path, 0))
}

fn extract_bagmru_node(
    hive: &RegistryHiveReader<'_>,
    nk: &NkRecord,
    key_path: &str,
    depth: usize,
) -> Vec<ShellbagEntry> {
    if depth > BAGMRU_MAX_DEPTH {
        return Vec::new();
    }
    let subkeys = match hive.read_subkeys_from_nk(nk) {
        Ok(subkeys) => subkeys,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    for (name, child_nk) in subkeys {
        let child_path = format!("{}\\{}", key_path, name);
        let child_slot = read_node_slot(hive, &child_nk);
        let child_last_write = child_nk
            .last_write_time
            .and_then(windows_filetime_to_rfc3339);

        // Each numbered BagMRU child key stores its PIDL in the default
        // (unnamed) value.
        if let Ok(Some(RegistryValue::Binary(data))) = hive.read_value(&child_nk, "") {
            let path = decode_pidl_path(&data).unwrap_or_default();
            entries.push(ShellbagEntry {
                path,
                raw_pidl_hex: hex::encode(&data),
                node_slot: child_slot,
                source_key_path: child_path.clone(),
                last_write: child_last_write.clone(),
            });
        }

        entries.extend(extract_bagmru_node(hive, &child_nk, &child_path, depth + 1));
    }
    entries
}

fn read_node_slot(hive: &RegistryHiveReader<'_>, nk: &NkRecord) -> Option<u32> {
    match hive.read_value(nk, "NodeSlot") {
        Ok(Some(RegistryValue::Dword(v))) => Some(v),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/registry/lookup/shellbags.rs"]
mod tests;
