use super::super::usb_identity::{utf16_text, volume_identity};
use super::super::{MountedDeviceEntry, RegistryHiveReader, RegistryValue, UsbVolumeEntry};
use super::partition_cache::partitions;
use std::collections::BTreeMap;

/// One snapshot per hive: no repeated hive/MountedDevices parsing per device.
pub(super) struct UsbMappings {
    mounted: Vec<MountedDeviceEntry>,
    disks: BTreeMap<String, Vec<UsbVolumeEntry>>,
}

impl UsbMappings {
    pub(super) fn new(hive: &RegistryHiveReader<'_>, prefixes: &[Vec<&str>]) -> Self {
        let mut result = Self {
            mounted: super::mounted::read_mounted_devices(hive),
            disks: BTreeMap::new(),
        };
        for prefix in prefixes {
            result.collect_disks(hive, prefix, "SCSI");
            result.collect_disks(hive, prefix, "USBSTOR");
        }
        result
    }

    pub(super) fn volumes(&self, instance_id: &str, source_key: &str) -> Vec<UsbVolumeEntry> {
        let identity = instance_id.to_ascii_lowercase();
        let normalized_identity = normalize_usb_identity(&identity);
        let mut volumes = self.disks.get(&identity).cloned().unwrap_or_default();
        if volumes.is_empty() {
            volumes = self
                .disks
                .iter()
                .filter(|(key, _)| normalize_usb_identity(key) == normalized_identity)
                .flat_map(|(_, value)| value.clone())
                .collect();
        }
        // Exact bus/class/instance identity; never trim serial suffixes or use VID/PID as vendor text.
        let direct: Vec<_> = self
            .mounted
            .iter()
            .filter_map(|entry| {
                let target = entry.target_name.as_deref().and_then(volume_identity)?;
                (target == identity || usb_storage_identity_matches(&target, &identity))
                    .then_some((entry, target))
            })
            .collect();
        if !direct.is_empty() {
            for (entry, target) in direct {
                let mut volume = UsbVolumeEntry {
                    volume_identity: target,
                    source_key_paths: vec![source_key.to_string()],
                    ..Default::default()
                };
                volume.drive_letters.extend(entry.drive_letter.clone());
                volume
                    .source_key_paths
                    .push(format!(r"MountedDevices\{}", entry.device_name));
                volumes.push(volume);
            }
        }
        merge_volumes(volumes)
    }

    fn collect_disks(&mut self, hive: &RegistryHiveReader<'_>, prefix: &[&str], bus: &str) {
        let root = [prefix, &[bus]].concat();
        for class in children(hive, &root) {
            let class_path = [root.as_slice(), &[&class]].concat();
            for instance in children(hive, &class_path) {
                let path = [class_path.as_slice(), &[&instance]].concat();
                let identity = if bus == "SCSI" {
                    let property = [
                        path.as_slice(),
                        &[
                            "Properties",
                            "{83da6326-97a6-4088-9453-a1923f573b29}",
                            "000A",
                        ],
                    ]
                    .concat();
                    let Some(parent) = text(hive, &property, "") else {
                        continue;
                    };
                    let parent = parent.to_ascii_lowercase();
                    if !parent.starts_with(r"usb\") {
                        continue;
                    }
                    parent
                } else {
                    format!(r"USBSTOR\{class}\{instance}").to_ascii_lowercase()
                };
                let mut volumes = self.disk_volumes(hive, prefix, &path);
                if bus == "SCSI" {
                    for volume in &mut volumes {
                        volume.source_key_paths.push(format!(
                            r"{}\Properties\{{83da6326-97a6-4088-9453-a1923f573b29}}\000A",
                            path.join("\\")
                        ));
                    }
                }
                self.disks.entry(identity).or_default().extend(volumes);
            }
        }
    }

    fn disk_volumes(
        &self,
        hive: &RegistryHiveReader<'_>,
        prefix: &[&str],
        path: &[&str],
    ) -> Vec<UsbVolumeEntry> {
        let partmgr = [path, &["Device Parameters", "Partmgr"]].concat();
        let Some(disk_id) =
            text(hive, &partmgr, "DiskId").and_then(|v| uuid::Uuid::parse_str(&v).ok())
        else {
            return Vec::new();
        };
        if disk_id.is_nil() {
            return Vec::new();
        }
        let Ok(Some(RegistryValue::Binary(cache))) =
            hive.lookup_value(&partmgr, "PartitionTableCache")
        else {
            return Vec::new();
        };
        // GPT has a real disk GUID in the layout header; it must agree with DiskId.
        if cache.get(..4) == Some(&1u32.to_le_bytes())
            && cache
                .get(8..24)
                .and_then(|v| <[u8; 16]>::try_from(v).ok())
                .map(uuid::Uuid::from_bytes_le)
                != Some(disk_id)
        {
            return Vec::new();
        }
        partitions(&cache)
            .into_iter()
            .map(|partition| {
                let identity = format!("{{{disk_id}}}#{:016x}", partition.offset);
                let mut volume = UsbVolumeEntry {
                    volume_identity: identity.clone(),
                    drive_letters: Vec::new(),
                    source_key_paths: vec![
                        path.join("\\"),
                        format!(r"{}\DiskId", partmgr.join("\\")),
                        format!(r"{}\PartitionTableCache", partmgr.join("\\")),
                    ],
                };
                let storage = [prefix, &["STORAGE", "Volume", identity.as_str()]].concat();
                if hive.navigate_to(&storage).ok().flatten().is_some() {
                    volume.source_key_paths.push(storage.join("\\"));
                }
                let mounted_id = hex::encode(partition.mounted_id);
                for entry in self
                    .mounted
                    .iter()
                    .filter(|entry| entry.disk_signature_hex.as_deref() == Some(&mounted_id))
                {
                    volume.drive_letters.extend(entry.drive_letter.clone());
                    volume
                        .source_key_paths
                        .push(format!(r"MountedDevices\{}", entry.device_name));
                }
                volume
            })
            .collect()
    }
}

fn normalize_usb_identity(identity: &str) -> String {
    let normalized = identity
        .to_ascii_lowercase()
        .replace("\\disk&", "\\")
        .to_string();
    let normalized = normalized
        .split_once("\\")
        .map(|(prefix, serial)| {
            let prefix = prefix
                .split_once("&rev_")
                .map(|(head, _)| head)
                .unwrap_or(prefix);
            format!("{prefix}\\{serial}")
        })
        .unwrap_or(normalized);
    normalized
        .trim_end_matches("&0")
        .trim_end_matches("&1")
        .to_string()
}

fn usb_storage_identity_matches(target: &str, identity: &str) -> bool {
    let Some(target) = target.strip_prefix("usbstor\\") else {
        return false;
    };
    let Some(identity) = identity.strip_prefix("usbstor\\") else {
        return false;
    };
    let Some((target_class, target_serial)) = target.rsplit_once('\\') else {
        return false;
    };
    let Some((identity_class, identity_serial)) = identity.rsplit_once('\\') else {
        return false;
    };
    let normalize_class = |class: &str| {
        let class = class.trim_start_matches("disk&").to_ascii_lowercase();
        class
            .split_once("&rev_")
            .map(|(head, _)| head)
            .unwrap_or(&class)
            .to_string()
    };
    let target_class = normalize_class(target_class);
    let identity_class = normalize_class(identity_class);
    target_class == identity_class
        && normalize_serial(target_serial) == normalize_serial(identity_serial)
}

fn normalize_serial(serial: &str) -> &str {
    serial.trim_end_matches("&0").trim_end_matches("&1")
}

fn children(hive: &RegistryHiveReader<'_>, path: &[&str]) -> Vec<String> {
    hive.navigate_to(path)
        .ok()
        .flatten()
        .and_then(|key| hive.read_subkey_names_from_nk(&key).ok())
        .unwrap_or_default()
}

fn text(hive: &RegistryHiveReader<'_>, path: &[&str], name: &str) -> Option<String> {
    match hive.lookup_value(path, name).ok().flatten()? {
        RegistryValue::String(value) => Some(value.trim_matches('\0').trim().to_string()),
        RegistryValue::Binary(value) => utf16_text(&value),
        _ => None,
    }
}

fn merge_volumes(volumes: Vec<UsbVolumeEntry>) -> Vec<UsbVolumeEntry> {
    let mut merged: BTreeMap<String, UsbVolumeEntry> = BTreeMap::new();
    for volume in volumes {
        let entry = merged
            .entry(volume.volume_identity.clone())
            .or_insert_with(|| UsbVolumeEntry {
                volume_identity: volume.volume_identity.clone(),
                ..Default::default()
            });
        entry.drive_letters.extend(volume.drive_letters);
        entry.source_key_paths.extend(volume.source_key_paths);
    }
    for volume in merged.values_mut() {
        volume.drive_letters.sort();
        volume.drive_letters.dedup();
        volume.source_key_paths.sort();
        volume.source_key_paths.dedup();
    }
    merged.into_values().collect()
}
