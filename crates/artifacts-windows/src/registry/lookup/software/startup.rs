use super::super::{
    windows_filetime_to_rfc3339, RegistryHiveReader, RegistryRunKey, RegistryValue, WinlogonConfig,
};
use super::values::read_optional_string_value;
use crate::registry::RegistryError;

pub fn extract_machine_run_keys_from_software_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<RegistryRunKey>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut result = Vec::new();
    let roots: &[&[&str]] = &[
        &["Microsoft", "Windows", "CurrentVersion", "Run"],
        &["Microsoft", "Windows", "CurrentVersion", "RunOnce"],
        &["Microsoft", "Windows", "CurrentVersion", "RunOnceEx"],
        &[
            "WOW6432Node",
            "Microsoft",
            "Windows",
            "CurrentVersion",
            "Run",
        ],
        &[
            "WOW6432Node",
            "Microsoft",
            "Windows",
            "CurrentVersion",
            "RunOnce",
        ],
    ];
    for root in roots {
        let Some(node) = hive.navigate_to(root).ok().flatten() else {
            continue;
        };
        let timestamp = node.last_write_time.and_then(windows_filetime_to_rfc3339);
        let Ok(values) = hive.read_all_values_from_nk(&node) else {
            continue;
        };
        for (name, value) in values {
            if let RegistryValue::String(command) = value {
                if !command.trim().is_empty() {
                    result.push(RegistryRunKey {
                        key_path: root.join("\\"),
                        value_name: name,
                        command,
                        timestamp: timestamp.clone(),
                        scope: "machine".to_string(),
                    });
                }
            }
        }
    }
    Ok(result)
}

pub fn extract_winlogon_fields_from_software_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<WinlogonConfig, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let path = ["Microsoft", "Windows NT", "CurrentVersion", "Winlogon"];
    Ok(WinlogonConfig {
        shell: read_optional_string_value(&hive, &path, "Shell"),
        userinit: read_optional_string_value(&hive, &path, "Userinit"),
        notify: read_optional_string_value(&hive, &path, "Notify"),
        auto_admin_logon: read_optional_string_value(&hive, &path, "AutoAdminLogon"),
        default_domain_name: read_optional_string_value(&hive, &path, "DefaultDomainName"),
        default_user_name: read_optional_string_value(&hive, &path, "DefaultUserName"),
        key_path: path.join("\\"),
    })
}
