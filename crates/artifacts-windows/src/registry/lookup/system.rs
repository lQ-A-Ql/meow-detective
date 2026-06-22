use super::reader::RegistryHiveReader;
use super::txlog_util::apply_single_txlog_override;
use super::*;
use crate::registry::RegistryError;

// ── SYSTEM hive field extraction ──────────────────────────────────────────────

pub fn extract_system_hive_fields(
    bytes: &[u8],
    hive_path: &str,
) -> Result<SystemHiveInfo, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = SystemHiveInfo::default();
    let control_sets = hive.control_set_candidates(&mut info.warnings);

    for control_set in control_sets {
        let computer_key = [
            control_set.as_str(),
            "Control",
            "ComputerName",
            "ComputerName",
        ];
        if info.computer_name.is_none() {
            info.computer_name = lookup_string_field(
                &hive,
                hive_path,
                "registry.system",
                &computer_key,
                "ComputerName",
                &mut info.warnings,
            );
        }

        let timezone_key = [control_set.as_str(), "Control", "TimeZoneInformation"];
        if info.timezone.is_none() {
            info.timezone = lookup_string_field(
                &hive,
                hive_path,
                "registry.system",
                &timezone_key,
                "TimeZoneKeyName",
                &mut info.warnings,
            )
            .or_else(|| {
                lookup_string_field(
                    &hive,
                    hive_path,
                    "registry.system",
                    &timezone_key,
                    "StandardName",
                    &mut info.warnings,
                )
            });
        }

        if info.computer_name.is_some() && info.timezone.is_some() {
            break;
        }
    }
    Ok(info)
}

/// Extract LSA authentication, notification and security packages from the
/// SYSTEM hive for each control set candidate.
pub fn extract_lsa_packages_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<LsaPackages>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut warnings = Vec::new();
    let control_sets = hive.control_set_candidates(&mut warnings);
    let mut result = Vec::new();

    for control_set in control_sets {
        let lsa_path = [control_set.as_str(), "Control", "Lsa"];
        let authentication_packages = match hive.lookup_value(&lsa_path, "Authentication Packages")
        {
            Ok(Some(RegistryValue::MultiString(values))) => values,
            _ => Vec::new(),
        };
        let notification_packages = match hive.lookup_value(&lsa_path, "Notification Packages") {
            Ok(Some(RegistryValue::MultiString(values))) => values,
            _ => Vec::new(),
        };
        let security_packages = match hive.lookup_value(&lsa_path, "Security Packages") {
            Ok(Some(RegistryValue::MultiString(values))) => values,
            _ => Vec::new(),
        };

        if !authentication_packages.is_empty()
            || !notification_packages.is_empty()
            || !security_packages.is_empty()
        {
            result.push(LsaPackages {
                control_set: control_set.clone(),
                authentication_packages,
                notification_packages,
                security_packages,
            });
        }
    }

    Ok(result)
}

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

/// Extract USB device history from `SYSTEM\Enum\USBSTOR`.
pub fn extract_usb_devices_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<UsbDeviceHistoryEntry>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut entries = Vec::new();

    let Some(usbstor_nk) = hive.navigate_to(&["Enum", "USBSTOR"]).unwrap_or(None) else {
        return Ok(entries);
    };
    let class_keys = hive
        .read_subkey_names_from_nk(&usbstor_nk)
        .unwrap_or_default();

    for class_name in class_keys {
        let (vendor, product, revision) = parse_usbstor_class_name(&class_name);
        let class_path = ["Enum", "USBSTOR", &class_name];
        let Some(class_nk) = hive.navigate_to(&class_path).unwrap_or(None) else {
            continue;
        };
        let first_connect = class_nk
            .last_write_time
            .and_then(super::windows_filetime_to_rfc3339);
        let serial_keys = hive
            .read_subkey_names_from_nk(&class_nk)
            .unwrap_or_default();

        for raw_serial in serial_keys {
            let serial_number = strip_usb_instance_suffix(&raw_serial);
            let serial_path = ["Enum", "USBSTOR", &class_name, &raw_serial];
            let Some(serial_nk) = hive.navigate_to(&serial_path).unwrap_or(None) else {
                continue;
            };
            let last_connect = serial_nk
                .last_write_time
                .and_then(super::windows_filetime_to_rfc3339);

            let values = hive.read_all_values_from_nk(&serial_nk).unwrap_or_default();
            let friendly_name = values
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("FriendlyName"))
                .and_then(|(_, value)| match value {
                    RegistryValue::String(s) => Some(s.clone()),
                    _ => None,
                });

            let fallback_name = usb_device_fallback_name(&vendor, &product, &class_name);
            let device_name = friendly_name.unwrap_or(fallback_name);

            entries.push(UsbDeviceHistoryEntry {
                device_name,
                serial_number,
                raw_serial_number: raw_serial,
                vendor: vendor.clone(),
                product: product.clone(),
                revision: revision.clone(),
                first_connect: first_connect.clone(),
                last_connect,
            });
        }
    }

    // Optional enrichment from `Enum\USB` using DeviceDesc when the USBSTOR
    // entry only had a fallback device name.
    enrich_usb_devices_from_enum_usb(&hive, &mut entries)?;

    Ok(entries)
}

fn parse_usbstor_class_name(name: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut vendor = None;
    let mut product = None;
    let mut revision = None;
    for part in name.split('&') {
        if let Some(value) = part.strip_prefix("Ven_") {
            vendor = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("Prod_") {
            product = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("Rev_") {
            revision = Some(value.to_string());
        }
    }
    (vendor, product, revision)
}

fn strip_usb_instance_suffix(raw: &str) -> String {
    if raw.len() >= 2 && (raw.ends_with("&0") || raw.ends_with("&1")) {
        raw[..raw.len() - 2].to_string()
    } else {
        raw.to_string()
    }
}

fn usb_device_fallback_name(
    vendor: &Option<String>,
    product: &Option<String>,
    class_name: &str,
) -> String {
    match (vendor.as_deref(), product.as_deref()) {
        (Some(v), Some(p)) => format!("{v} {p}"),
        (Some(v), None) => v.to_string(),
        (None, Some(p)) => p.to_string(),
        (None, None) => class_name.to_string(),
    }
}

fn enrich_usb_devices_from_enum_usb(
    hive: &RegistryHiveReader<'_>,
    entries: &mut [UsbDeviceHistoryEntry],
) -> Result<(), String> {
    let index_by_raw: std::collections::HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| (entry.raw_serial_number.clone(), idx))
        .collect();
    let Some(usb_nk) = hive.navigate_to(&["Enum", "USB"]).unwrap_or(None) else {
        return Ok(());
    };
    let vidpid_keys = hive.read_subkey_names_from_nk(&usb_nk).unwrap_or_default();

    for vidpid in vidpid_keys {
        let vidpid_path = ["Enum", "USB", &vidpid];
        let Some(vidpid_nk) = hive.navigate_to(&vidpid_path).unwrap_or(None) else {
            continue;
        };
        let serial_keys = hive
            .read_subkey_names_from_nk(&vidpid_nk)
            .unwrap_or_default();
        for raw_serial in serial_keys {
            let Some(&idx) = index_by_raw.get(&raw_serial) else {
                continue;
            };
            let serial_path = ["Enum", "USB", &vidpid, &raw_serial];
            let Some(serial_nk) = hive.navigate_to(&serial_path).unwrap_or(None) else {
                continue;
            };
            let values = hive.read_all_values_from_nk(&serial_nk).unwrap_or_default();
            if let Some(device_desc) = values
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("DeviceDesc"))
                .and_then(|(_, value)| match value {
                    RegistryValue::String(s) => Some(s.clone()),
                    _ => None,
                })
            {
                let entry = &mut entries[idx];
                let fallback = usb_device_fallback_name(
                    &entry.vendor,
                    &entry.product,
                    "Disk&Ven_Unknown&Prod_Unknown&Rev_1.00",
                );
                if entry.device_name == fallback {
                    entry.device_name = device_desc;
                }
            }
        }
    }
    Ok(())
}

/// Extract mounted-device mappings from `SYSTEM\MountedDevices`.
pub fn extract_mounted_devices_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<MountedDeviceEntry>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut entries = Vec::new();

    let Some(mounted_nk) = hive.navigate_to(&["MountedDevices"]).unwrap_or(None) else {
        return Ok(entries);
    };
    let values = hive
        .read_all_values_from_nk(&mounted_nk)
        .unwrap_or_default();

    for (device_name, value) in values {
        let drive_letter = device_name
            .strip_prefix("\\DosDevices\\")
            .map(str::to_string);
        let volume_guid = device_name
            .strip_prefix("\\\\?\\Volume{")
            .and_then(|rest| rest.split('}').next())
            .map(str::to_string);

        let (disk_signature_hex, target_name) = match value {
            RegistryValue::Binary(bytes) => (Some(hex::encode(bytes)), None),
            RegistryValue::String(s) => (None, Some(s)),
            _ => (None, None),
        };

        entries.push(MountedDeviceEntry {
            device_name,
            drive_letter,
            volume_guid,
            disk_signature_hex,
            target_name,
        });
    }

    Ok(entries)
}

/// Extract the shutdown time from `SYSTEM\<ControlSet>\Control\Windows\ShutdownTime`.
///
/// The value may be stored as `REG_BINARY` (8-byte FILETIME) or `REG_QWORD`; both are
/// accepted and converted to an RFC 3339 timestamp.
pub fn extract_shutdown_time_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<ShutdownTimeEntry>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut warnings = Vec::new();
    let control_sets = hive.control_set_candidates(&mut warnings);
    let mut entries = Vec::new();

    for control_set in control_sets {
        let key_path = [control_set.as_str(), "Control", "Windows"];
        let shutdown_time = match hive.lookup_value(&key_path, "ShutdownTime") {
            Ok(Some(RegistryValue::Binary(data))) if data.len() >= 8 => {
                let filetime = u64::from_le_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]);
                super::windows_filetime_to_rfc3339(filetime)
            }
            Ok(Some(RegistryValue::Qword(filetime))) => {
                super::windows_filetime_to_rfc3339(filetime)
            }
            Ok(Some(other)) => {
                warnings.push(format!(
                    "{}\\ShutdownTime has unsupported type: {:?}",
                    key_path.join("\\"),
                    other
                ));
                None
            }
            Ok(None) => {
                warnings.push(format!("{}\\ShutdownTime not found", key_path.join("\\")));
                None
            }
            Err(err) => {
                warnings.push(format!(
                    "{}\\ShutdownTime parse error: {}",
                    key_path.join("\\"),
                    err
                ));
                None
            }
        };

        if let Some(shutdown_time) = shutdown_time {
            entries.push(ShutdownTimeEntry {
                key_path: key_path.join("\\"),
                shutdown_time,
            });
        }
    }

    Ok(entries)
}

/// Extract AppCompatCache (ShimCache) entries from the SYSTEM hive.
///
/// The parser is fail-closed: it returns whatever entries it can parse and never
/// panics on an unknown format. It primarily supports the Windows 10/11 format
/// (header magic `0x30` / `0x34`, entry signature `"10ts"`) and falls back to
/// scanning for embedded UTF-16LE paths when the structured parser cannot make
/// progress.
pub fn extract_shimcache_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<ShimCacheEntry>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut warnings = Vec::new();
    let control_sets = hive.control_set_candidates(&mut warnings);
    let mut entries = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for control_set in control_sets {
        let key_path = [
            control_set.as_str(),
            "Control",
            "Session Manager",
            "AppCompatCache",
        ];
        let source_key_path = key_path.join("\\");
        let app_compat = match hive.lookup_value(&key_path, "AppCompatCache") {
            Ok(Some(RegistryValue::Binary(data))) => data,
            Ok(Some(other)) => {
                warnings.push(format!(
                    "{}\\AppCompatCache has unsupported type: {:?}",
                    source_key_path, other
                ));
                continue;
            }
            Ok(None) => {
                warnings.push(format!("{}\\AppCompatCache not found", source_key_path));
                continue;
            }
            Err(err) => {
                warnings.push(format!(
                    "{}\\AppCompatCache parse error: {}",
                    source_key_path, err
                ));
                continue;
            }
        };

        let parsed = parse_shimcache_entries(&app_compat, &source_key_path);
        for entry in parsed {
            if seen_paths.insert(entry.path.clone()) {
                entries.push(entry);
            }
        }
    }

    Ok(entries)
}

fn parse_shimcache_entries(data: &[u8], source_key_path: &str) -> Vec<ShimCacheEntry> {
    const WIN10_MAGIC: &[u8; 4] = b"10ts";
    const WIN8_MAGIC: &[u8; 4] = b"00ts";

    // Known header sizes after which entries begin. Try the most common first.
    let header_candidates = [0x30usize, 0x34, 0x80, 0x14];
    for header_size in header_candidates {
        if data.len() >= header_size + 12 {
            let entries =
                parse_shimcache_entry_stream(&data[header_size..], source_key_path, WIN10_MAGIC);
            if !entries.is_empty() {
                return entries;
            }
            let entries =
                parse_shimcache_entry_stream(&data[header_size..], source_key_path, WIN8_MAGIC);
            if !entries.is_empty() {
                return entries;
            }
        }
    }

    // No structured stream found at a known offset; scan for entry signatures.
    for magic in [WIN10_MAGIC, WIN8_MAGIC] {
        let entries = parse_shimcache_entry_stream(data, source_key_path, magic);
        if !entries.is_empty() {
            return entries;
        }
    }

    // Final fallback: extract any embedded UTF-16LE paths from the blob.
    extract_shimcache_paths_fallback(data, source_key_path)
}

fn parse_shimcache_entry_stream(
    mut data: &[u8],
    source_key_path: &str,
    entry_magic: &[u8; 4],
) -> Vec<ShimCacheEntry> {
    let mut entries = Vec::new();

    while data.len() >= 14 {
        // Find the next entry signature if we are not already aligned on one.
        if &data[..4] != entry_magic {
            if let Some(pos) = data.windows(4).position(|w| w == entry_magic) {
                data = &data[pos..];
            } else {
                break;
            }
        }
        if data.len() < 14 {
            break;
        }

        // Layout for Win10/8.x entries:
        //   0..4   signature
        //   4..8   unknown
        //   8..12  entry length (u32 LE)
        //   12..14 path length (u16 LE)
        //   path   UTF-16LE path
        //   8 bytes FILETIME last modified
        //   2 bytes data length
        //   data_length - 2 bytes data values
        //   2 bytes execution flag
        //   2 bytes padding
        let entry_len = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        if entry_len < 26 || entry_len > data.len() {
            // Skip the signature and continue scanning.
            data = &data[4..];
            continue;
        }
        let path_len = u16::from_le_bytes([data[12], data[13]]) as usize;
        if 14 + path_len + 8 > entry_len {
            data = &data[4..];
            continue;
        }
        let path_bytes = &data[14..14 + path_len];
        let path = decode_shimcache_path(path_bytes);

        let filetime_offset = 14 + path_len;
        let last_modified = if filetime_offset + 8 <= entry_len {
            let filetime = u64::from_le_bytes([
                data[filetime_offset],
                data[filetime_offset + 1],
                data[filetime_offset + 2],
                data[filetime_offset + 3],
                data[filetime_offset + 4],
                data[filetime_offset + 5],
                data[filetime_offset + 6],
                data[filetime_offset + 7],
            ]);
            super::windows_filetime_to_rfc3339(filetime)
        } else {
            None
        };

        if !path.is_empty() {
            entries.push(ShimCacheEntry {
                path,
                last_modified,
                source_key_path: source_key_path.to_string(),
            });
        }

        data = &data[entry_len..];
    }

    entries
}

fn decode_shimcache_path(bytes: &[u8]) -> String {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return String::new();
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16_lossy(&units);
    s.trim_end_matches('\0').to_string()
}

fn extract_shimcache_paths_fallback(data: &[u8], source_key_path: &str) -> Vec<ShimCacheEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Scan the blob in 2-byte steps looking for plausible UTF-16LE path runs.
    let mut index = 0;
    while index + 2 <= data.len() {
        // A path must contain a backslash and common Windows prefixes.
        let window = &data[index..];
        if let Some(path) = decode_utf16le_path(window) {
            let advance = path.encode_utf16().count() * 2 + 2;
            if path.len() >= 4 && seen.insert(path.clone()) {
                entries.push(ShimCacheEntry {
                    path,
                    last_modified: None,
                    source_key_path: source_key_path.to_string(),
                });
            }
            // Advance by at least the decoded path length in bytes to avoid loops.
            index += advance;
        } else {
            index += 2;
        }
    }

    entries
}

fn decode_utf16le_path(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }
    // Decode until a null unit or non-printable character.
    let mut units = Vec::new();
    for chunk in data.chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            break;
        }
        if unit < 0x20 || unit == 0xFFFD {
            return None;
        }
        units.push(unit);
    }
    if units.len() < 4 {
        return None;
    }
    let s = String::from_utf16_lossy(&units);
    let lower = s.to_ascii_lowercase();
    if !lower.contains('\\')
        && !lower.starts_with("c:\\")
        && !lower.starts_with("\\??\\")
        && !lower.starts_with("system32")
        && !lower.starts_with("windows")
    {
        return None;
    }
    Some(s)
}

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
                    .and_then(super::windows_filetime_to_rfc3339),
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

/// Like [`extract_system_hive_fields`], but after standard extraction checks a
/// transaction log for more recent writes.  When a txlog entry holds a newer
/// value (higher sequence number), the field's value is overwritten.
pub fn extract_system_hive_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<SystemHiveInfo, RegistryError> {
    let mut info = extract_system_hive_fields(bytes, hive_path)?;
    let txlog = parse_transaction_log(txlog_data)?;
    let mut txlog_applied = false;
    let mut ts_infos: Vec<TxlogTimestampInfo> = Vec::new();

    if let Some(ref mut field) = info.computer_name {
        let ts = apply_single_txlog_override(field, &txlog.transactions);
        txlog_applied = txlog_applied || ts.txlog_used;
        ts_infos.push(ts);
    }
    if let Some(ref mut field) = info.timezone {
        let ts = apply_single_txlog_override(field, &txlog.transactions);
        txlog_applied = txlog_applied || ts.txlog_used;
        ts_infos.push(ts);
    }

    info.txlog_applied = txlog_applied;
    info.txlog_timestamps = ts_infos;
    Ok(info)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::test_common::*;
    use super::*;
    use testing::{builders::registry as registry_fixture, fixtures};

    #[test]
    fn extract_system_fields_from_fixture() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Control",
            &[("ComputerName", 0x600), ("TimeZoneInformation", 0xa00)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", "LAB-PC", 0x1800);
        write_nk(&mut data, 0xa00, "TimeZoneInformation", &[], &[0xd00]);
        write_string_value(
            &mut data,
            0xd00,
            "TimeZoneKeyName",
            "China Standard Time",
            0x1900,
        );

        let info = extract_system_hive_fields(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(info.computer_name.unwrap().value, "LAB-PC");
        assert_eq!(info.timezone.unwrap().value, "China Standard Time");
    }

    #[test]
    fn extract_system_fields_falls_back_when_select_is_corrupt() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_vk(
            &mut data,
            0x1200,
            "Current",
            REG_DWORD,
            0x8000_0004,
            0x9530_7897,
        );
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("ComputerName", 0x600)], &[]);
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", "LAB-PC", 0x1800);

        let info = extract_system_hive_fields(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(info.computer_name.unwrap().value, "LAB-PC");
        assert!(info
            .warnings
            .iter()
            .any(|warning| warning.contains("Select\\Current")));
    }

    #[test]
    fn extract_system_fields_from_committed_tiny_fixture() {
        let bytes = std::fs::read(fixtures::tiny_registry_system_hive())
            .expect("read tiny SYSTEM registry fixture");

        let info = extract_system_hive_fields(&bytes, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(
            info.computer_name
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SYSTEM_COMPUTER_NAME)
        );
        assert_eq!(
            info.timezone.as_ref().map(|field| field.value.as_str()),
            Some(registry_fixture::SYSTEM_TIMEZONE)
        );
        assert!(info.warnings.is_empty());
    }

    // ── Txlog-override tests ───────────────────────────────────────────────

    use crate::registry::txlog::fixture::{build_synthetic_log1, SyntheticEntry};

    /// Build a minimal synthetic SYSTEM hive that has a ComputerName value.
    fn txlog_system_hive(computer_name: &str) -> Vec<u8> {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("ComputerName", 0x600)], &[]);
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", computer_name, 0x1800);
        data
    }

    #[test]
    fn system_hive_with_txlog_overrides_computer_name() {
        let hive_bytes = txlog_system_hive("OLD-PC");

        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 100,
            timestamp: Some(0x01DB_9F8C_0000_0000), // 2026-06-14 approx
            key_path:
                "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName"
                    .to_string(),
            value_name: Some("ComputerName".to_string()),
            data_before: Some(encode_utf16le("OLD-PC")),
            data_after: Some(encode_utf16le("NEW-PC")),
        }]);

        let info = extract_system_hive_fields_with_txlog(
            &hive_bytes,
            "Windows/System32/config/SYSTEM",
            &txlog_bytes,
        )
        .unwrap();

        let cn = info.computer_name.as_ref().unwrap();
        assert_eq!(
            cn.value, "NEW-PC",
            "ComputerName should be overridden by txlog"
        );
        assert!(info.txlog_applied, "txlog_applied should be true");
        assert_eq!(info.txlog_timestamps.len(), 1);
        let ts = &info.txlog_timestamps[0];
        assert_eq!(ts.field_name, "ComputerName");
        assert!(ts.txlog_used);
        assert!(ts.txlog_timestamp.is_some());
        assert!(ts.hive_timestamp.is_none());
    }

    #[test]
    fn system_hive_with_txlog_no_match_leaves_field_unchanged() {
        let hive_bytes = txlog_system_hive("ORIGINAL-PC");

        // Txlog entry for a completely different key — should not match.
        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 1,
            timestamp: Some(0x01DB_9F8C_0000_0000),
            key_path: "\\Registry\\Machine\\SOFTWARE\\Some\\Other\\Path".to_string(),
            value_name: Some("Unrelated".to_string()),
            data_before: None,
            data_after: Some(encode_utf16le("ignored")),
        }]);

        let info = extract_system_hive_fields_with_txlog(
            &hive_bytes,
            "Windows/System32/config/SYSTEM",
            &txlog_bytes,
        )
        .unwrap();

        let cn = info.computer_name.as_ref().unwrap();
        assert_eq!(
            cn.value, "ORIGINAL-PC",
            "ComputerName should stay unchanged"
        );
        assert!(!info.txlog_applied);
        let ts = &info.txlog_timestamps[0];
        assert_eq!(ts.field_name, "ComputerName");
        assert!(!ts.txlog_used);
        assert!(ts.txlog_timestamp.is_none());
    }

    // ── Service extraction tests ───────────────────────────────────────────────

    fn services_hive() -> Vec<u8> {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x0e00]);
        write_dword_value(&mut data, 0x0e00, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Services", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "Services",
            &[("TestSvc", 0x700), ("SvchostSvc", 0x900), ("DrvSvc", 0xa00)],
            &[],
        );

        // Value cells are placed in the 0x1000-0x1fff range (abs 0x2000-0x2fff),
        // below the subkey-list region that starts at abs 0x3000.
        // String data is placed at 0x3000+ (abs 0x4000+), safely above subkey lists.

        // TestSvc: own-process, delayed auto-start, fully populated values.
        write_nk(
            &mut data,
            0x700,
            "TestSvc",
            &[],
            &[
                0x1000, 0x1100, 0x1200, 0x1300, 0x1400, 0x1500, 0x1600, 0x1700,
            ],
        );
        write_dword_value(&mut data, 0x1000, "Type", 0x10);
        write_dword_value(&mut data, 0x1100, "Start", 2);
        write_dword_value(&mut data, 0x1200, "ErrorControl", 1);
        write_dword_value(&mut data, 0x1300, "DelayedAutoStart", 1);
        write_string_value(
            &mut data,
            0x1400,
            "ImagePath",
            "C:\\Windows\\svc.exe",
            0x3000,
        );
        write_string_value(&mut data, 0x1500, "DisplayName", "Test Service", 0x3100);
        write_string_value(&mut data, 0x1600, "Group", "Network", 0x3200);
        write_string_value(&mut data, 0x1700, "ObjectName", "LocalSystem", 0x3300);

        // SvchostSvc: share-process with Parameters\ServiceDll.
        write_nk(
            &mut data,
            0x900,
            "SvchostSvc",
            &[("Parameters", 0xb00)],
            &[0x1800, 0x1900, 0x1a00],
        );
        write_dword_value(&mut data, 0x1800, "Type", 0x20);
        write_dword_value(&mut data, 0x1900, "Start", 2);
        write_typed_string_value(
            &mut data,
            0x1a00,
            "ImagePath",
            REG_EXPAND_SZ,
            "%SystemRoot%\\system32\\svchost.exe -k netsvcs",
            0x3400,
        );
        write_nk(&mut data, 0xb00, "Parameters", &[], &[0x1b00]);
        write_string_value(
            &mut data,
            0x1b00,
            "ServiceDll",
            "C:\\Windows\\System32\\wuauserv.dll",
            0x3500,
        );

        // DrvSvc: kernel driver, boot start.
        write_nk(&mut data, 0xa00, "DrvSvc", &[], &[0x1c00, 0x1d00]);
        write_dword_value(&mut data, 0x1c00, "Type", 1);
        write_dword_value(&mut data, 0x1d00, "Start", 0);

        data
    }

    #[test]
    fn extract_services_maps_type_and_start() {
        let data = services_hive();
        let info =
            extract_services_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

        let test = info
            .services
            .iter()
            .find(|s| s.service_name == "TestSvc")
            .expect("TestSvc present");
        assert_eq!(test.service_type, ServiceType::Win32OwnProcess);
        assert_eq!(test.start_type, ServiceStartType::AutomaticDelayed);
        assert!(test.delayed_auto_start);
        assert_eq!(test.error_control, Some(1));
        assert_eq!(test.image_path.as_deref(), Some("C:\\Windows\\svc.exe"));
        assert_eq!(test.display_name.as_deref(), Some("Test Service"));
        assert_eq!(test.group.as_deref(), Some("Network"));
        assert_eq!(test.object_name.as_deref(), Some("LocalSystem"));
    }

    #[test]
    fn extract_services_resolves_svchost_service_dll() {
        let data = services_hive();
        let info =
            extract_services_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

        let svc = info
            .services
            .iter()
            .find(|s| s.service_name == "SvchostSvc")
            .expect("SvchostSvc present");
        assert_eq!(svc.service_type, ServiceType::Win32ShareProcess);
        assert_eq!(svc.start_type, ServiceStartType::Automatic);
        assert_eq!(
            svc.image_path.as_deref(),
            Some("%SystemRoot%\\system32\\svchost.exe -k netsvcs")
        );
        assert_eq!(
            svc.service_dll.as_deref(),
            Some("C:\\Windows\\System32\\wuauserv.dll")
        );
    }

    #[test]
    fn extract_services_maps_kernel_driver() {
        let data = services_hive();
        let info =
            extract_services_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

        let drv = info
            .services
            .iter()
            .find(|s| s.service_name == "DrvSvc")
            .expect("DrvSvc present");
        assert_eq!(drv.service_type, ServiceType::KernelDriver);
        assert_eq!(drv.start_type, ServiceStartType::Boot);
    }

    #[test]
    fn extract_services_deduplicates_across_control_sets() {
        let mut data = services_hive();
        // Add a second control set with the same Services subkeys to ensure
        // each service is reported only once.
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[
                ("Select", 0x200),
                ("ControlSet001", 0x300),
                ("ControlSet002", 0xc00),
            ],
            &[],
        );
        write_nk(
            &mut data,
            0xc00,
            "ControlSet002",
            &[("Services", 0xd00)],
            &[],
        );
        write_nk(
            &mut data,
            0xd00,
            "Services",
            &[("TestSvc", 0xe00), ("ExtraSvc", 0xf00)],
            &[],
        );
        write_nk(&mut data, 0xe00, "TestSvc", &[], &[0x6000, 0x6100]);
        write_dword_value(&mut data, 0x6000, "Type", 0x10);
        write_dword_value(&mut data, 0x6100, "Start", 3);
        write_nk(&mut data, 0xf00, "ExtraSvc", &[], &[0x6200]);
        write_dword_value(&mut data, 0x6200, "Type", 0x10);

        let info =
            extract_services_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();
        let test_count = info
            .services
            .iter()
            .filter(|s| s.service_name == "TestSvc")
            .count();
        assert_eq!(test_count, 1);
        assert!(info.services.iter().any(|s| s.service_name == "ExtraSvc"));
    }

    #[test]
    fn txlog_uses_highest_sequence_number() {
        // When multiple txlog entries match the same field, use the one with
        // the highest sequence number.
        let hive_bytes = txlog_system_hive("V1");

        let txlog_bytes = build_synthetic_log1(&[
            SyntheticEntry {
                operation: 2,
                sequence_number: 10,
                timestamp: Some(0x01DB_9F8C_0000_0000),
                key_path: "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName".to_string(),
                value_name: Some("ComputerName".to_string()),
                data_before: Some(encode_utf16le("V1")),
                data_after: Some(encode_utf16le("V2")),
            },
            SyntheticEntry {
                operation: 2,
                sequence_number: 20, // higher seq → should win
                timestamp: Some(0x01DB_A000_0000_0000),
                key_path: "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName".to_string(),
                value_name: Some("ComputerName".to_string()),
                data_before: Some(encode_utf16le("V2")),
                data_after: Some(encode_utf16le("V3")),
            },
        ]);

        let info = extract_system_hive_fields_with_txlog(
            &hive_bytes,
            "Windows/System32/config/SYSTEM",
            &txlog_bytes,
        )
        .unwrap();

        assert_eq!(info.computer_name.as_ref().unwrap().value, "V3");
    }

    // ── LSA packages extraction tests ──────────────────────────────────────────

    #[test]
    fn extract_lsa_packages_from_fixture() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("Lsa", 0x500)], &[]);
        write_nk(&mut data, 0x500, "Lsa", &[], &[0x1300, 0x1380, 0x1400]);
        write_multi_string_value(
            &mut data,
            0x1300,
            "Authentication Packages",
            &["msv1_0.dll", " Kerberos.dll"],
            0x4000,
        );
        write_multi_string_value(
            &mut data,
            0x1380,
            "Notification Packages",
            &["scecli.dll"],
            0x4100,
        );
        write_multi_string_value(
            &mut data,
            0x1400,
            "Security Packages",
            &["negotiate.dll", "secur32.dll"],
            0x4200,
        );

        let packages =
            extract_lsa_packages_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(packages.len(), 1);
        let pkg = &packages[0];
        assert_eq!(pkg.control_set, "ControlSet001");
        assert_eq!(
            pkg.authentication_packages,
            vec!["msv1_0.dll", " Kerberos.dll"]
        );
        assert_eq!(pkg.notification_packages, vec!["scecli.dll"]);
        assert_eq!(pkg.security_packages, vec!["negotiate.dll", "secur32.dll"]);
    }

    // ── USB / MountedDevices extraction tests ────────────────────────────────────

    fn usbstor_hive() -> Vec<u8> {
        let mut data = empty_hive("SYSTEM");
        write_nk(&mut data, 0x20, "SYSTEM", &[("Enum", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Enum", &[("USBSTOR", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "USBSTOR",
            &[("Disk&Ven_Kingston&Prod_DT101&Rev_1.00", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Disk&Ven_Kingston&Prod_DT101&Rev_1.00",
            &[("A1B2C3D4E5F6&0", 0x500)],
            &[],
        );
        write_nk(&mut data, 0x500, "A1B2C3D4E5F6&0", &[], &[0x1000]);
        write_string_value(
            &mut data,
            0x1000,
            "FriendlyName",
            "Kingston DT101 USB Device",
            0x3000,
        );

        // Non-zero FILETIMEs for class (first connect) and serial (last connect).
        set_nk_last_write(&mut data, 0x400, 0x01DB_9F8C_0000_0000);
        set_nk_last_write(&mut data, 0x500, 0x01DB_A000_0000_0000);

        data
    }

    fn set_nk_last_write(data: &mut [u8], offset: u32, filetime: u64) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        data[abs + 0x08..abs + 0x10].copy_from_slice(&filetime.to_le_bytes());
    }

    #[test]
    fn extract_usb_devices_from_system_hive_parses_class_and_serial() {
        let data = usbstor_hive();
        let entries =
            extract_usb_devices_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.device_name, "Kingston DT101 USB Device");
        assert_eq!(entry.serial_number, "A1B2C3D4E5F6");
        assert_eq!(entry.raw_serial_number, "A1B2C3D4E5F6&0");
        assert_eq!(entry.vendor.as_deref(), Some("Kingston"));
        assert_eq!(entry.product.as_deref(), Some("DT101"));
        assert_eq!(entry.revision.as_deref(), Some("1.00"));
        assert!(entry
            .first_connect
            .as_deref()
            .unwrap()
            .starts_with("2025-03-28"));
        assert!(entry
            .last_connect
            .as_deref()
            .unwrap()
            .starts_with("2025-03-28"));
    }

    #[test]
    fn extract_mounted_devices_from_system_hive_parses_dos_and_volume() {
        let mut data = empty_hive("SYSTEM");
        write_nk(&mut data, 0x20, "SYSTEM", &[("MountedDevices", 0x200)], &[]);
        write_nk(&mut data, 0x200, "MountedDevices", &[], &[0x1000, 0x1100]);
        write_binary_value(
            &mut data,
            0x1000,
            r"\DosDevices\C:",
            &[0xDE, 0xAD, 0xBE, 0xEF],
            0x3000,
        );
        write_binary_value(
            &mut data,
            0x1100,
            r"\\?\Volume{12345678-1234-1234-1234-123456789abc}",
            &[0xCA, 0xFE, 0xBA, 0xBE],
            0x3100,
        );

        let entries =
            extract_mounted_devices_from_system_hive(&data, "Windows/System32/config/SYSTEM")
                .unwrap();

        assert_eq!(entries.len(), 2);
        let dos = entries
            .iter()
            .find(|e| e.device_name == r"\DosDevices\C:")
            .expect("DOS device entry");
        assert_eq!(dos.drive_letter.as_deref(), Some("C:"));
        assert_eq!(dos.volume_guid.as_ref(), None);
        assert_eq!(dos.disk_signature_hex.as_deref(), Some("deadbeef"));

        let vol = entries
            .iter()
            .find(|e| e.device_name.starts_with(r"\\?\Volume{"))
            .expect("volume entry");
        assert_eq!(vol.drive_letter.as_ref(), None);
        assert_eq!(
            vol.volume_guid.as_deref(),
            Some("12345678-1234-1234-1234-123456789abc")
        );
        assert_eq!(vol.disk_signature_hex.as_deref(), Some("cafebabe"));
    }

    // ── ShutdownTime / ShimCache extraction tests ────────────────────────────────

    fn make_shutdown_time_hive(filetime: u64) -> Vec<u8> {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("Windows", 0x500)], &[]);
        write_nk(&mut data, 0x500, "Windows", &[], &[0x1300]);
        write_binary_value(
            &mut data,
            0x1300,
            "ShutdownTime",
            &filetime.to_le_bytes(),
            0x4000,
        );
        data
    }

    #[test]
    fn extract_shutdown_time_from_fixture() {
        let filetime = 0x01DB_A000_0000_0000u64;
        let data = make_shutdown_time_hive(filetime);

        let entries =
            extract_shutdown_time_from_system_hive(&data, "Windows/System32/config/SYSTEM")
                .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key_path, "ControlSet001\\Control\\Windows");
        assert!(entries[0].shutdown_time.starts_with("2025-03-28"));
    }

    #[test]
    fn extract_shutdown_time_accepts_qword_value() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("Windows", 0x500)], &[]);
        write_nk(&mut data, 0x500, "Windows", &[], &[0x1300]);
        let filetime = 0x01DB_A000_0000_0000u64;
        write_qword_value(&mut data, 0x1300, "ShutdownTime", filetime, 0x4000);

        let entries =
            extract_shutdown_time_from_system_hive(&data, "Windows/System32/config/SYSTEM")
                .unwrap();

        assert_eq!(entries.len(), 1);
        assert!(entries[0].shutdown_time.starts_with("2025-03-28"));
    }

    fn make_win10_shimcache_blob(path: &str) -> Vec<u8> {
        let mut header = vec![0u8; 0x30];
        header[0..4].copy_from_slice(&0x30u32.to_le_bytes());
        let path_utf16: Vec<u8> = path.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let path_len = path_utf16.len();
        let data_len = 4u16;
        let entry_len = 26 + path_len + data_len as usize;
        let mut entry = Vec::with_capacity(entry_len);
        entry.extend_from_slice(b"10ts");
        entry.extend_from_slice(&0u32.to_le_bytes()); // unknown
        entry.extend_from_slice(&(entry_len as u32).to_le_bytes());
        entry.extend_from_slice(&(path_len as u16).to_le_bytes());
        entry.extend_from_slice(&path_utf16);
        let filetime = 0x01DB_9F8C_0000_0000u64;
        entry.extend_from_slice(&filetime.to_le_bytes());
        entry.extend_from_slice(&data_len.to_le_bytes());
        entry.extend_from_slice(&0u16.to_le_bytes()); // data values (2 bytes when data_len == 4)
        entry.extend_from_slice(&0u16.to_le_bytes()); // execution flag
        entry.extend_from_slice(&0u16.to_le_bytes()); // padding
        header.extend(entry);
        header
    }

    fn make_shimcache_hive(blob: &[u8]) -> Vec<u8> {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Control",
            &[("Session Manager", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "Session Manager",
            &[("AppCompatCache", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "AppCompatCache", &[], &[0x1300]);
        write_binary_value(&mut data, 0x1300, "AppCompatCache", blob, 0x4000);
        data
    }

    #[test]
    fn extract_shimcache_from_fixture() {
        // Keep the path short so the whole synthetic blob fits into the
        // default 128-byte binary data cell used by write_binary_value.
        let blob = make_win10_shimcache_blob(r"C:\Windows\cmd.exe");
        let data = make_shimcache_hive(&blob);

        let entries =
            extract_shimcache_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, r"C:\Windows\cmd.exe");
        assert!(entries[0]
            .last_modified
            .as_deref()
            .unwrap()
            .starts_with("2025-03-28"));
        assert_eq!(
            entries[0].source_key_path,
            "ControlSet001\\Control\\Session Manager\\AppCompatCache"
        );
    }

    #[test]
    fn extract_shimcache_fallback_embedded_paths() {
        // No valid header/entry stream, but the blob contains a UTF-16LE Windows path.
        let path = r"C:\Windows\explorer.exe";
        let mut blob: Vec<u8> = vec![0u8; 0x20];
        blob.extend(path.encode_utf16().flat_map(u16::to_le_bytes));
        blob.extend_from_slice(&[0x00, 0x00]);
        let data = make_shimcache_hive(&blob);

        let entries =
            extract_shimcache_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert!(entries.iter().any(|e| e.path == path));
    }
}
