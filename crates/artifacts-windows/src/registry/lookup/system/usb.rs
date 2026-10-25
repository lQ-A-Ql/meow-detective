use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

/// Extract USB device history from `SYSTEM\Enum\USBSTOR`.
pub fn extract_usb_devices_from_system_hive(
    bytes: &(impl crate::registry::RegistryHiveSource + ?Sized),
    _hive_path: &str,
) -> Result<Vec<UsbDeviceHistoryEntry>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut entries = Vec::new();

    // Windows stores the device-enumeration tree below the selected control
    // set.  A root-level Enum tree is retained as a narrow fallback for older
    // hives and the synthetic parser fixtures.
    let control_sets = hive.control_set_candidates(&mut Vec::new());
    let mut prefixes = control_sets
        .iter()
        .map(|name| vec![name.as_str(), "Enum"])
        .collect::<Vec<_>>();
    prefixes.push(vec!["Enum"]);

    let mappings = super::usb_mapping::UsbMappings::new(&hive, &prefixes);
    for prefix in prefixes {
        extract_usb_storage_tree(&hive, &prefix, &mappings, &mut entries)?;
        enrich_usb_devices_from_enum_usb(&hive, &prefix, &mut entries)?;
        extract_usb_enum_storage_devices(&hive, &prefix, &mappings, &mut entries)?;
    }

    Ok(entries)
}

fn extract_usb_enum_storage_devices(
    hive: &RegistryHiveReader<'_>,
    prefix: &[&str],
    mappings: &super::usb_mapping::UsbMappings,
    entries: &mut Vec<UsbDeviceHistoryEntry>,
) -> Result<(), String> {
    let scsi_names = extract_scsi_usb_names(hive, prefix);
    let mut usb_path = prefix.to_vec();
    usb_path.push("USB");
    let Some(usb_nk) = hive.navigate_to(&usb_path).unwrap_or(None) else {
        return Ok(());
    };

    for vid_pid in hive.read_subkey_names_from_nk(&usb_nk).unwrap_or_default() {
        let (vendor, product) = parse_usb_vid_pid(&vid_pid);
        let mut class_path = usb_path.clone();
        class_path.push(&vid_pid);
        let Some(class_nk) = hive.navigate_to(&class_path).unwrap_or(None) else {
            continue;
        };
        for raw_serial in hive
            .read_subkey_names_from_nk(&class_nk)
            .unwrap_or_default()
        {
            let mut instance_path = class_path.clone();
            instance_path.push(&raw_serial);
            let Some(instance_nk) = hive.navigate_to(&instance_path).unwrap_or(None) else {
                continue;
            };
            let values = hive
                .read_all_values_from_nk(&instance_nk)
                .unwrap_or_default();
            if !is_usb_mass_storage_instance(&values) {
                continue;
            }
            let instance_identity = format!("{vid_pid}\\{raw_serial}");
            let device_name = scsi_names
                .get(&instance_identity.to_ascii_lowercase())
                .cloned()
                .or_else(|| {
                    values
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("DeviceDesc"))
                        .and_then(|(_, value)| registry_string(value))
                        .map(|value| clean_device_description(&value))
                        .filter(|value| !value.is_empty())
                })
                .unwrap_or_else(|| usb_device_fallback_name(&vendor, &product, &vid_pid));
            let revision = values
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("HardwareID"))
                .and_then(|(_, value)| registry_string(value))
                .and_then(|value| parse_usb_revision(&value));
            let candidate = UsbDeviceHistoryEntry {
                device_name,
                serial_number: strip_usb_instance_suffix(&raw_serial),
                raw_serial_number: raw_serial.clone(),
                vendor: vendor.clone(),
                product: product.clone(),
                revision,
                first_connect: None,
                last_connect: instance_nk
                    .last_write_time
                    .and_then(windows_filetime_to_rfc3339),
                instance_id: format!(r"USB\{instance_identity}"),
                source_key_path: instance_path.join("\\"),
                volumes: mappings.volumes(
                    &format!(r"USB\{instance_identity}"),
                    &instance_path.join("\\"),
                ),
            };
            if !entries
                .iter()
                .any(|entry| same_usb_identity(entry, &candidate))
            {
                entries.push(candidate);
            }
        }
    }
    Ok(())
}

fn extract_scsi_usb_names(
    hive: &RegistryHiveReader<'_>,
    prefix: &[&str],
) -> std::collections::HashMap<String, String> {
    let mut scsi_path = prefix.to_vec();
    scsi_path.push("SCSI");
    let Some(scsi_nk) = hive.navigate_to(&scsi_path).ok().flatten() else {
        return std::collections::HashMap::new();
    };
    let mut names = std::collections::HashMap::new();
    for class_name in hive.read_subkey_names_from_nk(&scsi_nk).unwrap_or_default() {
        let mut class_path = scsi_path.clone();
        class_path.push(&class_name);
        let Some(class_nk) = hive.navigate_to(&class_path).ok().flatten() else {
            continue;
        };
        for instance_name in hive
            .read_subkey_names_from_nk(&class_nk)
            .unwrap_or_default()
        {
            let mut instance_path = class_path.clone();
            instance_path.push(&instance_name);
            let Some(usb_path) = read_scsi_usb_instance_path(hive, &instance_path) else {
                continue;
            };
            let Some(identity) = usb_path.strip_prefix("USB\\") else {
                continue;
            };
            let Some(name) = read_scsi_friendly_name(hive, &instance_path) else {
                continue;
            };
            names.insert(identity.to_ascii_lowercase(), name);
        }
    }
    names
}

fn read_scsi_usb_instance_path(
    hive: &RegistryHiveReader<'_>,
    instance_path: &[&str],
) -> Option<String> {
    read_scsi_property(
        hive,
        instance_path,
        "{83da6326-97a6-4088-9453-a1923f573b29}",
        "000A",
    )
}

fn read_scsi_friendly_name(
    hive: &RegistryHiveReader<'_>,
    instance_path: &[&str],
) -> Option<String> {
    read_scsi_property(
        hive,
        instance_path,
        "{540b947e-8b40-45bc-a8a2-6a0b894cbda2}",
        "0004",
    )
}

fn read_scsi_property(
    hive: &RegistryHiveReader<'_>,
    instance_path: &[&str],
    property_guid: &str,
    value_key: &str,
) -> Option<String> {
    let mut path = instance_path.to_vec();
    path.extend(["Properties", property_guid, value_key]);
    let key = hive.navigate_to(&path).ok().flatten()?;
    let value = hive.read_value(&key, "").ok().flatten()?;
    registry_value_text(&value)
}

fn registry_value_text(value: &RegistryValue) -> Option<String> {
    match value {
        RegistryValue::String(value) => Some(value.trim_matches('\0').trim().to_string()),
        RegistryValue::MultiString(values) => values.first().map(|value| value.trim().to_string()),
        RegistryValue::Binary(bytes) if bytes.len() >= 2 && bytes.len().is_multiple_of(2) => {
            let units = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|unit| *unit != 0)
                .collect::<Vec<_>>();
            String::from_utf16(&units)
                .ok()
                .map(|value| value.trim().to_string())
        }
        _ => None,
    }
}

fn parse_usb_vid_pid(name: &str) -> (Option<String>, Option<String>) {
    let vendor = name.split('&').find_map(|part| {
        part.get(..4)
            .filter(|prefix| prefix.eq_ignore_ascii_case("VID_"))
            .map(|_| part[4..].to_string())
    });
    let product = name.split('&').find_map(|part| {
        part.get(..4)
            .filter(|prefix| prefix.eq_ignore_ascii_case("PID_"))
            .map(|_| part[4..].to_string())
    });
    (vendor, product)
}

fn parse_usb_revision(hardware_id: &str) -> Option<String> {
    hardware_id.split('&').find_map(|part| {
        part.get(..4)
            .filter(|prefix| prefix.eq_ignore_ascii_case("REV_"))
            .map(|_| {
                part[4..]
                    .split(['\\', '\0'])
                    .next()
                    .unwrap_or(part)
                    .to_string()
            })
    })
}

fn registry_string(value: &RegistryValue) -> Option<String> {
    match value {
        RegistryValue::String(value) => Some(value.clone()),
        RegistryValue::MultiString(values) => values.first().cloned(),
        _ => None,
    }
}

fn clean_device_description(value: &str) -> String {
    value
        .rsplit_once(';')
        .map(|(_, description)| description)
        .unwrap_or(value)
        .trim()
        .to_string()
}

fn is_usb_mass_storage_instance(values: &[(String, RegistryValue)]) -> bool {
    values.iter().any(|(name, value)| {
        let Some(text) = registry_string(value) else {
            return false;
        };
        let text = text.to_ascii_lowercase();
        (name.eq_ignore_ascii_case("Service")
            && (text.contains("usbstor") || text.contains("uaspstor")))
            || ((name.eq_ignore_ascii_case("HardwareID")
                || name.eq_ignore_ascii_case("CompatibleIDs"))
                && (text.contains("class_08") || text.contains("usbstor")))
            || (name.eq_ignore_ascii_case("DeviceDesc")
                && (text.contains("mass storage") || text.contains("uas")))
    })
}

fn same_usb_identity(left: &UsbDeviceHistoryEntry, right: &UsbDeviceHistoryEntry) -> bool {
    left.raw_serial_number == right.raw_serial_number
        && left.vendor == right.vendor
        && left.product == right.product
        && left.revision == right.revision
}

fn extract_usb_storage_tree(
    hive: &RegistryHiveReader<'_>,
    prefix: &[&str],
    mappings: &super::usb_mapping::UsbMappings,
    entries: &mut Vec<UsbDeviceHistoryEntry>,
) -> Result<(), String> {
    let mut usbstor_path = prefix.to_vec();
    usbstor_path.push("USBSTOR");
    let Some(usbstor_nk) = hive.navigate_to(&usbstor_path).unwrap_or(None) else {
        return Ok(());
    };
    let class_keys = hive
        .read_subkey_names_from_nk(&usbstor_nk)
        .unwrap_or_default();

    for class_name in class_keys {
        let (vendor, product, revision) = parse_usbstor_class_name(&class_name);
        let mut class_path = usbstor_path.clone();
        class_path.push(&class_name);
        let Some(class_nk) = hive.navigate_to(&class_path).unwrap_or(None) else {
            continue;
        };
        let first_connect = class_nk
            .last_write_time
            .and_then(windows_filetime_to_rfc3339);
        let serial_keys = hive
            .read_subkey_names_from_nk(&class_nk)
            .unwrap_or_default();

        for raw_serial in serial_keys {
            let serial_number = strip_usb_instance_suffix(&raw_serial);
            let mut serial_path = class_path.clone();
            serial_path.push(&raw_serial);
            let Some(serial_nk) = hive.navigate_to(&serial_path).unwrap_or(None) else {
                continue;
            };
            let last_connect = serial_nk
                .last_write_time
                .and_then(windows_filetime_to_rfc3339);

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
            let candidate = UsbDeviceHistoryEntry {
                device_name,
                serial_number,
                raw_serial_number: raw_serial.clone(),
                vendor: vendor.clone(),
                product: product.clone(),
                revision: revision.clone(),
                first_connect: first_connect.clone(),
                last_connect,
                instance_id: format!(r"USBSTOR\{class_name}\{raw_serial}"),
                source_key_path: serial_path.join("\\"),
                volumes: mappings.volumes(
                    &format!(r"USBSTOR\{class_name}\{raw_serial}"),
                    &serial_path.join("\\"),
                ),
            };
            if !entries.iter().any(|entry| {
                entry.raw_serial_number == candidate.raw_serial_number
                    && entry.vendor == candidate.vendor
                    && entry.product == candidate.product
                    && entry.revision == candidate.revision
            }) {
                entries.push(candidate);
            }
        }
    }
    Ok(())
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
    prefix: &[&str],
    entries: &mut [UsbDeviceHistoryEntry],
) -> Result<(), String> {
    let index_by_raw: std::collections::HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| (entry.raw_serial_number.clone(), idx))
        .collect();
    let mut usb_path = prefix.to_vec();
    usb_path.push("USB");
    let Some(usb_nk) = hive.navigate_to(&usb_path).unwrap_or(None) else {
        return Ok(());
    };
    let vidpid_keys = hive.read_subkey_names_from_nk(&usb_nk).unwrap_or_default();

    for vidpid in vidpid_keys {
        let mut vidpid_path = usb_path.clone();
        vidpid_path.push(&vidpid);
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
            let mut serial_path = vidpid_path.clone();
            serial_path.push(&raw_serial);
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
