use super::super::{InstalledSoftwareInfo, RegistryHiveReader};
use super::values::{read_optional_dword_value, read_optional_string_value};
use crate::registry::RegistryError;

pub fn extract_installed_software(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<InstalledSoftwareInfo>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut result = Vec::new();
    let roots: &[&[&str]] = &[
        &["Microsoft", "Windows", "CurrentVersion", "Uninstall"],
        &[
            "WOW6432Node",
            "Microsoft",
            "Windows",
            "CurrentVersion",
            "Uninstall",
        ],
    ];

    for root in roots {
        let Some(node) = hive.navigate_to(root).ok().flatten() else {
            continue;
        };
        let Ok(subkey_names) = hive.read_subkey_names_from_nk(&node) else {
            continue;
        };
        for subkey_name in subkey_names {
            let mut path = root.to_vec();
            path.push(subkey_name.as_str());
            let Some(display_name) = read_optional_string_value(&hive, &path, "DisplayName")
                .filter(|name| !name.trim().is_empty())
            else {
                continue;
            };
            result.push(InstalledSoftwareInfo {
                display_name,
                version: read_optional_string_value(&hive, &path, "DisplayVersion"),
                publisher: read_optional_string_value(&hive, &path, "Publisher"),
                install_date: read_optional_string_value(&hive, &path, "InstallDate"),
                estimated_size_kb: read_optional_dword_value(&hive, &path, "EstimatedSize")
                    .map(u64::from),
                uninstall_string: read_optional_string_value(&hive, &path, "UninstallString"),
                source_key: path.join("\\"),
            });
        }
    }
    Ok(result)
}
