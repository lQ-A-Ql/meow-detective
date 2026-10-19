use super::{
    types::{MAX_DEVICES, MAX_TEXT_BYTES},
    RegistryHardwareInfo, RegistryHardwareRecord,
};
use crate::registry::lookup::{
    reader::RegistryHiveReader,
    types::{NkRecord, RegistryValue},
};
use std::collections::{BTreeMap, HashMap};

const DISPLAY_CLASS: &str = "{4D36E968-E325-11CE-BFC1-08002BE10318}";

pub(super) fn extract(
    hive: &RegistryHiveReader<'_>,
    control: &str,
    info: &mut RegistryHardwareInfo,
) {
    let class = [control, "Control", "Class", DISPLAY_CLASS].join("\\");
    for (name, key) in children(hive, &class) {
        if name.len() == 4 && name.bytes().all(|b| b.is_ascii_digit()) {
            graphics(hive, &key, &format!("{class}\\{name}"), info);
        }
    }
    if !info.devices.iter().any(|record| record.kind == "gpu") {
        let video = [control, "Control", "Video"].join("\\");
        for (name, _) in children(hive, &video) {
            let path = format!("{video}\\{name}\\0000");
            if let Ok(Some(key)) = hive.navigate_to(&path.split('\\').collect::<Vec<_>>()) {
                graphics(hive, &key, &path, info);
            }
        }
    }
    if !info.devices.iter().any(|record| record.kind == "cpu") {
        processors(hive, control, info);
    }
    let path = [control, "Control", "SystemInformation"].join("\\");
    let Ok(Some(key)) = hive.navigate_to(&path.split('\\').collect::<Vec<_>>()) else {
        return;
    };
    let values = values(hive, &key);
    for (kind, product, vendor) in [
        ("system", "SystemProductName", "SystemManufacturer"),
        ("baseboard", "BaseBoardProduct", "BaseBoardManufacturer"),
        ("firmware", "BIOSVersion", "BIOSVendor"),
    ] {
        if info.devices.iter().any(|record| record.kind == kind) {
            continue;
        }
        if let Some(name) = text(&values, product) {
            let mut record =
                RegistryHardwareRecord::new(kind, format!("registry:{kind}"), path.clone());
            record.name = name;
            record.manufacturer = text(&values, vendor);
            if kind == "firmware" {
                record.property("releaseDate", text(&values, "BIOSReleaseDate"));
            }
            if kind == "system" {
                record.property("sku", text(&values, "SystemSKU"));
                record.property("family", text(&values, "SystemFamily"));
            }
            push(info, record);
        }
    }
}

fn graphics(
    hive: &RegistryHiveReader<'_>,
    key: &NkRecord,
    path: &str,
    info: &mut RegistryHardwareInfo,
) {
    let values = values(hive, key);
    let Some(name) =
        text(&values, "DriverDesc").or_else(|| text(&values, "HardwareInformation.AdapterString"))
    else {
        return;
    };
    let mut record = RegistryHardwareRecord::new("gpu", format!("registry:{path}"), path.into());
    record.name = name;
    record.manufacturer = text(&values, "ProviderName");
    for (property, value_name) in [
        ("driverVersion", "DriverVersion"),
        ("driverDate", "DriverDate"),
        ("chipType", "HardwareInformation.ChipType"),
        ("biosVersion", "HardwareInformation.BiosString"),
        ("matchingDeviceId", "MatchingDeviceId"),
    ] {
        record.property(property, text(&values, value_name));
    }
    let capacity = number(&values, "HardwareInformation.qwMemorySize")
        .or_else(|| number(&values, "HardwareInformation.MemorySize"));
    record.property("capacityBytes", capacity.filter(|value| *value > 0));
    push(info, record);
}

fn processors(hive: &RegistryHiveReader<'_>, control: &str, info: &mut RegistryHardwareInfo) {
    let path = [control, "Enum", "ACPI"].join("\\");
    let mut models: HashMap<String, (RegistryHardwareRecord, usize)> = HashMap::new();
    let mut scanned = 0;
    for (model_name, _) in children(hive, &path) {
        if scanned >= MAX_DEVICES {
            break;
        }
        let model_path = format!("{path}\\{model_name}");
        for (instance, key) in children(hive, &model_path) {
            if scanned >= MAX_DEVICES {
                break;
            }
            scanned += 1;
            let values = values(hive, &key);
            if !text(&values, "ClassGUID").is_some_and(|value| {
                value.eq_ignore_ascii_case("{50127DC3-0F36-415E-A6CC-4CB3BE910B65}")
            }) {
                continue;
            }
            let Some(name) = text(&values, "FriendlyName").or_else(|| text(&values, "DeviceDesc"))
            else {
                continue;
            };
            let entry = models.entry(name.to_ascii_lowercase()).or_insert_with(|| {
                let key_path = format!("{model_path}\\{instance}");
                let mut record =
                    RegistryHardwareRecord::new("cpu", format!("registry:{key_path}"), key_path);
                record.name = name;
                record.manufacturer = text(&values, "Mfg");
                (record, 0)
            });
            entry.1 += 1;
        }
    }
    let mut records = models.into_values().collect::<Vec<_>>();
    records.sort_by(|a, b| a.0.device_id.cmp(&b.0.device_id));
    for (mut record, count) in records {
        record.property("registryInstanceCount", Some(count));
        push(info, record);
    }
}

fn children(hive: &RegistryHiveReader<'_>, path: &str) -> Vec<(String, NkRecord)> {
    hive.navigate_to(&path.split('\\').collect::<Vec<_>>())
        .ok()
        .flatten()
        .filter(|key| key.num_subkeys <= MAX_DEVICES as u32)
        .and_then(|key| hive.read_subkeys_from_nk(&key).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|(name, _)| name.len() <= MAX_TEXT_BYTES)
        .take(MAX_DEVICES)
        .collect()
}

fn values(hive: &RegistryHiveReader<'_>, key: &NkRecord) -> BTreeMap<String, RegistryValue> {
    [
        "DriverDesc",
        "HardwareInformation.AdapterString",
        "ProviderName",
        "DriverVersion",
        "DriverDate",
        "HardwareInformation.ChipType",
        "HardwareInformation.BiosString",
        "MatchingDeviceId",
        "HardwareInformation.qwMemorySize",
        "HardwareInformation.MemorySize",
        "ClassGUID",
        "FriendlyName",
        "DeviceDesc",
        "Mfg",
        "SystemProductName",
        "SystemManufacturer",
        "BaseBoardProduct",
        "BaseBoardManufacturer",
        "BIOSVersion",
        "BIOSVendor",
        "BIOSReleaseDate",
        "SystemSKU",
        "SystemFamily",
    ]
    .into_iter()
    .filter_map(|name| {
        hive.read_bounded_value(key, name, MAX_TEXT_BYTES * 2)
            .ok()
            .flatten()
            .map(|value| (name.to_ascii_lowercase(), value))
    })
    .collect()
}

fn text(values: &BTreeMap<String, RegistryValue>, name: &str) -> Option<String> {
    let value = match values.get(&name.to_ascii_lowercase())? {
        RegistryValue::String(value) => value.clone(),
        RegistryValue::Binary(bytes)
            if bytes.len() <= MAX_TEXT_BYTES * 2 && bytes.len().is_multiple_of(2) =>
        {
            let units = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .take_while(|value| *value != 0)
                .collect::<Vec<_>>();
            String::from_utf16(&units).ok()?
        }
        _ => return None,
    };
    let value = value.trim_matches('\0').trim();
    (!value.is_empty() && value.len() <= MAX_TEXT_BYTES).then(|| value.to_string())
}

fn number(values: &BTreeMap<String, RegistryValue>, name: &str) -> Option<u64> {
    match values.get(&name.to_ascii_lowercase())? {
        RegistryValue::Dword(value) => Some(u64::from(*value)),
        RegistryValue::Qword(value) => Some(*value),
        RegistryValue::Binary(bytes) if bytes.len() == 4 => Some(u64::from(u32::from_le_bytes(
            bytes.as_slice().try_into().ok()?,
        ))),
        RegistryValue::Binary(bytes) if bytes.len() == 8 => {
            Some(u64::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        _ => None,
    }
}

fn push(info: &mut RegistryHardwareInfo, record: RegistryHardwareRecord) {
    if info.devices.len() < MAX_DEVICES {
        info.devices.push(record);
    } else if info.warnings.len() < 16 {
        info.warnings.push("hardware device budget exceeded".into());
    }
}
