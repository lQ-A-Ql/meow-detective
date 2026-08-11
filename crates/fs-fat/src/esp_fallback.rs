//! FAT32 write support, scoped to installing the UEFI fallback boot path.
//!
//! A fresh VMware VM has an empty NVRAM, so a GPT disk whose ESP lacks
//! `\EFI\BOOT\BOOTX64.EFI` cannot boot even though the vendor loader under
//! `\EFI\<vendor>\` is intact. This module creates `\EFI\BOOT` when missing
//! and copies the boot chain into it using short (8.3, uppercase) names only
//! — no LFN entries are emitted. All mutations go through the caller-
//! supplied block IO, which in production is the emulation copy-on-write
//! overlay: the evidence image itself is never written.
//!
//! Scope is deliberately FAT32-only; FAT12/16 ESPs are reported as
//! `ErrorKind::Unsupported` rather than handled half-way.

use crate::types::{FatReader, FatType};
use evidence_core::filesystem::invalid_fs_data;
use evidence_core::EvidenceReader;
use std::io::{self, ErrorKind};

/// Partition-relative block IO backing the writer. Offsets are relative to
/// the volume start; the implementor translates them to physical disk
/// addresses (e.g. the COW overlay plus the ESP partition offset).
pub trait FatBlockIo {
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<()>;
    fn write_at(&self, offset: u64, data: &[u8]) -> io::Result<()>;
}

/// Outcome of one fallback installation run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EspFallbackInstall {
    /// `\EFI\BOOT` did not exist and was created.
    pub created_boot_directory: bool,
    /// Files newly written into `\EFI\BOOT`.
    pub files_written: Vec<String>,
    /// Files that were already present and left untouched.
    pub files_skipped: Vec<String>,
}

const FAT32_EOC: u32 = 0x0FFF_FFFF;
const FAT32_BAD_OR_EOC_MIN: u32 = 0x0FFF_FFF8;
const FAT32_VALUE_MASK: u32 = 0x0FFF_FFFF;
const FSINFO_FREE_UNKNOWN: u32 = 0xFFFF_FFFF;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LFN: u8 = 0x0F;
const ENTRY_FREE: u8 = 0xE5;
const ENTRY_END: u8 = 0x00;

/// Allocation bookkeeping for one install run: the clusters taken (for the
/// FSInfo accounting) and a monotonically advancing next-free cursor so
/// successive chains do not rescan the FAT from cluster 2.
#[derive(Default)]
struct AllocState {
    allocated: Vec<u32>,
    next_free: u32,
}

/// Create `\EFI\BOOT` when missing and place `files` (8.3 uppercase names)
/// into it. Existing files are never overwritten — they are reported in
/// `files_skipped` so the caller can decide whether anything was needed.
pub fn install_efi_fallback(
    reader: Box<dyn EvidenceReader>,
    volume_offset: u64,
    io: &dyn FatBlockIo,
    files: &[(String, Vec<u8>)],
) -> io::Result<EspFallbackInstall> {
    if files.is_empty() {
        return Err(invalid_fs_data("no boot chain files supplied"));
    }
    let layout = FatReader::open(reader, volume_offset)?;
    if layout.fat_type != FatType::Fat32 {
        return Err(io::Error::new(
            ErrorKind::Unsupported,
            "EFI fallback installation supports FAT32 volumes only",
        ));
    }
    for (name, data) in files {
        parse_sfn(name)?;
        if data.is_empty() {
            return Err(invalid_fs_data(format!("boot chain file {name} is empty")));
        }
    }
    let mut state = AllocState::default();
    let (efi_cluster, _) = ensure_directory(&layout, io, layout.root_cluster, "EFI", &mut state)?;
    let (boot_cluster, created) = ensure_boot_directory(&layout, io, efi_cluster, &mut state)?;
    let mut result = EspFallbackInstall {
        created_boot_directory: created,
        ..EspFallbackInstall::default()
    };
    for (name, data) in files {
        if find_sfn_entry(&layout, boot_cluster, name)?.is_some() {
            result.files_skipped.push(name.clone());
            continue;
        }
        let chain = allocate_clusters(&layout, io, chain_length(&layout, data.len()), &mut state)?;
        write_chain_data(&layout, io, &chain, data)?;
        insert_directory_entry(
            &layout,
            io,
            boot_cluster,
            &sfn_file_entry(name, chain[0], data.len() as u32),
            &mut state,
        )?;
        result.files_written.push(name.clone());
    }
    update_fsinfo(&layout, io, &state)?;
    Ok(result)
}

fn chain_length(layout: &FatReader, bytes: usize) -> u32 {
    (bytes as u64).div_ceil(layout.cluster_size).max(1) as u32
}

fn fat_region_offset(layout: &FatReader, fat_index: u32) -> u64 {
    layout.volume_offset
        + (layout.reserved_sectors as u64 + fat_index as u64 * layout.sectors_per_fat as u64)
            * layout.bytes_per_sector as u64
}

fn cluster_offset(layout: &FatReader, cluster: u32) -> io::Result<u64> {
    if cluster < 2 || cluster >= layout.cluster_count + 2 {
        return Err(invalid_fs_data(format!("cluster {cluster} out of range")));
    }
    Ok(layout.volume_offset
        + (layout.first_data_sector as u64
            + (cluster - 2) as u64 * layout.sectors_per_cluster as u64)
            * layout.bytes_per_sector as u64)
}

fn read_fat_entry(layout: &FatReader, io: &dyn FatBlockIo, cluster: u32) -> io::Result<u32> {
    let mut raw = [0u8; 4];
    io.read_at(fat_region_offset(layout, 0) + cluster as u64 * 4, &mut raw)?;
    Ok(u32::from_le_bytes(raw) & FAT32_VALUE_MASK)
}

fn write_fat_entry(
    layout: &FatReader,
    io: &dyn FatBlockIo,
    cluster: u32,
    value: u32,
) -> io::Result<()> {
    // Every FAT copy is updated: firmware or a repair tool may consult any.
    for fat_index in 0..layout.fat_count as u32 {
        let offset = fat_region_offset(layout, fat_index) + cluster as u64 * 4;
        let mut raw = [0u8; 4];
        io.read_at(offset, &mut raw)?;
        let merged = (u32::from_le_bytes(raw) & !FAT32_VALUE_MASK) | (value & FAT32_VALUE_MASK);
        io.write_at(offset, &merged.to_le_bytes())?;
    }
    Ok(())
}

fn allocate_clusters(
    layout: &FatReader,
    io: &dyn FatBlockIo,
    count: u32,
    state: &mut AllocState,
) -> io::Result<Vec<u32>> {
    let mut chain = Vec::with_capacity(count as usize);
    let mut candidate = state.next_free.max(2);
    while chain.len() < count as usize {
        if candidate >= layout.cluster_count + 2 {
            return Err(invalid_fs_data("FAT32 volume has no free clusters"));
        }
        if read_fat_entry(layout, io, candidate)? == 0 {
            chain.push(candidate);
        }
        candidate += 1;
    }
    state.next_free = candidate;
    for (index, &cluster) in chain.iter().enumerate() {
        let next = chain.get(index + 1).copied().unwrap_or(FAT32_EOC);
        write_fat_entry(layout, io, cluster, next)?;
        io.write_at(
            cluster_offset(layout, cluster)?,
            &vec![0u8; layout.cluster_size as usize],
        )?;
    }
    state.allocated.extend_from_slice(&chain);
    Ok(chain)
}

fn write_chain_data(
    layout: &FatReader,
    io: &dyn FatBlockIo,
    chain: &[u32],
    data: &[u8],
) -> io::Result<()> {
    for (index, chunk) in data.chunks(layout.cluster_size as usize).enumerate() {
        io.write_at(cluster_offset(layout, chain[index])?, chunk)?;
    }
    Ok(())
}

/// Locate `name` under `parent_cluster`, creating it as a directory when
/// absent. Returns the directory cluster; `created` reports the new case.
fn ensure_directory(
    layout: &FatReader,
    io: &dyn FatBlockIo,
    parent_cluster: u32,
    name: &str,
    state: &mut AllocState,
) -> io::Result<(u32, bool)> {
    if let Some(entry) = find_sfn_entry(layout, parent_cluster, name)? {
        if !entry.is_dir {
            return Err(invalid_fs_data(format!(
                "{name} exists and is not a directory"
            )));
        }
        return Ok((entry.cluster, false));
    }
    let cluster = allocate_clusters(layout, io, 1, state)?[0];
    let parent_ref = if parent_cluster == layout.root_cluster {
        0
    } else {
        parent_cluster
    };
    let mut dot_entries = Vec::with_capacity(64);
    dot_entries.extend_from_slice(&sfn_dot_entry(b'.', 1, cluster));
    dot_entries.extend_from_slice(&sfn_dot_entry(b'.', 2, parent_ref));
    io.write_at(cluster_offset(layout, cluster)?, &dot_entries)?;
    insert_directory_entry(
        layout,
        io,
        parent_cluster,
        &sfn_dir_entry(name, cluster),
        state,
    )?;
    Ok((cluster, true))
}

fn ensure_boot_directory(
    layout: &FatReader,
    io: &dyn FatBlockIo,
    efi_cluster: u32,
    state: &mut AllocState,
) -> io::Result<(u32, bool)> {
    ensure_directory(layout, io, efi_cluster, "BOOT", state)
}

struct FoundEntry {
    cluster: u32,
    is_dir: bool,
}

/// Case-insensitive lookup of a name (short entries only; LFN accumulators
/// are skipped) in a directory cluster chain.
fn find_sfn_entry(
    layout: &FatReader,
    dir_cluster: u32,
    name: &str,
) -> io::Result<Option<FoundEntry>> {
    let data = layout.walk_cluster_chain(dir_cluster)?;
    for entry in data.chunks_exact(32) {
        if entry[0] == ENTRY_END {
            break;
        }
        if entry[0] == ENTRY_FREE || entry[11] == ATTR_LFN {
            continue;
        }
        if crate::directory::read_sfn_name(entry).eq_ignore_ascii_case(name) {
            let cluster = u16::from_le_bytes([entry[26], entry[27]]) as u32
                | ((u16::from_le_bytes([entry[20], entry[21]]) as u32) << 16);
            return Ok(Some(FoundEntry {
                cluster,
                is_dir: entry[11] & ATTR_DIRECTORY != 0,
            }));
        }
    }
    Ok(None)
}

/// Write a 32-byte entry into the first free slot of the directory chain,
/// extending the chain with a fresh cluster when every slot is occupied.
/// The chain is walked cluster by cluster (with the same cycle guard the
/// reader uses) so the write offset always lands in the cluster that
/// actually holds the free slot — a concatenated-chain index would
/// misplace writes on fragmented directories.
fn insert_directory_entry(
    layout: &FatReader,
    io: &dyn FatBlockIo,
    dir_cluster: u32,
    entry: &[u8; 32],
    state: &mut AllocState,
) -> io::Result<()> {
    let mut visited = std::collections::HashSet::new();
    let mut cluster = dir_cluster;
    loop {
        if !visited.insert(cluster) || visited.len() > layout.cluster_count as usize {
            return Err(invalid_fs_data(
                "cycle or overrun in directory cluster chain",
            ));
        }
        let base = cluster_offset(layout, cluster)?;
        let mut data = vec![0u8; layout.cluster_size as usize];
        io.read_at(base, &mut data)?;
        for (index, slot) in data.chunks_exact(32).enumerate() {
            if slot[0] == ENTRY_FREE || slot[0] == ENTRY_END {
                let offset = base + (index * 32) as u64;
                io.write_at(offset, entry)?;
                if slot[0] == ENTRY_END {
                    // The end-of-directory marker moved one slot forward;
                    // keep a terminator behind the new entry so readers
                    // never scan into stale garbage.
                    if let Some(next) = data.get((index + 1) * 32..(index + 2) * 32) {
                        if next[0] != ENTRY_END {
                            io.write_at(offset + 32, &[0u8; 32])?;
                        }
                    }
                }
                return Ok(());
            }
        }
        let next = read_fat_entry(layout, io, cluster)?;
        if next >= FAT32_BAD_OR_EOC_MIN {
            let extension = allocate_clusters(layout, io, 1, state)?[0];
            write_fat_entry(layout, io, cluster, extension)?;
            cluster = extension;
        } else {
            cluster = next;
        }
    }
}

/// Refresh FSInfo free-cluster accounting. An unknown free count is left
/// unknown; a known count is decremented by what this run allocated. The
/// FSInfo sector number is the spec default (1); the signature check makes a
/// non-standard layout a no-op rather than a corrupting write.
fn update_fsinfo(layout: &FatReader, io: &dyn FatBlockIo, state: &AllocState) -> io::Result<()> {
    let allocated = &state.allocated;
    if allocated.is_empty() {
        return Ok(());
    }
    let info_offset = layout.volume_offset + layout.bytes_per_sector as u64;
    let mut sector = vec![0u8; layout.bytes_per_sector as usize];
    io.read_at(info_offset, &mut sector)?;
    let valid = sector[0..4] == 0x4161_5252u32.to_le_bytes()
        && sector[484..488] == 0x6141_7272u32.to_le_bytes();
    if !valid {
        return Ok(());
    }
    let free = u32::from_le_bytes(sector[488..492].try_into().unwrap_or([0; 4]));
    if free != FSINFO_FREE_UNKNOWN {
        let remaining = free.saturating_sub(allocated.len() as u32);
        sector[488..492].copy_from_slice(&remaining.to_le_bytes());
    }
    let hint = allocated.last().copied().unwrap_or(1) + 1;
    sector[492..496].copy_from_slice(&hint.to_le_bytes());
    io.write_at(info_offset, &sector)
}

/// Validate an 8.3 uppercase name and split it into stem/extension parts.
fn parse_sfn(name: &str) -> io::Result<(&str, &str)> {
    let (stem, extension) = name.split_once('.').unwrap_or((name, ""));
    let valid = !stem.is_empty()
        && stem.len() <= 8
        && extension.len() <= 3
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'.');
    if !valid {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("{name} is not an uppercase 8.3 name"),
        ));
    }
    Ok((stem, extension))
}

fn base_sfn_entry(stem: &str, extension: &str, cluster: u32, attributes: u8) -> [u8; 32] {
    let mut entry = [0u8; 32];
    entry[..8].fill(b' ');
    entry[..stem.len()].copy_from_slice(stem.as_bytes());
    entry[8..11].fill(b' ');
    entry[8..8 + extension.len()].copy_from_slice(extension.as_bytes());
    entry[11] = attributes;
    entry[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
    entry[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
    entry
}

fn sfn_file_entry(name: &str, cluster: u32, size: u32) -> [u8; 32] {
    let (stem, extension) = parse_sfn(name).unwrap_or(("", ""));
    let mut entry = base_sfn_entry(stem, extension, cluster, ATTR_ARCHIVE);
    entry[28..32].copy_from_slice(&size.to_le_bytes());
    entry
}

fn sfn_dir_entry(name: &str, cluster: u32) -> [u8; 32] {
    let (stem, extension) = parse_sfn(name).unwrap_or(("", ""));
    base_sfn_entry(stem, extension, cluster, ATTR_DIRECTORY)
}

/// The `.` / `..` entries of a fresh directory: one or two leading dots,
/// directory attribute, zero size.
fn sfn_dot_entry(dot: u8, dot_count: usize, cluster: u32) -> [u8; 32] {
    let mut entry = [0u8; 32];
    entry[..11].fill(b' ');
    for byte in entry.iter_mut().take(dot_count) {
        *byte = dot;
    }
    entry[11] = ATTR_DIRECTORY;
    entry[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
    entry[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
    entry
}
