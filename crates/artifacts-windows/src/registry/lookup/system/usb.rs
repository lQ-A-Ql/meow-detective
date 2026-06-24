use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

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
            .and_then(windows_filetime_to_rfc3339);
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
