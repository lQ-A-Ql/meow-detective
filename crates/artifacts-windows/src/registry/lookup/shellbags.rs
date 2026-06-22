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
mod tests {
    use super::super::test_common::*;
    use super::*;

    fn make_pidl_blob_with_path(path: &str) -> Vec<u8> {
        let mut blob = vec![0x14, 0x00, 0x1f, 0x00, 0xe0, 0x00]; // synthetic PIDL prefix
        let utf16: Vec<u8> = path.encode_utf16().flat_map(u16::to_le_bytes).collect();
        blob.extend_from_slice(&utf16);
        blob.extend_from_slice(&[0x00, 0x00]); // null terminator
        blob.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // trailing padding
        blob
    }

    #[test]
    fn extract_shellbags_from_fixture() {
        let mut data = empty_hive("USRCLASS");
        // Local Settings\Software\Microsoft\Windows\Shell\BagMRU\0\0
        write_nk(
            &mut data,
            0x20,
            "USRCLASS",
            &[("Local Settings", 0x200)],
            &[],
        );
        write_nk(
            &mut data,
            0x200,
            "Local Settings",
            &[("Software", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x300, "Software", &[("Microsoft", 0x400)], &[]);
        write_nk(&mut data, 0x400, "Microsoft", &[("Windows", 0x500)], &[]);
        write_nk(&mut data, 0x500, "Windows", &[("Shell", 0x600)], &[]);
        write_nk(&mut data, 0x600, "Shell", &[("BagMRU", 0x700)], &[]);
        write_nk(&mut data, 0x700, "BagMRU", &[("0", 0x800)], &[0x900]);
        write_dword_value(&mut data, 0x900, "NodeSlot", 7);
        write_nk(&mut data, 0x800, "0", &[("0", 0xa00)], &[]);
        write_nk(&mut data, 0xa00, "0", &[], &[0xb00, 0xc00]);
        write_dword_value(&mut data, 0xc00, "NodeSlot", 7);
        let pidl = make_pidl_blob_with_path("C:\\Users\\Test\\Documents");
        write_binary_value(&mut data, 0xb00, "", &pidl, 0x4000);

        let entries =
            extract_shellbags_from_usrclass_hive(&data, "Users/Test/UsrClass.dat").unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.path, "C:\\Users\\Test\\Documents");
        assert_eq!(entry.raw_pidl_hex, hex::encode(&pidl));
        assert_eq!(entry.node_slot, Some(7));
        assert!(entry.source_key_path.ends_with("BagMRU\\0\\0"));
    }

    #[test]
    fn extract_shellbags_returns_empty_when_bagmru_missing() {
        let data = empty_hive("USRCLASS");
        let entries =
            extract_shellbags_from_usrclass_hive(&data, "Users/Test/UsrClass.dat").unwrap();
        assert!(entries.is_empty());
    }
}
