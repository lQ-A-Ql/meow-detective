use std::collections::HashMap;

use super::super::{NetworkProfileEntry, RegistryHiveReader};
use super::values::{
    read_optional_binary_value, read_optional_dword_value, read_optional_string_value,
    systemtime_bytes_to_rfc3339,
};
use crate::registry::RegistryError;

pub fn extract_network_profiles_from_software_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<NetworkProfileEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut profiles = HashMap::new();
    let profile_root = [
        "Microsoft",
        "Windows NT",
        "CurrentVersion",
        "NetworkList",
        "Profiles",
    ];
    if let Some(node) = hive.navigate_to(&profile_root).ok().flatten() {
        for guid in hive.read_subkey_names_from_nk(&node).unwrap_or_default() {
            let mut path = profile_root.to_vec();
            path.push(&guid);
            profiles.insert(
                guid.clone(),
                NetworkProfileEntry {
                    profile_guid: guid.clone(),
                    profile_name: read_optional_string_value(&hive, &path, "ProfileName")
                        .unwrap_or_default(),
                    description: read_optional_string_value(&hive, &path, "Description"),
                    date_created: read_optional_binary_value(&hive, &path, "DateCreated")
                        .and_then(|value| systemtime_bytes_to_rfc3339(&value)),
                    date_last_connected: read_optional_binary_value(
                        &hive,
                        &path,
                        "DateLastConnected",
                    )
                    .and_then(|value| systemtime_bytes_to_rfc3339(&value)),
                    name_type: read_optional_dword_value(&hive, &path, "NameType"),
                    managed: read_optional_dword_value(&hive, &path, "Managed")
                        .map(|value| value != 0)
                        .unwrap_or(false),
                    first_network: None,
                    default_gateway_mac_hex: None,
                    dns_suffix: None,
                    source_key_path: path.join("\\"),
                },
            );
        }
    }

    merge_signatures(&hive, &mut profiles);
    let mut result = profiles.into_values().collect::<Vec<_>>();
    result.sort_by(|left, right| left.profile_guid.cmp(&right.profile_guid));
    Ok(result)
}

fn merge_signatures(
    hive: &RegistryHiveReader<'_>,
    profiles: &mut HashMap<String, NetworkProfileEntry>,
) {
    for (root, managed) in signature_roots() {
        let Some(node) = hive.navigate_to(&root).ok().flatten() else {
            continue;
        };
        for guid in hive.read_subkey_names_from_nk(&node).unwrap_or_default() {
            let mut path = root.to_vec();
            path.push(&guid);
            let profile_guid = read_optional_string_value(hive, &path, "ProfileGuid")
                .unwrap_or_else(|| guid.clone());
            let first_network = read_optional_string_value(hive, &path, "FirstNetwork");
            let gateway =
                read_optional_binary_value(hive, &path, "DefaultGatewayMac").map(hex::encode);
            let dns_suffix = read_optional_string_value(hive, &path, "DnsSuffix");
            if let Some(entry) = profiles.get_mut(&profile_guid) {
                entry.managed = managed;
                entry.first_network = first_network;
                entry.default_gateway_mac_hex = gateway;
                entry.dns_suffix = dns_suffix;
            } else {
                profiles.insert(
                    profile_guid.clone(),
                    NetworkProfileEntry {
                        profile_guid,
                        profile_name: String::new(),
                        description: None,
                        date_created: None,
                        date_last_connected: None,
                        name_type: None,
                        managed,
                        first_network,
                        default_gateway_mac_hex: gateway,
                        dns_suffix,
                        source_key_path: path.join("\\"),
                    },
                );
            }
        }
    }
}

fn signature_roots() -> [([&'static str; 6], bool); 2] {
    [
        (
            [
                "Microsoft",
                "Windows NT",
                "CurrentVersion",
                "NetworkList",
                "Signatures",
                "Managed",
            ],
            true,
        ),
        (
            [
                "Microsoft",
                "Windows NT",
                "CurrentVersion",
                "NetworkList",
                "Signatures",
                "Unmanaged",
            ],
            false,
        ),
    ]
}
