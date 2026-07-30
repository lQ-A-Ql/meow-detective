use crate::{bootstrap::ProcessorStartBlock, MemoryWindowsError, Result, X64AddressSpace};

use super::{
    exports::find_export_address,
    limits::{
        TargetedCodeViewIdentity, TargetedKernelDiscovery, TargetedKernelPeImage,
        TargetedKernelSearchLimits, TargetedKernelSearchReport, MAX_CODEVIEW_BYTES, MAX_IMAGE_SIZE,
        MAX_PE_HEADER_BYTES, PE_EXPORT_DIRECTORY_LEN,
    },
    modules::KernelModule,
    utils::{read_counted, reserve_bytes, rva_range_is_inside, u16_at, u32_at},
};

const KERNEL_IMAGE_ALIGNMENT: u64 = 64 * 1024;
const PAGE_SEARCH_ALIGNMENT: u64 = crate::physical::PAGE_SIZE as u64;

/// Locates the kernel image from a validated processor-start target and resolves
/// the standard PE export `PsLoadedModuleList`.
///
/// The search is a bounded, read-only heuristic. It uses only PE-defined fields
/// and never infers a private kernel structure offset. A caller must still
/// validate any module-list result and any recovered key against the evidence
/// volume oracle.
pub fn discover_kernel_from_processor_start_block(
    address_space: &mut X64AddressSpace,
    start_block: ProcessorStartBlock,
    limits: TargetedKernelSearchLimits,
) -> Result<TargetedKernelDiscovery> {
    if start_block.directory_table_base != address_space.directory_table_base() {
        return Err(MemoryWindowsError::TargetedAddressSpaceMismatch);
    }
    discover_kernel_from_entry(address_space, start_block.long_mode_target, limits)
}

/// Performs the bounded kernel-image search from a trusted virtual entry point.
pub fn discover_kernel_from_entry(
    address_space: &mut X64AddressSpace,
    long_mode_target: u64,
    limits: TargetedKernelSearchLimits,
) -> Result<TargetedKernelDiscovery> {
    if !super::utils::is_kernel_pointer(long_mode_target) {
        return Err(MemoryWindowsError::NonCanonicalAddress {
            address: long_mode_target,
        });
    }
    let (image, mut report) = locate_kernel_image(address_space, long_mode_target, limits)?;
    let mut scanned_bytes = report.bytes_scanned;
    let Some(ps_loaded_module_list) = find_export_address(
        address_space,
        image,
        "PsLoadedModuleList",
        limits,
        &mut scanned_bytes,
    )?
    else {
        return Err(MemoryWindowsError::TargetedKernelImageNotFound);
    };
    report.bytes_scanned = scanned_bytes;
    Ok(TargetedKernelDiscovery {
        image,
        ps_loaded_module_list,
        report,
    })
}

fn locate_kernel_image(
    address_space: &mut X64AddressSpace,
    long_mode_target: u64,
    limits: TargetedKernelSearchLimits,
) -> Result<(TargetedKernelPeImage, TargetedKernelSearchReport)> {
    let mut report = TargetedKernelSearchReport {
        pages_scanned: 0,
        bytes_scanned: 0,
        unreadable_pages: 0,
        rejected_pe_candidates: 0,
    };
    let coarse_probe_limit = limits
        .maximum_pages
        .div_ceil((KERNEL_IMAGE_ALIGNMENT / PAGE_SEARCH_ALIGNMENT) as usize);
    let coarse_start = long_mode_target & !(KERNEL_IMAGE_ALIGNMENT - 1);
    if let Some(image) = probe_kernel_bases(
        address_space,
        long_mode_target,
        coarse_start,
        KERNEL_IMAGE_ALIGNMENT,
        coarse_probe_limit,
        limits,
        &mut report,
    )? {
        return Ok((image, report));
    }
    let page_start = long_mode_target & !(PAGE_SEARCH_ALIGNMENT - 1);
    if let Some(image) = probe_kernel_bases(
        address_space,
        long_mode_target,
        page_start,
        PAGE_SEARCH_ALIGNMENT,
        limits.maximum_pages,
        limits,
        &mut report,
    )? {
        return Ok((image, report));
    }
    Err(MemoryWindowsError::TargetedKernelImageNotFound)
}

#[allow(clippy::too_many_arguments)]
fn probe_kernel_bases(
    address_space: &mut X64AddressSpace,
    long_mode_target: u64,
    mut candidate: u64,
    stride: u64,
    probe_limit: usize,
    limits: TargetedKernelSearchLimits,
    report: &mut TargetedKernelSearchReport,
) -> Result<Option<TargetedKernelPeImage>> {
    let mut probes = 0usize;
    while probes < probe_limit && report.pages_scanned < limits.maximum_pages {
        probes += 1;
        report.pages_scanned += 1;
        let mut signature = [0u8; 2];
        reserve_bytes(&mut report.bytes_scanned, signature.len(), limits)?;
        if address_space
            .read_virtual_exact(candidate, &mut signature)
            .is_err()
        {
            report.unreadable_pages += 1;
        } else if signature == *b"MZ" {
            match read_pe_image_at(
                address_space,
                candidate,
                Some(long_mode_target),
                true,
                limits,
                &mut report.bytes_scanned,
            ) {
                Ok(Some(image)) => return Ok(Some(image)),
                Ok(None) => report.rejected_pe_candidates += 1,
                Err(error) => return Err(error),
            }
        }
        if candidate < stride {
            break;
        }
        candidate -= stride;
    }
    Ok(None)
}

pub(crate) fn read_pe_image_at(
    address_space: &mut X64AddressSpace,
    base: u64,
    entry: Option<u64>,
    require_export: bool,
    limits: TargetedKernelSearchLimits,
    scanned_bytes: &mut u64,
) -> Result<Option<TargetedKernelPeImage>> {
    let mut dos = [0u8; 0x40];
    if !read_counted(address_space, base, &mut dos, scanned_bytes, limits)? {
        return Ok(None);
    }
    let pe_offset = u32_at(&dos, 0x3C).ok_or(MemoryWindowsError::MalformedPe)? as u64;
    if pe_offset > 0x100_000 {
        return Ok(None);
    }
    let Some(nt_address) = base.checked_add(pe_offset) else {
        return Ok(None);
    };
    let mut nt = [0u8; 24];
    if !read_counted(address_space, nt_address, &mut nt, scanned_bytes, limits)?
        || &nt[..4] != b"PE\0\0"
        || u16_at(&nt, 4) != Some(0x8664)
    {
        return Ok(None);
    }
    let optional_size = usize::from(u16_at(&nt, 20).ok_or(MemoryWindowsError::MalformedPe)?);
    if !(112 + 8..=MAX_PE_HEADER_BYTES).contains(&optional_size) {
        return Ok(None);
    }
    let Some(optional_address) = nt_address.checked_add(24) else {
        return Ok(None);
    };
    let mut optional = vec![0u8; optional_size];
    if !read_counted(
        address_space,
        optional_address,
        &mut optional,
        scanned_bytes,
        limits,
    )? || u16_at(&optional, 0) != Some(0x20B)
    {
        return Ok(None);
    }
    build_pe_image(base, pe_offset, &nt, &optional, entry, require_export)
}

fn build_pe_image(
    base: u64,
    pe_offset: u64,
    nt: &[u8],
    optional: &[u8],
    entry: Option<u64>,
    require_export: bool,
) -> Result<Option<TargetedKernelPeImage>> {
    let size_of_image = u32_at(optional, 56).ok_or(MemoryWindowsError::MalformedPe)?;
    let number_of_sections = u16_at(nt, 6).ok_or(MemoryWindowsError::MalformedPe)?;
    let Some(image_end) = base.checked_add(u64::from(size_of_image)) else {
        return Ok(None);
    };
    if !(crate::physical::PAGE_SIZE as u32..=MAX_IMAGE_SIZE).contains(&size_of_image)
        || entry.is_some_and(|entry| !(base..image_end).contains(&entry))
        || number_of_sections == 0
        || number_of_sections > 96
    {
        return Ok(None);
    }
    let number_of_rva_and_sizes = u32_at(optional, 108).ok_or(MemoryWindowsError::MalformedPe)?;
    if number_of_rva_and_sizes < 1 {
        return Ok(None);
    }
    let export_rva = u32_at(optional, 112).ok_or(MemoryWindowsError::MalformedPe)?;
    let export_size = u32_at(optional, 116).ok_or(MemoryWindowsError::MalformedPe)?;
    if require_export
        && (export_rva == 0
            || export_size < PE_EXPORT_DIRECTORY_LEN as u32
            || !rva_range_is_inside(export_rva, export_size, size_of_image))
    {
        return Ok(None);
    }
    if export_rva != 0
        && (export_size < PE_EXPORT_DIRECTORY_LEN as u32
            || !rva_range_is_inside(export_rva, export_size, size_of_image))
    {
        return Ok(None);
    }
    let (debug_rva, debug_size) = if number_of_rva_and_sizes >= 7 {
        (
            u32_at(optional, 112 + 6 * 8).ok_or(MemoryWindowsError::MalformedPe)?,
            u32_at(optional, 112 + 6 * 8 + 4).ok_or(MemoryWindowsError::MalformedPe)?,
        )
    } else {
        (0, 0)
    };
    if debug_rva != 0
        && (debug_size < 28 || !rva_range_is_inside(debug_rva, debug_size, size_of_image))
    {
        return Ok(None);
    }
    let section_table = base
        .checked_add(pe_offset)
        .and_then(|address| address.checked_add(24))
        .and_then(|address| address.checked_add(optional.len() as u64))
        .ok_or(MemoryWindowsError::MalformedPe)?;
    let section_table_rva = pe_offset
        .checked_add(24)
        .and_then(|value| value.checked_add(optional.len() as u64))
        .ok_or(MemoryWindowsError::MalformedPe)?;
    let section_table_bytes = u64::from(number_of_sections)
        .checked_mul(40)
        .ok_or(MemoryWindowsError::MalformedPe)?;
    if section_table_rva > u64::from(u32::MAX)
        || section_table_bytes > u64::from(u32::MAX)
        || !rva_range_is_inside(
            section_table_rva as u32,
            section_table_bytes as u32,
            size_of_image,
        )
    {
        return Ok(None);
    }
    Ok(Some(TargetedKernelPeImage {
        base,
        time_date_stamp: u32_at(nt, 8).ok_or(MemoryWindowsError::MalformedPe)?,
        size_of_image,
        section_count: number_of_sections,
        section_table,
        export_rva,
        export_size,
        debug_rva,
        debug_size,
    }))
}

pub fn read_codeview_identity(
    address_space: &mut X64AddressSpace,
    image: TargetedKernelPeImage,
    limits: TargetedKernelSearchLimits,
    scanned_bytes: &mut u64,
) -> Result<Option<TargetedCodeViewIdentity>> {
    if image.debug_rva == 0 || image.debug_size == 0 {
        return Ok(None);
    }
    if image.debug_size as usize > MAX_CODEVIEW_BYTES || !image.debug_size.is_multiple_of(28) {
        return Err(MemoryWindowsError::MalformedPe);
    }
    let debug_address = image
        .base
        .checked_add(u64::from(image.debug_rva))
        .ok_or(MemoryWindowsError::MalformedPe)?;
    let mut directories = vec![0u8; image.debug_size as usize];
    if !read_counted(
        address_space,
        debug_address,
        &mut directories,
        scanned_bytes,
        limits,
    )? {
        return Ok(None);
    }
    for entry in directories.chunks_exact(28) {
        if u32_at(entry, 12).ok_or(MemoryWindowsError::MalformedPe)? != 2 {
            continue;
        }
        let data_size = u32_at(entry, 16).ok_or(MemoryWindowsError::MalformedPe)?;
        let data_rva = u32_at(entry, 20).ok_or(MemoryWindowsError::MalformedPe)?;
        if !(24..=MAX_CODEVIEW_BYTES as u32).contains(&data_size)
            || !rva_range_is_inside(data_rva, data_size, image.size_of_image)
        {
            return Err(MemoryWindowsError::MalformedPe);
        }
        let data_address = image
            .base
            .checked_add(u64::from(data_rva))
            .ok_or(MemoryWindowsError::MalformedPe)?;
        let mut codeview = vec![0u8; data_size as usize];
        if !read_counted(
            address_space,
            data_address,
            &mut codeview,
            scanned_bytes,
            limits,
        )? {
            return Ok(None);
        }
        if &codeview[..4] != b"RSDS" {
            continue;
        }
        let guid = format_guid(&codeview[4..20]);
        let age = u32_at(&codeview, 20).ok_or(MemoryWindowsError::MalformedPe)?;
        let end = codeview[24..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|offset| offset + 24)
            .unwrap_or(codeview.len());
        let pdb_name =
            std::str::from_utf8(&codeview[24..end]).map_err(|_| MemoryWindowsError::MalformedPe)?;
        return TargetedCodeViewIdentity::new(guid, age, pdb_name.to_string()).map(Some);
    }
    Ok(None)
}

fn format_guid(bytes: &[u8]) -> String {
    let data1 = u32::from_le_bytes(bytes[0..4].try_into().expect("GUID data1"));
    let data2 = u16::from_le_bytes(bytes[4..6].try_into().expect("GUID data2"));
    let data3 = u16::from_le_bytes(bytes[6..8].try_into().expect("GUID data3"));
    format!(
        "{data1:08X}-{data2:04X}-{data3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

pub(crate) fn read_module_pe_image(
    address_space: &mut X64AddressSpace,
    module: &KernelModule,
    limits: TargetedKernelSearchLimits,
    scanned_bytes: &mut u64,
) -> Result<TargetedKernelPeImage> {
    let image = read_pe_image_at(
        address_space,
        module.base,
        Some(module.base),
        false,
        limits,
        scanned_bytes,
    )?
    .ok_or(MemoryWindowsError::MalformedPe)?;
    if image.size_of_image != module.size {
        return Err(MemoryWindowsError::TargetedModuleImageSizeMismatch {
            module_size: module.size,
            pe_size: image.size_of_image,
        });
    }
    Ok(image)
}
