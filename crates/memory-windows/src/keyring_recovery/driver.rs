use std::collections::HashSet;

use crate::{MemoryWindowsError, Result, X64AddressSpace};

use super::{
    object_directory::read_pointer_field,
    profile::{DriverLayout, KeyringLayout},
};

const MAXIMUM_CLIENT_EXTENSIONS: usize = 64;
const MAXIMUM_DRIVER_NAME_BYTES: usize = 512;

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
    let keyring_pointer = read_pointer_field(address_space, body, keyring.client_keyring_offset)?;
    if keyring_pointer == 0 {
        return Err(MemoryWindowsError::BitLockerKeyringNotFound);
    }
    Ok(keyring_pointer)
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
