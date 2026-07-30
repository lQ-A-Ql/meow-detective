use std::collections::HashSet;

use crate::{MemoryWindowsError, Result, X64AddressSpace};

use super::{
    limits::{LoadedModuleEntryLayout, TargetedKernelSearchLimits},
    utils::{
        is_kernel_pointer, read_field_u16, read_field_u32, read_field_u64, read_module_bytes,
        signed_offset,
    },
};

/// One module from the profile-validated Windows loaded-module list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KernelModule {
    pub name: String,
    pub base: u64,
    pub size: u32,
}

/// Enumerates a module list only when its layout is supplied by a trusted,
/// version-specific profile. This function deliberately has no Windows offset
/// defaults, so an unknown build fails closed at the caller boundary.
pub(crate) fn enumerate_loaded_modules(
    address_space: &mut X64AddressSpace,
    list_head: u64,
    layout: LoadedModuleEntryLayout,
    limits: TargetedKernelSearchLimits,
) -> Result<Vec<KernelModule>> {
    if !is_kernel_pointer(list_head) {
        return Err(MemoryWindowsError::MalformedModuleList);
    }
    let mut scanned_bytes = 0u64;
    let mut list_head_bytes = [0u8; 8];
    read_module_bytes(
        address_space,
        list_head,
        &mut list_head_bytes,
        &mut scanned_bytes,
        limits,
    )?;
    let mut current = u64::from_le_bytes(list_head_bytes);
    let mut seen = HashSet::new();
    let mut modules = Vec::new();
    while current != list_head {
        if modules.len() >= limits.maximum_modules
            || !is_kernel_pointer(current)
            || !seen.insert(current)
        {
            return Err(MemoryWindowsError::MalformedModuleList);
        }
        let entry = signed_offset(current, layout.link_to_entry)
            .ok_or(MemoryWindowsError::MalformedModuleList)?;
        let next = read_field_u64(
            address_space,
            current,
            layout.flink_offset,
            &mut scanned_bytes,
            limits,
        )?;
        let base = read_field_u64(
            address_space,
            entry,
            layout.dll_base_offset,
            &mut scanned_bytes,
            limits,
        )?;
        let size = read_field_u32(
            address_space,
            entry,
            layout.size_of_image_offset,
            &mut scanned_bytes,
            limits,
        )?;
        let name_length = usize::from(read_field_u16(
            address_space,
            entry,
            layout.name_length_offset,
            &mut scanned_bytes,
            limits,
        )?);
        let name_buffer = read_field_u64(
            address_space,
            entry,
            layout.name_buffer_offset,
            &mut scanned_bytes,
            limits,
        )?;
        if !is_kernel_pointer(base)
            || size == 0
            || base.checked_add(u64::from(size)).is_none()
            || name_length == 0
            || name_length > limits.maximum_module_name_bytes
            || !name_length.is_multiple_of(2)
            || !is_kernel_pointer(name_buffer)
        {
            return Err(MemoryWindowsError::MalformedModuleList);
        }
        let name = read_utf16_name(
            address_space,
            name_buffer,
            name_length,
            &mut scanned_bytes,
            limits,
        )?;
        modules.push(KernelModule { name, base, size });
        current = next;
    }
    Ok(modules)
}

fn read_utf16_name(
    address_space: &mut X64AddressSpace,
    address: u64,
    length: usize,
    scanned_bytes: &mut u64,
    limits: TargetedKernelSearchLimits,
) -> Result<String> {
    let mut bytes = vec![0u8; length];
    read_module_bytes(address_space, address, &mut bytes, scanned_bytes, limits)?;
    let words = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
    let name = String::from_utf16(&words.collect::<Vec<_>>())
        .map_err(|_| MemoryWindowsError::MalformedModuleList)?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name
            .chars()
            .any(|character| character.is_control() || matches!(character, '\\' | '/' | ':'))
    {
        return Err(MemoryWindowsError::MalformedModuleList);
    }
    Ok(name)
}
