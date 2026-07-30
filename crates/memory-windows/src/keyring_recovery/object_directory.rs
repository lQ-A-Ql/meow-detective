use std::collections::HashSet;

use crate::{is_canonical_address, MemoryWindowsError, Result, X64AddressSpace};

use super::profile::ObjectManagerLayout;

const MAXIMUM_DIRECTORY_ENTRIES: usize = 1_024;
const MAXIMUM_OBJECT_NAME_BYTES: usize = 512;

pub(crate) fn root_directory(
    address_space: &mut X64AddressSpace,
    kernel_base: u64,
    layout: ObjectManagerLayout,
) -> Result<u64> {
    let symbol = kernel_base
        .checked_add(u64::from(layout.root_directory_object_rva))
        .ok_or(MemoryWindowsError::MalformedObjectDirectory)?;
    let directory = address_space.read_virtual_u64(symbol)?;
    require_pointer(directory)?;
    Ok(directory)
}

pub(crate) fn find_named_object(
    address_space: &mut X64AddressSpace,
    kernel_base: u64,
    directory: u64,
    expected_name: &'static str,
    layout: ObjectManagerLayout,
) -> Result<u64> {
    require_pointer(directory)?;
    let mut state = DirectoryWalkState::default();
    for bucket in 0..layout.directory_bucket_count {
        let bucket_address = directory
            .checked_add(u64::from(bucket) * 8)
            .ok_or(MemoryWindowsError::MalformedObjectDirectory)?;
        let entry = address_space.read_virtual_u64(bucket_address)?;
        state.walk_chain(address_space, kernel_base, entry, expected_name, layout)?;
    }
    match state.matching_object {
        Some(object) => Ok(object),
        None => Err(MemoryWindowsError::NamedKernelObjectNotFound {
            name: expected_name,
        }),
    }
}

#[derive(Default)]
struct DirectoryWalkState {
    visited: HashSet<u64>,
    matching_object: Option<u64>,
}

impl DirectoryWalkState {
    fn walk_chain(
        &mut self,
        address_space: &mut X64AddressSpace,
        kernel_base: u64,
        mut entry: u64,
        expected_name: &'static str,
        layout: ObjectManagerLayout,
    ) -> Result<()> {
        while entry != 0 {
            require_pointer(entry)?;
            if !self.visited.insert(entry) || self.visited.len() > MAXIMUM_DIRECTORY_ENTRIES {
                return Err(MemoryWindowsError::MalformedObjectDirectory);
            }
            let object =
                read_pointer_field(address_space, entry, layout.directory_entry_object_offset)?;
            let name = read_object_name(address_space, kernel_base, object, layout)?;
            if name.eq_ignore_ascii_case(expected_name)
                && self.matching_object.replace(object).is_some()
            {
                return Err(MemoryWindowsError::AmbiguousNamedKernelObject {
                    name: expected_name,
                });
            }
            entry = read_pointer_field(address_space, entry, layout.directory_entry_chain_offset)?;
        }
        Ok(())
    }
}

fn read_object_name(
    address_space: &mut X64AddressSpace,
    kernel_base: u64,
    object: u64,
    layout: ObjectManagerLayout,
) -> Result<String> {
    require_pointer(object)?;
    let header = object
        .checked_sub(u64::from(layout.object_header_body_offset))
        .ok_or(MemoryWindowsError::MalformedObjectDirectory)?;
    let info_mask = read_u8(
        address_space,
        header + u64::from(layout.object_header_info_mask_offset),
    )?;
    let index = info_mask & (layout.name_info_bit | (layout.name_info_bit - 1));
    let offset_table = kernel_base
        .checked_add(u64::from(layout.info_mask_to_offset_rva))
        .and_then(|address| address.checked_add(u64::from(index)))
        .ok_or(MemoryWindowsError::MalformedObjectDirectory)?;
    let name_info_offset = u64::from(read_u8(address_space, offset_table)?);
    if name_info_offset == 0 {
        return Err(MemoryWindowsError::MalformedObjectDirectory);
    }
    let unicode = header
        .checked_sub(name_info_offset)
        .and_then(|address| address.checked_add(u64::from(layout.name_info_name_offset)))
        .ok_or(MemoryWindowsError::MalformedObjectDirectory)?;
    read_unicode_string(address_space, unicode, layout)
}

fn read_unicode_string(
    address_space: &mut X64AddressSpace,
    address: u64,
    layout: ObjectManagerLayout,
) -> Result<String> {
    let length = usize::from(read_u16(
        address_space,
        address + u64::from(layout.unicode_length_offset),
    )?);
    let maximum_length = usize::from(read_u16(
        address_space,
        address + u64::from(layout.unicode_maximum_length_offset),
    )?);
    if length == 0
        || !length.is_multiple_of(2)
        || length > maximum_length
        || maximum_length > MAXIMUM_OBJECT_NAME_BYTES
    {
        return Err(MemoryWindowsError::MalformedObjectDirectory);
    }
    let buffer = read_pointer_field(address_space, address, layout.unicode_buffer_offset)?;
    let mut bytes = vec![0u8; length];
    address_space.read_virtual_exact(buffer, &mut bytes)?;
    let words = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&words).map_err(|_| MemoryWindowsError::MalformedObjectDirectory)
}

pub(crate) fn read_pointer_field(
    address_space: &mut X64AddressSpace,
    base: u64,
    offset: u16,
) -> Result<u64> {
    let address = base
        .checked_add(u64::from(offset))
        .ok_or(MemoryWindowsError::MalformedObjectDirectory)?;
    let pointer = address_space.read_virtual_u64(address)?;
    if pointer != 0 {
        require_pointer(pointer)?;
    }
    Ok(pointer)
}

fn read_u8(address_space: &mut X64AddressSpace, address: u64) -> Result<u8> {
    let mut bytes = [0u8; 1];
    address_space.read_virtual_exact(address, &mut bytes)?;
    Ok(bytes[0])
}

fn read_u16(address_space: &mut X64AddressSpace, address: u64) -> Result<u16> {
    let mut bytes = [0u8; 2];
    address_space.read_virtual_exact(address, &mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn require_pointer(pointer: u64) -> Result<()> {
    if pointer != 0 && is_canonical_address(pointer) && pointer >> 63 == 1 {
        Ok(())
    } else {
        Err(MemoryWindowsError::MalformedObjectDirectory)
    }
}
