use crate::{MemoryWindowsError, Result, X64AddressSpace};

use super::{
    limits::{TargetedKernelPeImage, TargetedKernelSearchLimits},
    utils::{read_counted, read_pe_u32, rva_range_contains, rva_range_is_inside, u16_at, u32_at},
};

pub(crate) fn find_export_address(
    address_space: &mut X64AddressSpace,
    image: TargetedKernelPeImage,
    wanted: &str,
    limits: TargetedKernelSearchLimits,
    scanned_bytes: &mut u64,
) -> Result<Option<u64>> {
    let Some(tables) = read_export_tables(address_space, image, limits, scanned_bytes)? else {
        return Ok(None);
    };
    resolve_named_export(address_space, image, wanted, tables, limits, scanned_bytes)
}

struct ExportTables {
    functions: usize,
    functions_rva: u32,
    name_rvas: Vec<u8>,
    ordinals: Vec<u8>,
}

fn read_export_tables(
    address_space: &mut X64AddressSpace,
    image: TargetedKernelPeImage,
    limits: TargetedKernelSearchLimits,
    scanned_bytes: &mut u64,
) -> Result<Option<ExportTables>> {
    let Some(export_address) = image.base.checked_add(u64::from(image.export_rva)) else {
        return Ok(None);
    };
    let mut directory = [0u8; 40];
    if !read_counted(
        address_space,
        export_address,
        &mut directory,
        scanned_bytes,
        limits,
    )? {
        return Ok(None);
    }
    let functions = usize::try_from(u32_at(&directory, 20).ok_or(MemoryWindowsError::MalformedPe)?)
        .map_err(|_| MemoryWindowsError::MalformedPe)?;
    let names = usize::try_from(u32_at(&directory, 24).ok_or(MemoryWindowsError::MalformedPe)?)
        .map_err(|_| MemoryWindowsError::MalformedPe)?;
    if functions == 0
        || names == 0
        || functions > limits.maximum_export_entries
        || names > functions
    {
        return Ok(None);
    }
    let names_rva = u32_at(&directory, 32).ok_or(MemoryWindowsError::MalformedPe)?;
    let ordinals_rva = u32_at(&directory, 36).ok_or(MemoryWindowsError::MalformedPe)?;
    let functions_rva = u32_at(&directory, 28).ok_or(MemoryWindowsError::MalformedPe)?;
    let names_len = names
        .checked_mul(4)
        .ok_or(MemoryWindowsError::MalformedPe)?;
    let ordinals_len = names
        .checked_mul(2)
        .ok_or(MemoryWindowsError::MalformedPe)?;
    let functions_len = functions
        .checked_mul(4)
        .ok_or(MemoryWindowsError::MalformedPe)?;
    if !rva_range_is_inside(names_rva, names_len as u32, image.size_of_image)
        || !rva_range_is_inside(ordinals_rva, ordinals_len as u32, image.size_of_image)
        || !rva_range_is_inside(functions_rva, functions_len as u32, image.size_of_image)
    {
        return Ok(None);
    }
    let Some(names_address) = image.base.checked_add(u64::from(names_rva)) else {
        return Ok(None);
    };
    let Some(ordinals_address) = image.base.checked_add(u64::from(ordinals_rva)) else {
        return Ok(None);
    };
    let mut name_rvas = vec![0u8; names_len];
    let mut ordinals = vec![0u8; ordinals_len];
    if !read_counted(
        address_space,
        names_address,
        &mut name_rvas,
        scanned_bytes,
        limits,
    )? || !read_counted(
        address_space,
        ordinals_address,
        &mut ordinals,
        scanned_bytes,
        limits,
    )? {
        return Ok(None);
    }
    Ok(Some(ExportTables {
        functions,
        functions_rva,
        name_rvas,
        ordinals,
    }))
}

fn resolve_named_export(
    address_space: &mut X64AddressSpace,
    image: TargetedKernelPeImage,
    wanted: &str,
    tables: ExportTables,
    limits: TargetedKernelSearchLimits,
    scanned_bytes: &mut u64,
) -> Result<Option<u64>> {
    let names = tables.name_rvas.len() / 4;
    for index in 0..names {
        let name_rva =
            u32_at(&tables.name_rvas, index * 4).ok_or(MemoryWindowsError::MalformedPe)?;
        if !rva_range_is_inside(name_rva, 1, image.size_of_image) {
            continue;
        }
        let Some(name_address) = image.base.checked_add(u64::from(name_rva)) else {
            continue;
        };
        let Some(name) = read_export_name(address_space, name_address, limits, scanned_bytes)?
        else {
            continue;
        };
        if name != wanted {
            continue;
        }
        let ordinal = usize::from(
            u16_at(&tables.ordinals, index * 2).ok_or(MemoryWindowsError::MalformedPe)?,
        );
        if ordinal >= tables.functions {
            return Ok(None);
        }
        let Some(function_address) = image
            .base
            .checked_add(u64::from(tables.functions_rva))
            .and_then(|value| value.checked_add(ordinal as u64 * 4))
        else {
            return Ok(None);
        };
        let Some(function_rva) =
            read_pe_u32(address_space, function_address, limits, scanned_bytes)?
        else {
            return Ok(None);
        };
        if !rva_range_is_inside(function_rva, 1, image.size_of_image)
            || rva_range_contains(image.export_rva, image.export_size, function_rva)
        {
            return Ok(None);
        }
        return Ok(image.base.checked_add(u64::from(function_rva)));
    }
    Ok(None)
}

fn read_export_name(
    address_space: &mut X64AddressSpace,
    address: u64,
    limits: TargetedKernelSearchLimits,
    scanned_bytes: &mut u64,
) -> Result<Option<String>> {
    let mut bytes = Vec::new();
    const NAME_READ_CHUNK: usize = 64;
    while bytes.len() < limits.maximum_module_name_bytes {
        let remaining = limits.maximum_module_name_bytes - bytes.len();
        let Some(chunk_address) = address.checked_add(bytes.len() as u64) else {
            return Ok(None);
        };
        let page_remaining = crate::physical::PAGE_SIZE as u64
            - (chunk_address & (crate::physical::PAGE_SIZE as u64 - 1));
        let take = remaining
            .min(NAME_READ_CHUNK)
            .min(usize::try_from(page_remaining).unwrap_or(NAME_READ_CHUNK));
        let mut chunk = vec![0u8; take];
        if !read_counted(
            address_space,
            chunk_address,
            &mut chunk,
            scanned_bytes,
            limits,
        )? {
            return Ok(None);
        }
        let Some(end) = chunk.iter().position(|byte| *byte == 0) else {
            bytes.extend_from_slice(&chunk);
            continue;
        };
        bytes.extend_from_slice(&chunk[..end]);
        return Ok(String::from_utf8(bytes).ok());
    }
    Ok(None)
}
