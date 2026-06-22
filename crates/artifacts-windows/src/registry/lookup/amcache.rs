use super::reader::RegistryHiveReader;
use super::types::{AmcacheApplicationEntry, AmcacheApplicationFileEntry, RegistryValue};
use super::windows_filetime_to_rfc3339;
use crate::registry::RegistryError;

/// Aggregated Amcache.hve extraction results.
#[derive(Debug, Clone, Default)]
pub struct AmcacheInfo {
    pub applications: Vec<AmcacheApplicationEntry>,
    pub application_files: Vec<AmcacheApplicationFileEntry>,
    pub warnings: Vec<String>,
}

/// Parse `Amcache.hve` and extract installed application inventory and
/// application-file execution evidence.
pub fn extract_amcache_entries(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<AmcacheInfo, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = AmcacheInfo::default();

    extract_applications(&hive, &mut info);
    extract_application_files(&hive, &mut info);

    Ok(info)
}

fn extract_applications(hive: &RegistryHiveReader<'_>, info: &mut AmcacheInfo) {
    let Some(inventory) = navigate_key(hive, &["InventoryApplication"], info) else {
        return;
    };

    let subkeys = match hive.read_subkeys_from_nk(&inventory) {
        Ok(subkeys) => subkeys,
        Err(err) => {
            info.warnings.push(format!(
                "Root\\InventoryApplication subkey enumeration failed: {err}"
            ));
            return;
        }
    };

    for (subkey_name, subkey_nk) in subkeys {
        let values = match hive.read_all_values_from_nk(&subkey_nk) {
            Ok(values) => values,
            Err(err) => {
                info.warnings.push(format!(
                    "Root\\InventoryApplication\\{} value read failed: {err}",
                    subkey_name
                ));
                continue;
            }
        };

        let mut entry = AmcacheApplicationEntry {
            registry_key_path: format!("Root\\InventoryApplication\\{}", subkey_name),
            ..Default::default()
        };

        for (name, value) in values {
            match name.to_ascii_lowercase().as_str() {
                "programid" => entry.program_id = string_from_value(&value),
                "name" => entry.name = string_from_value(&value),
                "version" => entry.version = string_from_value(&value),
                "publisher" => entry.publisher = string_from_value(&value),
                "source" => entry.source = string_from_value(&value),
                "osversionatinstalltime" => {
                    entry.os_version_at_install_time = string_from_value(&value)
                }
                "installdate" => {
                    entry.install_date = filetime_from_value(&value, &mut info.warnings)
                }
                _ => {}
            }
        }

        info.applications.push(entry);
    }
}

fn extract_application_files(hive: &RegistryHiveReader<'_>, info: &mut AmcacheInfo) {
    let Some(inventory) = navigate_key(hive, &["InventoryApplicationFile"], info) else {
        return;
    };

    let subkeys = match hive.read_subkeys_from_nk(&inventory) {
        Ok(subkeys) => subkeys,
        Err(err) => {
            info.warnings.push(format!(
                "Root\\InventoryApplicationFile subkey enumeration failed: {err}"
            ));
            return;
        }
    };

    for (subkey_name, subkey_nk) in subkeys {
        let values = match hive.read_all_values_from_nk(&subkey_nk) {
            Ok(values) => values,
            Err(err) => {
                info.warnings.push(format!(
                    "Root\\InventoryApplicationFile\\{} value read failed: {err}",
                    subkey_name
                ));
                continue;
            }
        };

        let mut entry = AmcacheApplicationFileEntry {
            registry_key_path: format!("Root\\InventoryApplicationFile\\{}", subkey_name),
            ..Default::default()
        };

        for (name, value) in values {
            match name.to_ascii_lowercase().as_str() {
                "programid" => entry.program_id = string_from_value(&value),
                "lowercaselongpath" => entry.lower_case_long_path = string_from_value(&value),
                "longpathhash" => entry.long_path_hash = string_from_value(&value),
                "productname" => entry.product_name = string_from_value(&value),
                "companyname" => entry.company_name = string_from_value(&value),
                "fileversion" => entry.file_version = string_from_value(&value),
                "filesize" => entry.file_size = qword_from_value(&value),
                "ispefile" => entry.is_pe_file = bool_from_dword(&value),
                "linkdate" => entry.link_date = filetime_from_value(&value, &mut info.warnings),
                _ => {}
            }
        }

        info.application_files.push(entry);
    }
}

fn navigate_key(
    hive: &RegistryHiveReader<'_>,
    path: &[&str],
    info: &mut AmcacheInfo,
) -> Option<super::types::NkRecord> {
    match hive.navigate_to(path) {
        Ok(Some(nk)) => Some(nk),
        Ok(None) => {
            // Missing keys are expected on trimmed/synthetic hives; do not
            // treat as an error.
            None
        }
        Err(err) => {
            info.warnings
                .push(format!("{} navigation failed: {err}", path.join("\\")));
            None
        }
    }
}

fn string_from_value(value: &RegistryValue) -> Option<String> {
    match value {
        RegistryValue::String(s) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn qword_from_value(value: &RegistryValue) -> Option<u64> {
    match value {
        RegistryValue::Qword(v) => Some(*v),
        RegistryValue::Dword(v) => Some(u64::from(*v)),
        _ => None,
    }
}

fn bool_from_dword(value: &RegistryValue) -> Option<bool> {
    match value {
        RegistryValue::Dword(v) => Some(*v != 0),
        RegistryValue::Qword(v) => Some(*v != 0),
        _ => None,
    }
}

fn filetime_from_value(value: &RegistryValue, warnings: &mut Vec<String>) -> Option<String> {
    let filetime = match value {
        RegistryValue::Qword(v) => *v,
        RegistryValue::Dword(v) => u64::from(*v),
        RegistryValue::Binary(data) if data.len() == 8 => u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]),
        other => {
            warnings.push(format!("unexpected timestamp type: {other:?}"));
            return None;
        }
    };

    windows_filetime_to_rfc3339(filetime)
}

#[cfg(test)]
mod tests {
    use super::super::test_common::*;
    use super::*;

    #[test]
    fn extract_amcache_applications_from_fixture() {
        // Layout offsets chosen to avoid colliding with internal nodes and
        // subkey/value lists used by empty_hive/write_nk helpers.
        let mut data = empty_hive("Root");
        write_nk(
            &mut data,
            0x20,
            "Root",
            &[("InventoryApplication", 0x200)],
            &[],
        );
        write_nk(
            &mut data,
            0x200,
            "InventoryApplication",
            &[("app-0001", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "app-0001",
            &[],
            &[0x1000, 0x1080, 0x1100, 0x1180],
        );
        write_string_value(&mut data, 0x1000, "Name", "Contoso App", 0x3000);
        write_string_value(&mut data, 0x1080, "Version", "1.2.3", 0x3080);
        write_string_value(&mut data, 0x1100, "Publisher", "Contoso Ltd.", 0x3100);
        write_string_value(&mut data, 0x1180, "Source", "AddRemoveProgram", 0x3180);

        let info =
            extract_amcache_entries(&data, "Windows/AppCompat/Programs/Amcache.hve").unwrap();

        assert_eq!(info.applications.len(), 1);
        let app = &info.applications[0];
        assert_eq!(app.name.as_deref(), Some("Contoso App"));
        assert_eq!(app.version.as_deref(), Some("1.2.3"));
        assert_eq!(app.publisher.as_deref(), Some("Contoso Ltd."));
        assert_eq!(app.source.as_deref(), Some("AddRemoveProgram"));
        assert_eq!(
            app.registry_key_path,
            "Root\\InventoryApplication\\app-0001"
        );
    }

    #[test]
    fn extract_amcache_application_files_from_fixture() {
        let mut data = empty_hive("Root");
        write_nk(
            &mut data,
            0x20,
            "Root",
            &[("InventoryApplicationFile", 0x200)],
            &[],
        );
        write_nk(
            &mut data,
            0x200,
            "InventoryApplicationFile",
            &[("file-0001", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "file-0001",
            &[],
            &[0x1000, 0x1080, 0x1100, 0x1180],
        );
        write_string_value(
            &mut data,
            0x1000,
            "LowerCaseLongPath",
            "c:\\program files\\contoso\\app.exe",
            0x3000,
        );
        write_string_value(&mut data, 0x1080, "LongPathHash", "deadbeef", 0x3080);
        write_qword_value(&mut data, 0x1100, "FileSize", 1_048_576, 0x3100);
        write_string_value(&mut data, 0x1180, "ProgramId", "prog-1234", 0x3180);

        let info =
            extract_amcache_entries(&data, "Windows/AppCompat/Programs/Amcache.hve").unwrap();

        assert_eq!(info.application_files.len(), 1);
        let file = &info.application_files[0];
        assert_eq!(
            file.lower_case_long_path.as_deref(),
            Some("c:\\program files\\contoso\\app.exe")
        );
        assert_eq!(file.long_path_hash.as_deref(), Some("deadbeef"));
        assert_eq!(file.file_size, Some(1_048_576));
        assert_eq!(file.program_id.as_deref(), Some("prog-1234"));
        assert_eq!(
            file.registry_key_path,
            "Root\\InventoryApplicationFile\\file-0001"
        );
    }
}
