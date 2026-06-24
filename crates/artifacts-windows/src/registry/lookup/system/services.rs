use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

/// Extract all services and drivers from `SYSTEM\<ControlSet>\Services`.
pub fn extract_services_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<SystemServiceInfo, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut warnings = Vec::new();
    let control_sets = hive.control_set_candidates(&mut warnings);
    let mut info = SystemServiceInfo {
        services: Vec::new(),
        warnings,
    };
    let mut seen = std::collections::HashSet::new();

    for control_set in control_sets {
        let services_path = [control_set.as_str(), "Services"];
        let Some(services_nk) = hive.navigate_to(&services_path).unwrap_or(None) else {
            continue;
        };
        let service_names = hive
            .read_subkey_names_from_nk(&services_nk)
            .unwrap_or_default();

        for service_name in service_names {
            if !seen.insert(service_name.clone()) {
                continue;
            }
            let service_path = [control_set.as_str(), "Services", service_name.as_str()];
            let Some(service_nk) = hive.navigate_to(&service_path).unwrap_or(None) else {
                info.warnings.push(format!(
                    "could not navigate to service key {}",
                    service_path.join("\\")
                ));
                continue;
            };
            let values = match hive.read_all_values_from_nk(&service_nk) {
                Ok(v) => v,
                Err(err) => {
                    info.warnings.push(format!(
                        "failed to read values for {}: {}",
                        service_path.join("\\"),
                        err
                    ));
                    continue;
                }
            };

            let mut entry = SystemServiceEntry {
                service_name: service_name.clone(),
                key_path: service_path.join("\\"),
                key_last_write: service_nk
                    .last_write_time
                    .and_then(windows_filetime_to_rfc3339),
                ..Default::default()
            };

            let mut start_raw: Option<u32> = None;
            for (name, value) in values {
                match (name.as_str(), value) {
                    ("Type", RegistryValue::Dword(v)) => {
                        entry.service_type = ServiceType::from_raw(v);
                    }
                    ("Start", RegistryValue::Dword(v)) => {
                        start_raw = Some(v);
                    }
                    ("ErrorControl", RegistryValue::Dword(v)) => {
                        entry.error_control = Some(v);
                    }
                    ("DelayedAutoStart", RegistryValue::Dword(v)) => {
                        entry.delayed_auto_start = v != 0;
                    }
                    ("ImagePath", RegistryValue::String(v)) => {
                        entry.image_path = Some(v);
                    }
                    ("DisplayName", RegistryValue::String(v)) => {
                        entry.display_name = Some(v);
                    }
                    ("Group", RegistryValue::String(v)) => {
                        entry.group = Some(v);
                    }
                    ("ObjectName", RegistryValue::String(v)) => {
                        entry.object_name = Some(v);
                    }
                    ("FailureCommand", RegistryValue::String(v)) => {
                        entry.failure_command = Some(v);
                    }
                    ("DependOnService", RegistryValue::MultiString(v)) => {
                        entry.depend_on_service = v;
                    }
                    ("DependOnGroup", RegistryValue::MultiString(v)) => {
                        entry.depend_on_group = v;
                    }
                    ("RequiredPrivileges", RegistryValue::MultiString(v)) => {
                        entry.required_privileges = v;
                    }
                    _ => {}
                }
            }

            if let Some(start) = start_raw {
                entry.start_type = ServiceStartType::from_raw(start, entry.delayed_auto_start);
            }

            // Resolve the real DLL for svchost-hosted services.
            if let Some(ref path) = entry.image_path {
                if path.to_ascii_lowercase().contains("svchost.exe")
                    && matches!(
                        entry.service_type,
                        ServiceType::Win32ShareProcess | ServiceType::Win32ShareProcessInteractive
                    )
                {
                    let params_path = [
                        control_set.as_str(),
                        "Services",
                        service_name.as_str(),
                        "Parameters",
                    ];
                    if let Ok(Some(RegistryValue::String(dll))) =
                        hive.lookup_value(&params_path, "ServiceDll")
                    {
                        entry.service_dll = Some(dll);
                    }
                }
            }

            info.services.push(entry);
        }
    }

    Ok(info)
}
