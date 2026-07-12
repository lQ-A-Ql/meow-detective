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
        extract_control_set_services(&hive, &control_set, &mut seen, &mut info);
    }

    Ok(info)
}

fn extract_control_set_services(
    hive: &RegistryHiveReader<'_>,
    control_set: &str,
    seen: &mut std::collections::HashSet<String>,
    info: &mut SystemServiceInfo,
) {
    let Some(services_key) = hive.navigate_to(&[control_set, "Services"]).unwrap_or(None) else {
        return;
    };
    for service_name in hive
        .read_subkey_names_from_nk(&services_key)
        .unwrap_or_default()
    {
        if !seen.insert(service_name.clone()) {
            continue;
        }
        match extract_service(hive, control_set, &service_name) {
            Ok(entry) => info.services.push(entry),
            Err(warning) => info.warnings.push(warning),
        }
    }
}

fn extract_service(
    hive: &RegistryHiveReader<'_>,
    control_set: &str,
    service_name: &str,
) -> Result<SystemServiceEntry, String> {
    let path = [control_set, "Services", service_name];
    let path_text = path.join("\\");
    let key = hive
        .navigate_to(&path)
        .map_err(|error| format!("could not navigate to service key {path_text}: {error}"))?
        .ok_or_else(|| format!("could not navigate to service key {path_text}"))?;
    let values = hive
        .read_all_values_from_nk(&key)
        .map_err(|error| format!("failed to read values for {path_text}: {error}"))?;
    let mut entry = SystemServiceEntry {
        service_name: service_name.to_string(),
        key_path: path_text,
        key_last_write: key.last_write_time.and_then(windows_filetime_to_rfc3339),
        ..Default::default()
    };
    let mut start = None;
    for (name, value) in values {
        apply_service_value(&mut entry, &mut start, &name, value);
    }
    if let Some(value) = start {
        entry.start_type = ServiceStartType::from_raw(value, entry.delayed_auto_start);
    }
    resolve_service_dll(hive, control_set, service_name, &mut entry);
    Ok(entry)
}

fn apply_service_value(
    entry: &mut SystemServiceEntry,
    start: &mut Option<u32>,
    name: &str,
    value: RegistryValue,
) {
    match (name, value) {
        ("Type", RegistryValue::Dword(value)) => entry.service_type = ServiceType::from_raw(value),
        ("Start", RegistryValue::Dword(value)) => *start = Some(value),
        ("ErrorControl", RegistryValue::Dword(value)) => entry.error_control = Some(value),
        ("DelayedAutoStart", RegistryValue::Dword(value)) => entry.delayed_auto_start = value != 0,
        ("ImagePath", RegistryValue::String(value)) => entry.image_path = Some(value),
        ("DisplayName", RegistryValue::String(value)) => entry.display_name = Some(value),
        ("Group", RegistryValue::String(value)) => entry.group = Some(value),
        ("ObjectName", RegistryValue::String(value)) => entry.object_name = Some(value),
        ("FailureCommand", RegistryValue::String(value)) => entry.failure_command = Some(value),
        ("DependOnService", RegistryValue::MultiString(value)) => entry.depend_on_service = value,
        ("DependOnGroup", RegistryValue::MultiString(value)) => entry.depend_on_group = value,
        ("RequiredPrivileges", RegistryValue::MultiString(value)) => {
            entry.required_privileges = value
        }
        _ => {}
    }
}

fn resolve_service_dll(
    hive: &RegistryHiveReader<'_>,
    control_set: &str,
    service_name: &str,
    entry: &mut SystemServiceEntry,
) {
    let Some(image_path) = entry.image_path.as_deref() else {
        return;
    };
    let is_shared = matches!(
        entry.service_type,
        ServiceType::Win32ShareProcess | ServiceType::Win32ShareProcessInteractive
    );
    if image_path.to_ascii_lowercase().contains("svchost.exe") && is_shared {
        let path = [control_set, "Services", service_name, "Parameters"];
        if let Ok(Some(RegistryValue::String(dll))) = hive.lookup_value(&path, "ServiceDll") {
            entry.service_dll = Some(dll);
        }
    }
}
