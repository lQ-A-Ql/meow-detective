use super::reader::RegistryHiveReader;
use super::*;
use crate::registry::RegistryError;

mod drivers;
mod mounted;
mod network;
mod partition_cache;
mod services;
mod shutdown;
mod usb;
mod usb_mapping;

#[cfg(test)]
#[path = "../../../../tests/unit/registry/lookup/system.rs"]
mod tests;

pub use drivers::extract_shimcache_from_system_hive;
pub use mounted::extract_mounted_devices_from_system_hive;
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

/// Extract fields from the page-recovered SYSTEM hive.
pub fn extract_system_hive_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<SystemHiveInfo, RegistryError> {
    let snapshot = crate::registry::RegistrySnapshot::prepare(bytes, Some(txlog_data), None)?;
    let mut info = extract_system_hive_fields(snapshot.bytes(), hive_path)?;
    info.txlog_applied = snapshot.logs_applied();
    Ok(info)
}
