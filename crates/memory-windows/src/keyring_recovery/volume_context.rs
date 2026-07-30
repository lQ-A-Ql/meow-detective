use std::collections::HashSet;

use volume_bitlocker::RecoveredVmk;
use zeroize::Zeroizing;

use crate::{is_canonical_address, MemoryWindowsError, Result, X64AddressSpace};

use super::profile::{DeviceObjectLayout, DriverLayout, FveVolumeContextLayout};

const VMK_DATUM_SIZE: usize = 44;
const VMK_LENGTH: usize = 32;
/// Bounded device-extension window scanned for VMK datum pointers when the
/// version-specific offset is unknown (0). The reviewed 26100 layout keeps
/// the pointer at 0x3D0, well inside this window.
const DATUM_SCAN_WINDOW: u64 = 0x800;

pub(crate) struct DeviceContextVmks {
    pub vmks: Vec<RecoveredVmk>,
    pub devices_examined: usize,
    pub datum_pointers_examined: usize,
}

pub(crate) fn read_device_context_vmks(
    address_space: &mut X64AddressSpace,
    driver_object: u64,
    driver: DriverLayout,
    devices: DeviceObjectLayout,
    context: FveVolumeContextLayout,
) -> Result<DeviceContextVmks> {
    let mut device = read_pointer(address_space, driver_object, driver.device_object_offset)?;
    let mut seen = HashSet::new();
    let mut vmks = Vec::new();
    let mut datum_pointers_examined = 0usize;

    while device != 0 {
        if !seen.insert(device) || seen.len() > usize::from(devices.maximum_devices) {
            return Err(MemoryWindowsError::MalformedFvevolDeviceChain);
        }
        validate_device_object(address_space, device, driver_object, devices)?;
        let extension =
            read_required_pointer(address_space, device, devices.device_extension_offset)?;
        // Collect every candidate, not just the profile-offset one: a device
        // extension can hold several VMK datum generations (e.g. the active
        // VMK alongside the older VMK that wrapped the recovery protector's
        // reverse datum). The volume-bound CCM oracles downstream arbitrate.
        let mut candidates = Vec::new();
        if context.vmk_datum_pointer_offset != 0 {
            let datum = read_pointer(address_space, extension, context.vmk_datum_pointer_offset)?;
            if datum != 0 && datum_parses(address_space, datum, context) {
                candidates.push(datum);
            }
        }
        scan_for_vmk_datums(address_space, extension, context, &mut candidates);
        for datum in candidates {
            datum_pointers_examined += 1;
            let mut bytes = Zeroizing::new([0u8; VMK_DATUM_SIZE]);
            address_space.read_virtual_exact(datum, &mut bytes[..])?;
            if let Some(vmk) = parse_profiled_vmk_datum(&bytes[..], context) {
                vmks.push(RecoveredVmk::new(vmk));
            }
        }
        device = read_pointer(address_space, device, devices.next_device_offset)?;
    }

    if vmks.is_empty() {
        return Err(MemoryWindowsError::BitLockerDeviceVmkNotFound);
    }
    Ok(DeviceContextVmks {
        vmks,
        devices_examined: seen.len(),
        datum_pointers_examined,
    })
}

fn datum_parses(
    address_space: &mut X64AddressSpace,
    datum: u64,
    context: FveVolumeContextLayout,
) -> bool {
    let mut bytes = Zeroizing::new([0u8; VMK_DATUM_SIZE]);
    address_space
        .read_virtual_exact(datum, &mut bytes[..])
        .is_ok_and(|()| parse_profiled_vmk_datum(&bytes[..], context).is_some())
}

/// Offset-blind discovery: scans the validated device extension for pointers
/// to buffers that parse exactly as a VMK key datum, appending every unique
/// match. The volume-bound CCM oracles downstream remain the final arbiter.
fn scan_for_vmk_datums(
    address_space: &mut X64AddressSpace,
    extension: u64,
    context: FveVolumeContextLayout,
    candidates: &mut Vec<u64>,
) {
    for step in (0..DATUM_SCAN_WINDOW).step_by(8) {
        let pointer = address_space
            .read_virtual_u64(extension + step)
            .unwrap_or(0);
        if pointer == 0 || !is_canonical_address(pointer) || pointer >> 63 != 1 {
            continue;
        }
        if candidates.contains(&pointer) {
            continue;
        }
        if datum_parses(address_space, pointer, context) {
            candidates.push(pointer);
        }
    }
}

pub(crate) fn parse_profiled_vmk_datum(
    bytes: &[u8],
    layout: FveVolumeContextLayout,
) -> Option<[u8; VMK_LENGTH]> {
    let key_start = usize::from(layout.vmk_offset);
    let key_end = key_start.checked_add(VMK_LENGTH)?;
    let matches = bytes.len() == usize::from(layout.vmk_datum_size)
        && key_end == bytes.len()
        && read_slice_u16(bytes, 0)? == layout.vmk_datum_size
        && read_slice_u16(bytes, 2)? == layout.vmk_datum_entry_type
        && read_slice_u16(bytes, 4)? == layout.vmk_datum_value_type
        && read_slice_u16(bytes, 6)? == layout.vmk_datum_version
        && read_slice_u16(bytes, 8)? == layout.vmk_datum_algorithm;
    matches.then(|| {
        let mut vmk = [0u8; VMK_LENGTH];
        vmk.copy_from_slice(&bytes[key_start..key_end]);
        vmk
    })
}

fn validate_device_object(
    address_space: &mut X64AddressSpace,
    device: u64,
    driver_object: u64,
    layout: DeviceObjectLayout,
) -> Result<()> {
    let object_type = read_u16(address_space, device, layout.object_type_offset)?;
    let object_size = read_u16(address_space, device, layout.object_size_offset)?;
    let owner = read_required_pointer(address_space, device, layout.driver_object_offset)?;
    if object_type != layout.expected_object_type
        || object_size < layout.minimum_object_size
        || owner != driver_object
    {
        return Err(MemoryWindowsError::MalformedFvevolDeviceChain);
    }
    Ok(())
}

fn read_required_pointer(
    address_space: &mut X64AddressSpace,
    base: u64,
    offset: u16,
) -> Result<u64> {
    let pointer = read_pointer(address_space, base, offset)?;
    if pointer == 0 {
        Err(MemoryWindowsError::MalformedFvevolDeviceChain)
    } else {
        Ok(pointer)
    }
}

fn read_pointer(address_space: &mut X64AddressSpace, base: u64, offset: u16) -> Result<u64> {
    let address = base
        .checked_add(u64::from(offset))
        .ok_or(MemoryWindowsError::MalformedFvevolDeviceChain)?;
    let pointer = address_space.read_virtual_u64(address)?;
    if pointer != 0 && (!is_canonical_address(pointer) || pointer >> 63 != 1) {
        return Err(MemoryWindowsError::MalformedFvevolDeviceChain);
    }
    Ok(pointer)
}

fn read_u16(address_space: &mut X64AddressSpace, base: u64, offset: u16) -> Result<u16> {
    let address = base
        .checked_add(u64::from(offset))
        .ok_or(MemoryWindowsError::MalformedFvevolDeviceChain)?;
    let mut bytes = [0u8; 2];
    address_space.read_virtual_exact(address, &mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_slice_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
}
