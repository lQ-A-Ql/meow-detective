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
    let interfaces_path = [control_set, "Services", "Tcpip", "Parameters", "Interfaces"];
    let Ok(Some(interfaces)) = hive.navigate_to(&interfaces_path) else {
        return Vec::new();
    };
    hive.read_subkey_names_from_nk(&interfaces)
        .unwrap_or_default()
        .into_iter()
        .filter(|guid| seen_guids.insert(guid.clone()))
        .map(|guid| extract_adapter(hive, control_set, guid))
        .collect()
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
    let (name, mac) = resolve_adapter_friendly_name_and_mac(hive, control_set, &guid);
    adapter.name = name.or(adapter.name);
    adapter.mac_address = mac.or(adapter.mac_address);
    adapter
}

fn apply_interface_value(adapter: &mut NetworkAdapterInfo, name: &str, value: RegistryValue) {
    match (name, value) {
        ("DhcpIPAddress", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.ip_address = Some(value)
        }
        ("DhcpDefaultGateway", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.gateway = Some(value)
        }
        ("DhcpServer", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.dhcp_server = Some(value)
        }
        ("DhcpSubnetMask", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.subnet_mask = Some(value)
        }
        ("EnableDHCP", RegistryValue::Dword(value)) => adapter.dhcp_enabled = Some(value != 0),
        ("NameServer", RegistryValue::String(value)) if !value.is_empty() => {
            adapter.dns_servers = value
                .split(',')
                .map(str::trim)
                .filter(|server| !server.is_empty())
                .map(str::to_string)
                .collect()
        }
        ("IPAddress", RegistryValue::MultiString(values)) if adapter.ip_address.is_none() => {
            adapter.ip_address = values.into_iter().find(|value| !value.is_empty())
        }
        ("DefaultGateway", RegistryValue::MultiString(values)) if adapter.gateway.is_none() => {
            adapter.gateway = values.into_iter().find(|value| !value.is_empty())
        }
        _ => {}
    }
}

fn resolve_adapter_friendly_name_and_mac(
    hive: &RegistryHiveReader<'_>,
    control_set: &str,
    guid: &str,
) -> (Option<String>, Option<String>) {
    // Connection name under Control\Network\{NetworkClass}\{GUID}\Connection.
    let network_path = [
        control_set,
        "Control",
        "Network",
        "{4D36E972-E325-11CE-BFC1-08002BE10318}",
        guid,
        "Connection",
    ];
    let mut name = None;
    let mut mac = None;

    if let Ok(Some(nk)) = hive.navigate_to(&network_path) {
        if let Ok(values) = hive.read_all_values_from_nk(&nk) {
            for (n, v) in values {
                if n.eq_ignore_ascii_case("Name") {
                    if let RegistryValue::String(v) = v {
                        if !v.is_empty() {
                            name = Some(v);
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
                    }
                }
                if matches_guid {
                    mac = entry_mac;
                    break;
                }
            }
        }
    }

    (name, mac)
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
