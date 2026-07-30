use crate::{is_canonical_address, MemoryWindowsError, Result, X64AddressSpace};

use super::limits::TargetedKernelSearchLimits;

pub(crate) fn read_counted(
    address_space: &mut X64AddressSpace,
    address: u64,
    buffer: &mut [u8],
    scanned_bytes: &mut u64,
    limits: TargetedKernelSearchLimits,
) -> Result<bool> {
    reserve_bytes(scanned_bytes, buffer.len(), limits)?;
    Ok(address_space.read_virtual_exact(address, buffer).is_ok())
}

pub(crate) fn reserve_bytes(
    scanned_bytes: &mut u64,
    amount: usize,
    limits: TargetedKernelSearchLimits,
) -> Result<()> {
    let next = scanned_bytes.checked_add(amount as u64).ok_or(
        MemoryWindowsError::TargetedScanBudgetExceeded {
            resource: "kernel-byte",
            limit: limits.maximum_scanned_bytes,
        },
    )?;
    if next > limits.maximum_scanned_bytes {
        return Err(MemoryWindowsError::TargetedScanBudgetExceeded {
            resource: "kernel-byte",
            limit: limits.maximum_scanned_bytes,
        });
    }
    *scanned_bytes = next;
    Ok(())
}

pub(crate) fn read_module_bytes(
    address_space: &mut X64AddressSpace,
    address: u64,
    buffer: &mut [u8],
    scanned_bytes: &mut u64,
    limits: TargetedKernelSearchLimits,
) -> Result<()> {
    reserve_bytes(scanned_bytes, buffer.len(), limits)?;
    address_space
        .read_virtual_exact(address, buffer)
        .map_err(|_| MemoryWindowsError::MalformedModuleList)
}

pub(crate) fn read_pe_u32(
    address_space: &mut X64AddressSpace,
    address: u64,
    limits: TargetedKernelSearchLimits,
    scanned_bytes: &mut u64,
) -> Result<Option<u32>> {
    let mut bytes = [0u8; 4];
    if !read_counted(address_space, address, &mut bytes, scanned_bytes, limits)? {
        return Ok(None);
    }
    Ok(Some(u32::from_le_bytes(bytes)))
}

pub(crate) fn read_field_u64(
    address_space: &mut X64AddressSpace,
    base: u64,
    offset: u16,
    scanned_bytes: &mut u64,
    limits: TargetedKernelSearchLimits,
) -> Result<u64> {
    let address = base
        .checked_add(u64::from(offset))
        .ok_or(MemoryWindowsError::MalformedModuleList)?;
    let mut bytes = [0u8; 8];
    read_module_bytes(address_space, address, &mut bytes, scanned_bytes, limits)?;
    Ok(u64::from_le_bytes(bytes))
}

pub(crate) fn read_field_u32(
    address_space: &mut X64AddressSpace,
    base: u64,
    offset: u16,
    scanned_bytes: &mut u64,
    limits: TargetedKernelSearchLimits,
) -> Result<u32> {
    let address = base
        .checked_add(u64::from(offset))
        .ok_or(MemoryWindowsError::MalformedModuleList)?;
    let mut bytes = [0u8; 4];
    read_module_bytes(address_space, address, &mut bytes, scanned_bytes, limits)?;
    Ok(u32::from_le_bytes(bytes))
}

pub(crate) fn read_field_u16(
    address_space: &mut X64AddressSpace,
    base: u64,
    offset: u16,
    scanned_bytes: &mut u64,
    limits: TargetedKernelSearchLimits,
) -> Result<u16> {
    let address = base
        .checked_add(u64::from(offset))
        .ok_or(MemoryWindowsError::MalformedModuleList)?;
    let mut bytes = [0u8; 2];
    read_module_bytes(address_space, address, &mut bytes, scanned_bytes, limits)?;
    Ok(u16::from_le_bytes(bytes))
}

pub(crate) fn signed_offset(address: u64, offset: i32) -> Option<u64> {
    if offset.is_negative() {
        address.checked_sub(u64::from(offset.unsigned_abs()))
    } else {
        address.checked_add(offset as u64)
    }
}

pub(crate) fn rva_range_is_inside(rva: u32, length: u32, image_size: u32) -> bool {
    rva.checked_add(length).is_some_and(|end| end <= image_size)
}

pub(crate) fn rva_range_contains(start: u32, length: u32, value: u32) -> bool {
    start
        .checked_add(length)
        .is_some_and(|end| (start..end).contains(&value))
}

pub(crate) fn is_kernel_pointer(address: u64) -> bool {
    is_canonical_address(address) && address >> 63 == 1
}

pub(crate) fn validate_limit(valid: bool, reason: &'static str) -> Result<()> {
    if valid {
        Ok(())
    } else {
        Err(MemoryWindowsError::InvalidTargetedScanLimit { reason })
    }
}

pub(crate) fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
}

pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
}
