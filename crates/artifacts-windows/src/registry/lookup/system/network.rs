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
        let interfaces_path = [
            control_set.as_str(),
            "Services",
            "Tcpip",
            "Parameters",
            "Interfaces",
        ];
        let Ok(Some(interfaces_nk)) = hive.navigate_to(&interfaces_path) else {
            continue;
        };
        let guids = hive
            .read_subkey_names_from_nk(&interfaces_nk)
            .unwrap_or_default();

        for guid in guids {
            if !seen_guids.insert(guid.clone()) {
                continue;
            }
            let mut adapter = NetworkAdapterInfo {
                guid: guid.clone(),
                ..Default::default()
            };

            let interface_path = [
                control_set.as_str(),
                "Services",
                "Tcpip",
                "Parameters",
                "Interfaces",
                &guid,
            ];
            let Ok(Some(nk)) = hive.navigate_to(&interface_path) else {
                adapters.push(adapter);
                continue;
            };

            let values = hive.read_all_values_from_nk(&nk).unwrap_or_default();
            for (name, value) in values {
                match (name.as_str(), value) {
                    ("DhcpIPAddress", RegistryValue::String(v)) if !v.is_empty() => {
                        adapter.ip_address = Some(v);
                    }
                    ("DhcpDefaultGateway", RegistryValue::String(v)) if !v.is_empty() => {
                        adapter.gateway = Some(v);
                    }
                    ("DhcpServer", RegistryValue::String(v)) if !v.is_empty() => {
                        adapter.dhcp_server = Some(v);
                    }
                    ("DhcpSubnetMask", RegistryValue::String(v)) if !v.is_empty() => {
                        adapter.subnet_mask = Some(v);
                    }
                    ("EnableDHCP", RegistryValue::Dword(v)) => {
                        adapter.dhcp_enabled = Some(v != 0);
                    }
                    ("NameServer", RegistryValue::String(v)) if !v.is_empty() => {
                        adapter.dns_servers = v
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect();
                    }
                    ("IPAddress", RegistryValue::MultiString(v))
                        if adapter.ip_address.is_none() =>
                    {
                        adapter.ip_address = v.into_iter().find(|s| !s.is_empty());
                    }
                    ("DefaultGateway", RegistryValue::MultiString(v))
                        if adapter.gateway.is_none() =>
                    {
                        adapter.gateway = v.into_iter().find(|s| !s.is_empty());
                    }
                    _ => {}
                }
            }

            // Try to resolve a friendly name and MAC address from the
            // network class or connection description keys.
            if let Some((name, mac)) =
                resolve_adapter_friendly_name_and_mac(&hive, &control_set, &guid)
            {
                adapter.name = name.or(adapter.name);
                adapter.mac_address = mac.or(adapter.mac_address);
            }

            adapters.push(adapter);
        }
    }

    Ok(adapters)
}

fn resolve_adapter_friendly_name_and_mac(
    hive: &RegistryHiveReader<'_>,
    control_set: &str,
    guid: &str,
) -> Option<(Option<String>, Option<String>)> {
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

    Some((name, mac))
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
