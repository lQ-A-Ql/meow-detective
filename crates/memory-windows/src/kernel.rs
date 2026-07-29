use std::{collections::HashSet, path::Path};

use crate::{
    physical::PAGE_SIZE,
    x64::{is_canonical_address, translate_raw},
    MemoryWindowsError, RawMemoryImage, Result, X64AddressSpace,
};

const KDBG_TAG_OFFSET: u64 = 0x10;
const KDBG_SIZE_OFFSET: usize = 0x14;
const KDBG_KERNEL_BASE_OFFSET: usize = 0x18;
const KDBG_LOADED_MODULE_LIST_OFFSET: usize = 0x48;
const KDBG_MIN_SIZE: u32 = 0x100;
const KDBG_MAX_SIZE: u32 = 0x2_000;
const MAX_KDBG_CANDIDATES: usize = 32;
const MAX_PRESENT_PML4_ENTRIES: usize = 64;
const MAX_PML4_ROOT_CANDIDATES: usize = 16_384;
const MAX_MODULES: usize = 1_024;
const MAX_MODULE_NAME_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdbgCandidate {
    pub physical_address: u64,
    pub kernel_base: u64,
    pub loaded_module_list: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelModule {
    pub name: String,
    pub base: u64,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeCodeViewIdentity {
    pub timestamp: u32,
    pub size_of_image: u32,
    pub pdb_guid: String,
    pub pdb_age: u32,
    pub pdb_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelDiscovery {
    pub kdbg: KdbgCandidate,
    pub directory_table_base: u64,
    pub kernel_base: u64,
    pub modules: Vec<KernelModule>,
    pub fvevol: Option<KernelModule>,
    pub fvevol_identity: Option<PeCodeViewIdentity>,
}

/// Discovers the active Windows kernel mapping and loaded `fvevol.sys` identity
/// from a raw physical memory image without using external tools or symbols.
pub fn discover_kernel(path: &Path) -> Result<KernelDiscovery> {
    let mut image = RawMemoryImage::open(path)?;
    let candidates = find_kdbg_candidates(&mut image)?;
    for kdbg in candidates {
        let Some(directory_table_base) = find_kernel_page_table(&mut image, kdbg)? else {
            continue;
        };
        let mut address_space = X64AddressSpace::new(image, directory_table_base)?;
        let modules = enumerate_modules(&mut address_space, kdbg.loaded_module_list)?;
        let fvevol = modules
            .iter()
            .find(|module| module.name.eq_ignore_ascii_case("fvevol.sys"))
            .cloned();
        let fvevol_identity = fvevol
            .as_ref()
            .map(|module| read_pe_codeview_identity(&mut address_space, module))
            .transpose()?;
        return Ok(KernelDiscovery {
            kdbg,
            directory_table_base,
            kernel_base: kdbg.kernel_base,
            modules,
            fvevol,
            fvevol_identity,
        });
    }
    Err(MemoryWindowsError::KernelAddressSpaceNotFound)
}

/// Finds structural KDBG candidates only. The candidates contain no secret data.
pub fn find_kdbg_candidates(image: &mut RawMemoryImage) -> Result<Vec<KdbgCandidate>> {
    let tag_positions = image.scan_tag(*b"KDBG", MAX_KDBG_CANDIDATES * 8)?;
    let mut candidates = Vec::new();
    for tag_position in tag_positions {
        let Some(physical_address) = tag_position.checked_sub(KDBG_TAG_OFFSET) else {
            continue;
        };
        let mut header = [0u8; 0x58];
        if image.read_exact_at(physical_address, &mut header).is_err() {
            continue;
        }
        if header[KDBG_TAG_OFFSET as usize..KDBG_TAG_OFFSET as usize + 4] != *b"KDBG" {
            continue;
        }
        let size = u32_at(&header, KDBG_SIZE_OFFSET)?;
        let kernel_base = u64_at(&header, KDBG_KERNEL_BASE_OFFSET)?;
        let loaded_module_list = u64_at(&header, KDBG_LOADED_MODULE_LIST_OFFSET)?;
        if !(KDBG_MIN_SIZE..=KDBG_MAX_SIZE).contains(&size)
            || !is_kernel_address(kernel_base)
            || kernel_base & 0xFFF != 0
            || !is_kernel_address(loaded_module_list)
        {
            continue;
        }
        candidates.push(KdbgCandidate {
            physical_address,
            kernel_base,
            loaded_module_list,
        });
        if candidates.len() == MAX_KDBG_CANDIDATES {
            break;
        }
    }
    if candidates.is_empty() {
        return Err(MemoryWindowsError::KdbgNotFound);
    }
    Ok(candidates)
}

fn find_kernel_page_table(image: &mut RawMemoryImage, kdbg: KdbgCandidate) -> Result<Option<u64>> {
    let pml4_index = ((kdbg.kernel_base >> 39) & 0x1FF) as usize;
    let image_len = image.len();
    let mut candidates = Vec::new();
    image.visit_pages(|physical_address, page| {
        if looks_like_pml4(page, pml4_index, image_len) {
            candidates.push(physical_address);
        }
        candidates.len() < MAX_PML4_ROOT_CANDIDATES
    })?;
    for candidate in candidates {
        if kernel_image_is_mapped(image, candidate, kdbg.kernel_base) {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn looks_like_pml4(page: &[u8], kernel_index: usize, image_len: u64) -> bool {
    let mut present_count = 0usize;
    for index in 0..512 {
        let start = index * 8;
        let entry = u64::from_le_bytes(page[start..start + 8].try_into().expect("u64 slice"));
        if entry & 1 == 0 {
            continue;
        }
        present_count += 1;
        let frame = entry & 0x000F_FFFF_FFFF_F000;
        if frame >= image_len || frame & (PAGE_SIZE as u64 - 1) != 0 {
            return false;
        }
    }
    if present_count == 0 || present_count > MAX_PRESENT_PML4_ENTRIES {
        return false;
    }
    let start = kernel_index * 8;
    u64::from_le_bytes(page[start..start + 8].try_into().expect("u64 slice")) & 1 != 0
}

fn kernel_image_is_mapped(image: &mut RawMemoryImage, root: u64, kernel_base: u64) -> bool {
    let Ok(physical) = translate_raw(image, root, kernel_base) else {
        return false;
    };
    let mut header = [0u8; 0x200];
    if image.read_exact_at(physical, &mut header).is_err() || &header[..2] != b"MZ" {
        return false;
    }
    let Ok(pe_offset) = u32_at(&header, 0x3C).map(|value| value as usize) else {
        return false;
    };
    pe_offset + 26 <= header.len()
        && &header[pe_offset..pe_offset + 4] == b"PE\0\0"
        && header[pe_offset + 4..pe_offset + 6] == 0x8664_u16.to_le_bytes()
        && header[pe_offset + 24..pe_offset + 26] == 0x20B_u16.to_le_bytes()
}

fn enumerate_modules(
    address_space: &mut X64AddressSpace,
    list_head: u64,
) -> Result<Vec<KernelModule>> {
    let mut list = [0u8; 16];
    address_space.read_virtual_exact(list_head, &mut list)?;
    let mut current = u64::from_le_bytes(list[..8].try_into().expect("u64 slice"));
    let mut seen = HashSet::new();
    let mut modules = Vec::new();
    while current != list_head {
        if modules.len() == MAX_MODULES || !is_kernel_address(current) || !seen.insert(current) {
            return Err(MemoryWindowsError::MalformedModuleList);
        }
        let mut entry = [0u8; 0x68];
        address_space.read_virtual_exact(current, &mut entry)?;
        let next = u64_at(&entry, 0)?;
        let base = u64_at(&entry, 0x30)?;
        let size = u32_at(&entry, 0x40)?;
        let name_length = u16_at(&entry, 0x58)? as usize;
        let name_buffer = u64_at(&entry, 0x60)?;
        if !is_kernel_address(base)
            || size == 0
            || name_length == 0
            || name_length > MAX_MODULE_NAME_BYTES
            || !name_length.is_multiple_of(2)
            || !is_canonical_address(name_buffer)
        {
            return Err(MemoryWindowsError::MalformedModuleList);
        }
        let mut name_bytes = vec![0u8; name_length];
        address_space.read_virtual_exact(name_buffer, &mut name_bytes)?;
        let words = name_bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        let full_name = String::from_utf16_lossy(&words);
        let name = full_name
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&full_name)
            .to_string();
        modules.push(KernelModule { name, base, size });
        current = next;
    }
    Ok(modules)
}

fn read_pe_codeview_identity(
    address_space: &mut X64AddressSpace,
    module: &KernelModule,
) -> Result<PeCodeViewIdentity> {
    let mut headers = [0u8; 0x400];
    address_space.read_virtual_exact(module.base, &mut headers)?;
    if &headers[..2] != b"MZ" {
        return Err(MemoryWindowsError::MalformedPe);
    }
    let pe_offset = u32_at(&headers, 0x3C)? as usize;
    if pe_offset + 0x108 > headers.len() || &headers[pe_offset..pe_offset + 4] != b"PE\0\0" {
        return Err(MemoryWindowsError::MalformedPe);
    }
    let timestamp = u32_at(&headers, pe_offset + 8)?;
    let optional = pe_offset + 24;
    if u16_at(&headers, optional)? != 0x20B {
        return Err(MemoryWindowsError::MalformedPe);
    }
    let size_of_image = u32_at(&headers, optional + 56)?;
    let debug_directory = optional + 112 + 6 * 8;
    let debug_rva = u32_at(&headers, debug_directory)? as u64;
    let debug_size = u32_at(&headers, debug_directory + 4)? as usize;
    if debug_rva == 0 || !(28..=0x10_000).contains(&debug_size) {
        return Err(MemoryWindowsError::MalformedPe);
    }
    let mut directories = vec![0u8; debug_size];
    address_space.read_virtual_exact(module.base + debug_rva, &mut directories)?;
    for entry in directories.chunks_exact(28) {
        if u32_at(entry, 12)? != 2 {
            continue;
        }
        let data_size = u32_at(entry, 16)? as usize;
        let data_rva = u32_at(entry, 20)? as u64;
        if !(24..=0x1_000).contains(&data_size) || data_rva == 0 {
            continue;
        }
        let mut codeview = vec![0u8; data_size];
        address_space.read_virtual_exact(module.base + data_rva, &mut codeview)?;
        if &codeview[..4] != b"RSDS" {
            continue;
        }
        let guid = format_guid(&codeview[4..20]);
        let pdb_age = u32_at(&codeview, 20)?;
        let pdb_name_end = codeview[24..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|index| 24 + index)
            .unwrap_or(codeview.len());
        let pdb_name = String::from_utf8_lossy(&codeview[24..pdb_name_end]).to_string();
        return Ok(PeCodeViewIdentity {
            timestamp,
            size_of_image,
            pdb_guid: guid,
            pdb_age,
            pdb_name,
        });
    }
    Err(MemoryWindowsError::MalformedPe)
}

fn is_kernel_address(value: u64) -> bool {
    is_canonical_address(value) && value >> 63 == 1
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(MemoryWindowsError::MalformedPe)
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(MemoryWindowsError::MalformedPe)
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64> {
    bytes
        .get(offset..offset + 8)
        .and_then(|value| value.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(MemoryWindowsError::MalformedPe)
}

fn format_guid(bytes: &[u8]) -> String {
    let data1 = u32::from_le_bytes(bytes[0..4].try_into().expect("guid data1"));
    let data2 = u16::from_le_bytes(bytes[4..6].try_into().expect("guid data2"));
    let data3 = u16::from_le_bytes(bytes[6..8].try_into().expect("guid data3"));
    format!(
        "{data1:08x}-{data2:04x}-{data3:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}
