use super::reader::RegistryHiveReader;
use super::txlog_util::apply_single_txlog_override;
use super::*;
use crate::registry::RegistryError;

mod drivers;
mod network;
mod services;
mod shutdown;
mod usb;

#[cfg(test)]
#[path = "../../../../tests/unit/registry/lookup/system.rs"]
mod tests;

pub use drivers::extract_shimcache_from_system_hive;
pub use network::extract_network_adapters_from_system_hive;
pub use services::extract_services_from_system_hive;
pub use shutdown::extract_shutdown_time_from_system_hive;
pub use usb::extract_usb_devices_from_system_hive;

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
