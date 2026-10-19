//! Hardware records from an evidence SYSTEM hive, never from the examiner host.
mod devices;
mod smbios;
mod smbios_records;
mod types;

use super::{reader::RegistryHiveReader, types::RegistryValue};
use crate::registry::RegistryError;
pub use types::{RegistryHardwareInfo, RegistryHardwareRecord};

pub(crate) fn extract_system_hardware(
    bytes: &[u8],
    read_smbios: impl FnOnce(&str) -> Result<Option<Vec<u8>>, RegistryError>,
) -> Result<RegistryHardwareInfo, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = RegistryHardwareInfo::default();
    let control_sets = control_sets(&hive, &mut info.warnings)?;
    let mut selected = None;
    for control_set in control_sets {
        if hive.navigate_to(&[&control_set])?.is_some() {
            selected = Some(control_set);
            break;
        }
    }
    let Some(control_set) = selected else {
        return Ok(info);
    };
    let key = [&control_set, "Services", "mssmbios", "Data"].join("\\");
    let smbios = read_smbios(&key);
    match smbios {
        Ok(Some(bytes)) => match smbios::parse(&bytes, &key) {
            Ok(records) => info.devices.extend(records),
            Err(error) => info.warnings.push(format!("SMBiosData: {error}")),
        },
        Ok(None) => {}
        Err(error) => info.warnings.push(format!("SMBiosData: {error}")),
    }
    devices::extract(&hive, &control_set, &mut info);
    Ok(info)
}

fn control_sets(
    hive: &RegistryHiveReader<'_>,
    warnings: &mut Vec<String>,
) -> Result<Vec<String>, RegistryError> {
    let selected = match hive.navigate_to(&["Select"])? {
        Some(key) => hive.read_bounded_value(&key, "Current", 4),
        None => Ok(None),
    };
    let mut sets = Vec::new();
    match selected {
        Ok(Some(RegistryValue::Dword(value))) if (1..=999).contains(&value) => {
            sets.push(format!("ControlSet{value:03}"))
        }
        _ => warnings.push("Select\\Current unavailable; using a fallback control set".into()),
    }
    for fallback in ["ControlSet001", "ControlSet002", "CurrentControlSet"] {
        if !sets.iter().any(|item| item == fallback) {
            sets.push(fallback.into());
        }
    }
    Ok(sets)
}

#[cfg(test)]
#[path = "../../../../tests/unit/registry/lookup/hardware.rs"]
mod tests;
