use std::collections::HashSet;

use crate::{is_canonical_address, MemoryWindowsError, Result, X64AddressSpace};

use super::{
    keyring::is_valid_keyring_header,
    object_directory::read_pointer_field,
    profile::{DriverLayout, KeyringLayout},
};

const MAXIMUM_CLIENT_EXTENSIONS: usize = 64;
const MAXIMUM_DRIVER_NAME_BYTES: usize = 512;
const KEYRING_HEADER_PROBE_LEN: usize = 0x20;
/// Bounded window scanned for the keyring pointer when the version-specific
/// offset is unknown (0). The reviewed 26100 layout keeps it at 0x278, well
/// inside this window.
const KEYRING_SCAN_WINDOW: u64 = 0x800;

const DRIVER_OBJECT_PROBE_LEN: usize = 0x50;
const FVEVOL_DRIVER_NAME: &[u16] = &[
    0x005C, 0x0044, 0x0072, 0x0069, 0x0076, 0x0065, 0x0072, 0x005C, 0x0046, 0x0056, 0x0045, 0x0056,
    0x006F, 0x006C,
];
const DRIVER_CARVE_CHUNK: u64 = 16 * 1024 * 1024;

/// Version-free FVEVol driver discovery: carves the physical image for the
/// driver-start pointer, validates the surrounding DRIVER_OBJECT struct, and
/// lifts the candidate back into the virtual space through a reverse
/// page-table lookup. No object-manager globals are required.
pub(crate) fn scan_fvevol_driver_object(
    address_space: &mut X64AddressSpace,
    fvevol_base: u64,
    fvevol_size: u32,
    layout: DriverLayout,
) -> Result<u64> {
    let image_len = address_space.image_len();
    let mut offset = 0u64;
    let mut tail = [0u8; 8];
    let mut tail_len = 0usize;
    while offset < image_len {
        let take = DRIVER_CARVE_CHUNK.min(image_len - offset) as usize;
        let mut buffer = Vec::with_capacity(take + 8);
        buffer.extend_from_slice(&tail[..tail_len]);
        let chunk_start = offset - tail_len as u64;
        buffer.resize(tail_len + take, 0);
        address_space.read_physical_exact(offset, &mut buffer[tail_len..])?;
        for (index, window) in buffer.windows(8).enumerate() {
            if u64::from_le_bytes(window.try_into().expect("8-byte window")) != fvevol_base {
                continue;
            }
            let hit = chunk_start + index as u64;
            let Some(candidate) = hit.checked_sub(u64::from(layout.driver_start_offset)) else {
                continue;
            };
            if !driver_object_matches_physical(
                address_space,
                candidate,
                fvevol_base,
                fvevol_size,
                layout,
            ) {
                continue;
            }
            let Some(va) = address_space.find_virtual_for_physical(candidate)? else {
                continue;
            };
            if driver_object_matches_virtual(address_space, va, fvevol_base, fvevol_size, layout)? {
                return Ok(va);
            }
        }
        tail_len = 8.min(buffer.len());
        tail[..tail_len].copy_from_slice(&buffer[buffer.len() - tail_len..]);
        offset += take as u64;
    }
    Err(MemoryWindowsError::TargetedFvevolNotFound)
}

fn driver_object_matches_physical(
    address_space: &mut X64AddressSpace,
    candidate: u64,
    fvevol_base: u64,
    fvevol_size: u32,
    layout: DriverLayout,
) -> bool {
    let mut probe = [0u8; DRIVER_OBJECT_PROBE_LEN];
    if address_space
        .read_physical_exact(candidate, &mut probe)
        .is_err()
    {
        return false;
    }
    let name_length = u16::from_le_bytes([
        probe[layout.driver_name_offset as usize],
        probe[layout.driver_name_offset as usize + 1],
    ]);
    let buffer_ptr = u64::from_le_bytes(
        probe[layout.driver_name_offset as usize + 8..layout.driver_name_offset as usize + 16]
            .try_into()
            .expect("pointer field"),
    );
    let device = u64::from_le_bytes(
        probe[layout.device_object_offset as usize..layout.device_object_offset as usize + 8]
            .try_into()
            .expect("pointer field"),
    );
    let extension = u64::from_le_bytes(
        probe[layout.driver_extension_offset as usize..layout.driver_extension_offset as usize + 8]
            .try_into()
            .expect("pointer field"),
    );
    name_length as usize == FVEVOL_DRIVER_NAME.len() * 2
        && is_canonical_address(buffer_ptr)
        && buffer_ptr >> 63 == 1
        && is_canonical_address(device)
        && device != 0
        && is_canonical_address(extension)
        && extension != 0
        && fvevol_size > 0
        && u64::from_le_bytes(
            probe[layout.driver_start_offset as usize..layout.driver_start_offset as usize + 8]
                .try_into()
                .expect("driver start"),
        ) == fvevol_base
}

fn driver_object_matches_virtual(
    address_space: &mut X64AddressSpace,
    va: u64,
    fvevol_base: u64,
    fvevol_size: u32,
    layout: DriverLayout,
) -> Result<bool> {
    let start = address_space
        .read_virtual_u64(va + u64::from(layout.driver_start_offset))
        .unwrap_or(0);
    let size = address_space
        .read_virtual_u64(va + u64::from(layout.driver_size_offset))
        .unwrap_or(0) as u32;
    if start != fvevol_base || size != fvevol_size {
        return Ok(false);
    }
    let name = read_driver_name(address_space, va, layout)?;
    Ok(name.eq_ignore_ascii_case("\\Driver\\FVEVol"))
}

pub(crate) fn find_keyring(
    address_space: &mut X64AddressSpace,
    driver_object: u64,
    fvevol_base: u64,
    fvevol_size: u32,
    driver: DriverLayout,
    keyring: KeyringLayout,
) -> Result<u64> {
    validate_driver(
        address_space,
        driver_object,
        fvevol_base,
        fvevol_size,
        driver,
    )?;
    let driver_extension =
        read_pointer_field(address_space, driver_object, driver.driver_extension_offset)?;
    if driver_extension == 0
        || read_pointer_field(
            address_space,
            driver_extension,
            driver.extension_driver_object_offset,
        )? != driver_object
    {
        return Err(MemoryWindowsError::MalformedFvevolDriverObject);
    }
    let client = find_client_extension(address_space, driver_object, driver_extension, driver)?;
    let body = client
        .checked_add(u64::from(driver.client_body_offset))
        .ok_or(MemoryWindowsError::MalformedFvevolDriverObject)?;
    if keyring.client_keyring_offset != 0 {
        let keyring_pointer =
            read_pointer_field(address_space, body, keyring.client_keyring_offset)?;
        if keyring_pointer != 0 && keyring_header_matches(address_space, keyring_pointer, keyring) {
            return Ok(keyring_pointer);
        }
    }
    scan_for_keyring(address_space, body, keyring)
}

/// Validates the header of one candidate keyring address.
fn keyring_header_matches(
    address_space: &mut X64AddressSpace,
    keyring_address: u64,
    layout: KeyringLayout,
) -> bool {
    let mut header = [0u8; KEYRING_HEADER_PROBE_LEN];
    address_space
        .read_virtual_exact(keyring_address, &mut header)
        .is_ok_and(|()| is_valid_keyring_header(&header, layout))
}

/// Offset-blind discovery: scans the validated client-extension body for a
/// pointer to a buffer that fully parses as an "-FVE-FS-" keyring header.
/// Exactly one candidate must exist; ambiguity fails closed.
fn scan_for_keyring(
    address_space: &mut X64AddressSpace,
    body: u64,
    layout: KeyringLayout,
) -> Result<u64> {
    let mut matching = None;
    for step in (0..KEYRING_SCAN_WINDOW).step_by(8) {
        let pointer = address_space.read_virtual_u64(body + step).unwrap_or(0);
        if pointer == 0 || !is_canonical_address(pointer) || pointer >> 63 != 1 {
            continue;
        }
        if !keyring_header_matches(address_space, pointer, layout) {
            continue;
        }
        if matching.replace(pointer).is_some() {
            return Err(MemoryWindowsError::MalformedFvevolDriverObject);
        }
    }
    matching.ok_or(MemoryWindowsError::BitLockerKeyringNotFound)
}

fn validate_driver(
    address_space: &mut X64AddressSpace,
    driver_object: u64,
    expected_start: u64,
    expected_size: u32,
    layout: DriverLayout,
) -> Result<()> {
    let start = read_pointer_field(address_space, driver_object, layout.driver_start_offset)?;
    let size = read_u32(
        address_space,
        driver_object + u64::from(layout.driver_size_offset),
    )?;
    let device = read_pointer_field(address_space, driver_object, layout.device_object_offset)?;
    let name = read_driver_name(address_space, driver_object, layout)?;
    if start != expected_start
        || size != expected_size
        || device == 0
        || !name.eq_ignore_ascii_case("\\Driver\\FVEVol")
    {
        return Err(MemoryWindowsError::MalformedFvevolDriverObject);
    }
    Ok(())
}

fn find_client_extension(
    address_space: &mut X64AddressSpace,
    driver_object: u64,
    driver_extension: u64,
    layout: DriverLayout,
) -> Result<u64> {
    let mut current = read_pointer_field(
        address_space,
        driver_extension,
        layout.extension_client_list_offset,
    )?;
    let mut seen = HashSet::new();
    let mut matching = None;
    while current != 0 {
        if !seen.insert(current) || seen.len() > MAXIMUM_CLIENT_EXTENSIONS {
            return Err(MemoryWindowsError::MalformedFvevolDriverObject);
        }
        let identifier =
            read_pointer_field(address_space, current, layout.client_identifier_offset)?;
        if identifier == driver_object && matching.replace(current).is_some() {
            return Err(MemoryWindowsError::AmbiguousFvevolClientExtension);
        }
        current = read_pointer_field(address_space, current, layout.client_next_offset)?;
    }
    matching.ok_or(MemoryWindowsError::FvevolClientExtensionNotFound)
}

fn read_driver_name(
    address_space: &mut X64AddressSpace,
    driver_object: u64,
    layout: DriverLayout,
) -> Result<String> {
    let unicode = driver_object + u64::from(layout.driver_name_offset);
    let length = usize::from(read_u16(address_space, unicode)?);
    let maximum_length = usize::from(read_u16(address_space, unicode + 2)?);
    let buffer = read_pointer_field(address_space, unicode, 8)?;
    if length == 0
        || !length.is_multiple_of(2)
        || length > maximum_length
        || maximum_length > MAXIMUM_DRIVER_NAME_BYTES
        || buffer == 0
    {
        return Err(MemoryWindowsError::MalformedFvevolDriverObject);
    }
    let mut bytes = vec![0u8; length];
    address_space.read_virtual_exact(buffer, &mut bytes)?;
    let words = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&words).map_err(|_| MemoryWindowsError::MalformedFvevolDriverObject)
}

fn read_u16(address_space: &mut X64AddressSpace, address: u64) -> Result<u16> {
    let mut bytes = [0u8; 2];
    address_space.read_virtual_exact(address, &mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(address_space: &mut X64AddressSpace, address: u64) -> Result<u32> {
    let mut bytes = [0u8; 4];
    address_space.read_virtual_exact(address, &mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
