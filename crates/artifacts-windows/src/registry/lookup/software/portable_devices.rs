use super::super::usb_identity::volume_identity;
use super::super::{PortableDeviceEntry, RegistryHiveReader};
use super::values::read_optional_string_value;
use crate::registry::RegistryError;

/// WPD FriendlyName is a cached display/volume name, not a filesystem probe.
pub fn extract_portable_devices_from_software_hive(
    bytes: &(impl crate::registry::RegistryHiveSource + ?Sized),
    _hive_path: &str,
) -> Result<Vec<PortableDeviceEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let root = ["Microsoft", "Windows Portable Devices", "Devices"];
    let Some(key) = hive.navigate_to(&root)? else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    for name in hive.read_subkey_names_from_nk(&key)? {
        let Some(identity) = volume_identity(&name) else {
            continue;
        };
        let path = [root.as_slice(), &[name.as_str()]].concat();
        let Some(label) = read_optional_string_value(&hive, &path, "FriendlyName") else {
            continue;
        };
        let label = label.trim().to_string();
        // A bare drive path is a fallback display name, not a volume label.
        let bare = label.trim_end_matches(['\\', '/']);
        if bare.len() == 2 && bare.as_bytes()[0].is_ascii_alphabetic() && bare.ends_with(':') {
            continue;
        }
        entries.push(PortableDeviceEntry {
            volume_identity: identity,
            volume_label: label,
            source_key_path: format!("{}\\FriendlyName", path.join("\\")),
        });
    }
    Ok(entries)
}
