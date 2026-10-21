use super::super::usb_identity::{utf16_text, volume_identity};
use super::super::{MountedDeviceEntry, RegistryHiveReader, RegistryValue};

pub fn extract_mounted_devices_from_system_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<MountedDeviceEntry>, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    Ok(read_mounted_devices(&hive))
}

pub(super) fn read_mounted_devices(hive: &RegistryHiveReader<'_>) -> Vec<MountedDeviceEntry> {
    let Some(key) = hive.navigate_to(&["MountedDevices"]).ok().flatten() else {
        return Vec::new();
    };
    hive.read_all_values_from_nk(&key)
        .unwrap_or_default()
        .into_iter()
        .map(|(device_name, value)| {
            let lower = device_name.to_ascii_lowercase();
            let drive_letter = lower
                .strip_prefix(r"\dosdevices\")
                .filter(|v| {
                    v.len() == 2 && v.as_bytes()[0].is_ascii_alphabetic() && v.ends_with(':')
                })
                .map(str::to_ascii_uppercase);
            let volume_guid = lower
                .strip_prefix(r"\??\volume{")
                .or_else(|| lower.strip_prefix(r"\\?\volume{"))
                .and_then(|rest| rest.strip_suffix('}'))
                .and_then(|v| uuid::Uuid::parse_str(v).ok())
                .map(|v| v.to_string());
            let (disk_signature_hex, target_name) = match value {
                RegistryValue::Binary(bytes) => {
                    let text = utf16_text(&bytes).filter(|v| volume_identity(v).is_some());
                    (Some(hex::encode(bytes)), text)
                }
                RegistryValue::String(value) => (None, Some(value)),
                _ => (None, None),
            };
            MountedDeviceEntry {
                device_name,
                drive_letter,
                volume_guid,
                disk_signature_hex,
                target_name,
            }
        })
        .collect()
}
