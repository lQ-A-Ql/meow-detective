use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

/// Extract TCP/IP network adapter configuration from the SYSTEM hive.
/// Reads `Services\Tcpip\Parameters\Interfaces` for each control set and
/// enriches the entries with friendly names and MAC addresses when possible.
pub fn extract_network_adapters_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<NetworkAdapterInfo>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut warnings = Vec::new();
    let control_sets = hive.control_set_candidates(&mut warnings);
    let mut adapters: Vec<NetworkAdapterInfo> = Vec::new();
    let mut seen_guids = std::collections::HashSet::new();

    for control_set in control_sets {
        adapters.extend(extract_control_set_adapters(
            &hive,
            &control_set,
            &mut seen_guids,
        ));
    }

    Ok(adapters)
}

fn extract_control_set_adapters(
    hive: &RegistryHiveReader<'_>,
    control_set: &str,
    seen_guids: &mut std::collections::HashSet<String>,
) -> Vec<NetworkAdapterInfo> {
    adapter_guids(hive, control_set)
        .into_iter()
        .filter(|guid| seen_guids.insert(guid.clone()))
        .map(|guid| extract_adapter(hive, control_set, guid))
        .collect()
}

fn adapter_guids(hive: &RegistryHiveReader<'_>, control_set: &str) -> Vec<String> {
    let paths: [&[&str]; 3] = [
        &[control_set, "Services", "Tcpip", "Parameters", "Interfaces"],
        &[
            control_set,
            "Control",
            "Network",
            "{4D36E972-E325-11CE-BFC1-08002BE10318}",
        ],
        &[control_set, "Control", "NetworkSetup2", "Interfaces"],
    ];
    let mut guids = Vec::new();
    let mut normalized = std::collections::HashSet::new();
    for path in paths {
        let Ok(Some(parent)) = hive.navigate_to(path) else {
            continue;
        };
        for guid in hive.read_subkey_names_from_nk(&parent).unwrap_or_default() {
            if is_interface_guid(&guid) && normalized.insert(guid.to_ascii_lowercase()) {
                guids.push(guid);
            }
        }
    }
    guids
}

fn is_interface_guid(value: &str) -> bool {
    let Some(inner) = value
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    else {
        return false;
    };
    inner.len() == 36
        && inner.chars().enumerate().all(|(index, ch)| match index {
            8 | 13 | 18 | 23 => ch == '-',
            _ => ch.is_ascii_hexdigit(),
        })
}

fn extract_adapter(
    hive: &RegistryHiveReader<'_>,
    control_set: &str,
    guid: String,
) -> NetworkAdapterInfo {
    let mut adapter = NetworkAdapterInfo {
        guid: guid.clone(),
        ..Default::default()
    };
    let path = [
        control_set,
        "Services",
        "Tcpip",
        "Parameters",
        "Interfaces",
        &guid,
    ];
    if let Ok(Some(key)) = hive.navigate_to(&path) {
        for (name, value) in hive.read_all_values_from_nk(&key).unwrap_or_default() {
            apply_interface_value(&mut adapter, &name, value);
        }
    }
    enrich_adapter_identity(hive, control_set, &guid, &mut adapter);
    enrich_adapter_from_network_setup2(hive, control_set, &guid, &mut adapter);
    adapter
}

fn apply_interface_value(adapter: &mut NetworkAdapterInfo, name: &str, value: RegistryValue) {
    match (name.to_ascii_lowercase().as_str(), value) {
        ("dhcpipaddress", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.ip_addresses = split_network_values(&value)
        }
        ("dhcpdefaultgateway", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.gateways = split_network_values(&value)
        }
        ("dhcpserver", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.dhcp_server = Some(value)
        }
        ("dhcpsubnetmask", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.subnet_masks = split_network_values(&value)
        }
        ("enabledhcp", RegistryValue::Dword(value)) => adapter.dhcp_enabled = Some(value != 0),
        ("nameserver" | "dhcpnameserver", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.dns_servers = split_network_values(&value)
        }
        ("ipaddress", RegistryValue::MultiString(values)) if adapter.ip_addresses.is_empty() => {
            adapter.ip_addresses = non_empty_values(values)
        }
        ("subnetmask", RegistryValue::MultiString(values)) if adapter.subnet_masks.is_empty() => {
            adapter.subnet_masks = non_empty_values(values)
        }
        ("defaultgateway", RegistryValue::MultiString(values)) if adapter.gateways.is_empty() => {
            adapter.gateways = non_empty_values(values)
        }
        _ => {}
    }
}

fn enrich_adapter_identity(
    hive: &RegistryHiveReader<'_>,
    control_set: &str,
    guid: &str,
    adapter: &mut NetworkAdapterInfo,
) {
    // Connection name under Control\Network\{NetworkClass}\{GUID}\Connection.
    let network_path = [
        control_set,
        "Control",
        "Network",
        "{4D36E972-E325-11CE-BFC1-08002BE10318}",
        guid,
        "Connection",
    ];
    if let Ok(Some(nk)) = hive.navigate_to(&network_path) {
        if let Ok(values) = hive.read_all_values_from_nk(&nk) {
            for (n, v) in values {
                if n.eq_ignore_ascii_case("Name") {
                    if let RegistryValue::String(v) = v {
                        if !v.is_empty() {
                            adapter.name = Some(v);
                        }
                    }
                }
            }
        }
    }

    // MAC address is stored under the class key.  Scan subkeys until we find
    // one whose NetCfgInstanceId matches the interface GUID.
    let class_path = [
        control_set,
        "Control",
        "Class",
        "{4D36E972-E325-11CE-BFC1-08002BE10318}",
    ];
    if let Ok(Some(class_nk)) = hive.navigate_to(&class_path) {
        if let Ok(subkeys) = hive.read_subkey_names_from_nk(&class_nk) {
            for subkey in subkeys {
                let entry_path = [
                    control_set,
                    "Control",
                    "Class",
                    "{4D36E972-E325-11CE-BFC1-08002BE10318}",
                    &subkey,
                ];
                let Ok(Some(entry_nk)) = hive.navigate_to(&entry_path) else {
                    continue;
                };
                let Ok(values) = hive.read_all_values_from_nk(&entry_nk) else {
                    continue;
                };
                let mut matches_guid = false;
                let mut entry_mac = None;
                let mut description = None;
                let mut pnp_instance_id = None;
                let mut service_name = None;
                for (n, v) in values {
                    if n.eq_ignore_ascii_case("NetCfgInstanceId") {
                        if let RegistryValue::String(v) = v {
                            matches_guid = v.eq_ignore_ascii_case(guid);
                        }
                    } else if n.eq_ignore_ascii_case("NetworkAddress")
                        || n.eq_ignore_ascii_case("PermanentAddress")
                    {
                        if let RegistryValue::String(v) = v {
                            if !v.is_empty() {
                                entry_mac = Some(format_mac_address(&v));
                            }
                        } else if let RegistryValue::Binary(bytes) = v {
                            entry_mac = Some(format_mac_bytes(&bytes));
                        }
                    } else if n.eq_ignore_ascii_case("DriverDesc") {
                        description = registry_string(v);
                    } else if n.eq_ignore_ascii_case("PnPInstanceId") {
                        pnp_instance_id = registry_string(v);
                    } else if n.eq_ignore_ascii_case("Service") {
                        service_name = registry_string(v);
                    }
                }
                if matches_guid {
                    adapter.mac_address = entry_mac.or(adapter.mac_address.take());
                    adapter.description = description;
                    adapter.pnp_instance_id = pnp_instance_id;
                    adapter.service_name = service_name;
                    if adapter.name.is_none() {
                        adapter.name = adapter.description.clone();
                    }
                    break;
                }
            }
        }
    }
}

fn enrich_adapter_from_network_setup2(
    hive: &RegistryHiveReader<'_>,
    control_set: &str,
    guid: &str,
    adapter: &mut NetworkAdapterInfo,
) {
    let kernel_path = [
        control_set,
        "Control",
        "NetworkSetup2",
        "Interfaces",
        guid,
        "Kernel",
    ];
    let Ok(Some(kernel)) = hive.navigate_to(&kernel_path) else {
        return;
    };
    let Ok(values) = hive.read_all_values_from_nk(&kernel) else {
        return;
    };

    for (name, value) in values {
        match name.to_ascii_lowercase().as_str() {
            "ifalias" if adapter.name.is_none() => adapter.name = registry_string(value),
            "ifdescr" if adapter.description.is_none() => {
                adapter.description = registry_string(value)
            }
            "currentaddress" => {
                adapter.mac_address = network_setup_address(value).or(adapter.mac_address.take())
            }
            "permanentaddress" if adapter.permanent_mac_address.is_none() => {
                adapter.permanent_mac_address = network_setup_address(value)
            }
            _ => {}
        }
    }

    if adapter.mac_address.is_none() {
        adapter.mac_address = adapter.permanent_mac_address.clone();
    }
}

fn registry_string(value: RegistryValue) -> Option<String> {
    match value {
        RegistryValue::String(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

fn network_setup_address(value: RegistryValue) -> Option<String> {
    match value {
        RegistryValue::Binary(bytes) if bytes.len() >= 6 => Some(format_mac_bytes(&bytes[..6])),
        RegistryValue::String(value) if !value.is_empty() => Some(format_mac_address(&value)),
        _ => None,
    }
}

fn split_network_values(value: &str) -> Vec<String> {
    value
        .split([',', ';', ' '])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn non_empty_values(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn format_mac_address(raw: &str) -> String {
    let digits: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if digits.len() == 12 {
        digits
            .chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|c| c.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join(":")
            .to_ascii_uppercase()
    } else {
        raw.to_string()
    }
}

fn format_mac_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
#[path = "../../../../tests/unit/registry/lookup/system/network.rs"]
mod tests;
