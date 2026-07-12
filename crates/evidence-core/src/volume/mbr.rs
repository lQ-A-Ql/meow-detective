/// MBR partition table parser.
/// Reads 4 primary partition entries from a 512-byte MBR sector (bytes 446-509).
/// For extended/logical partitions, use `parse_mbr_full` which walks the EBR chain.
use std::io::{Read, Seek, SeekFrom};

/// MBR partition type classification for display and status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbrPartitionStatus {
    Supported,
    EncryptedBitLocker,
    Unsupported,
}

/// Result of classifying an MBR type code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MbrPartitionClass {
    pub name: &'static str,
    pub status: MbrPartitionStatus,
}

/// Map an MBR type code to a human-readable name and support status.
pub fn classify_mbr_partition_type(type_code: u8) -> MbrPartitionClass {
    match type_code {
        0x01 => MbrPartitionClass {
            name: "FAT12",
            status: MbrPartitionStatus::Supported,
        },
        0x04 => MbrPartitionClass {
            name: "FAT16",
            status: MbrPartitionStatus::Supported,
        },
        0x06 => MbrPartitionClass {
            name: "FAT16B",
            status: MbrPartitionStatus::Supported,
        },
        0x07 => MbrPartitionClass {
            name: "NTFS/exFAT/HPFS",
            status: MbrPartitionStatus::Supported,
        },
        0x0B => MbrPartitionClass {
            name: "FAT32 (CHS)",
            status: MbrPartitionStatus::Supported,
        },
        0x0C => MbrPartitionClass {
            name: "FAT32 (LBA)",
            status: MbrPartitionStatus::Supported,
        },
        0x0E => MbrPartitionClass {
            name: "FAT16B (LBA)",
            status: MbrPartitionStatus::Supported,
        },
        0x17 => MbrPartitionClass {
            name: "Hidden NTFS",
            status: MbrPartitionStatus::Supported,
        },
        0x1B => MbrPartitionClass {
            name: "Hidden FAT32",
            status: MbrPartitionStatus::Supported,
        },
        0x1C => MbrPartitionClass {
            name: "Hidden FAT32 (LBA)",
            status: MbrPartitionStatus::Supported,
        },
        0x42 => MbrPartitionClass {
            name: "BitLocker",
            status: MbrPartitionStatus::EncryptedBitLocker,
        },
        0x82 => MbrPartitionClass {
            name: "Linux swap",
            status: MbrPartitionStatus::Unsupported,
        },
        0x83 => MbrPartitionClass {
            name: "Linux",
            status: MbrPartitionStatus::Unsupported,
        },
        0x8E => MbrPartitionClass {
            name: "Linux LVM",
            status: MbrPartitionStatus::Supported,
        },
        0xA5 => MbrPartitionClass {
            name: "FreeBSD",
            status: MbrPartitionStatus::Unsupported,
        },
        0xA8 => MbrPartitionClass {
            name: "Apple UFS",
            status: MbrPartitionStatus::Unsupported,
        },
        0xAF => MbrPartitionClass {
            name: "Apple HFS/HFS+",
            status: MbrPartitionStatus::Unsupported,
        },
        0xEE => MbrPartitionClass {
            name: "GPT Protective",
            status: MbrPartitionStatus::Unsupported,
        },
        0xEF => MbrPartitionClass {
            name: "EFI System",
            status: MbrPartitionStatus::Unsupported,
        },
        0x05 | 0x0F => MbrPartitionClass {
            name: "Extended",
            status: MbrPartitionStatus::Unsupported,
        },
        0x00 => MbrPartitionClass {
            name: "Empty",
            status: MbrPartitionStatus::Unsupported,
        },
        _ => MbrPartitionClass {
            name: "Unknown",
            status: MbrPartitionStatus::Unsupported,
        },
    }
}

/// Extended partition type codes indicating an EBR chain.
pub const EXTENDED_TYPES: &[u8] = &[0x05, 0x0F];

const SECTOR_SIZE: u64 = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionEntry {
    /// Sequential partition number (0-based, across primary + logical).
    pub partition_number: usize,
    /// Whether this entry came from an EBR (logical) vs primary MBR.
    pub is_logical: bool,
    pub bootable: bool,
    pub partition_type: u8,
    pub lba_start: u32,
    pub sector_count: u32,
    /// For extended partitions: the absolute LBA of the first EBR.
    pub ebr_lba: Option<u32>,
}

impl PartitionEntry {
    pub fn is_extended(&self) -> bool {
        EXTENDED_TYPES.contains(&self.partition_type)
    }
}

/// Parse the 4 primary partition entries from the MBR sector.
pub fn parse_partition_table(mbr: &[u8]) -> Vec<PartitionEntry> {
    if mbr.len() < 512 {
        return vec![];
    }
    let mut entries = Vec::with_capacity(4);
    for i in 0..4 {
        let base = 446 + i * 16;
        let bootable = mbr[base] == 0x80;
        let partition_type = mbr[base + 4];
        let lba_start =
            u32::from_le_bytes([mbr[base + 8], mbr[base + 9], mbr[base + 10], mbr[base + 11]]);
        let sector_count = u32::from_le_bytes([
            mbr[base + 12],
            mbr[base + 13],
            mbr[base + 14],
            mbr[base + 15],
        ]);
        entries.push(PartitionEntry {
            partition_number: i,
            is_logical: false,
            bootable,
            partition_type,
            lba_start,
            sector_count,
            ebr_lba: if EXTENDED_TYPES.contains(&partition_type) {
                Some(lba_start)
            } else {
                None
            },
        });
    }
    entries
}

/// Parse all MBR partitions including logical partitions in EBR chains.
///
/// `reader` must be positioned at byte 0 and provide random access to the
/// entire disk. This function seeks to read the MBR and any EBR sectors.
pub fn parse_mbr_full<R: Read + Seek>(reader: &mut R) -> std::io::Result<Vec<PartitionEntry>> {
    // Read MBR
    reader.seek(SeekFrom::Start(0))?;
    let mut sector0 = [0u8; 512];
    reader.read_exact(&mut sector0)?;

    let mut entries: Vec<PartitionEntry> = parse_partition_table(&sector0)
        .into_iter()
        .filter(|e| e.partition_type != 0)
        .collect();
    let mut next_number = entries.len(); // next available partition_number

    // Walk EBR chain for each extended partition
    for i in 0..entries.len() {
        if !entries[i].is_extended() {
            continue;
        }
        let extended_start = entries[i].lba_start;
        let logical = parse_ebr_chain(reader, extended_start, &mut next_number)?;
        entries.extend(logical);
    }

    Ok(entries)
}

/// Walk the EBR (Extended Boot Record) chain starting at `first_ebr_lba`.
///
/// Each EBR sector contains:
///   - bytes 446-461: first entry — the actual logical volume (LBA relative to EBR)
///   - bytes 462-477: second entry — pointer to next EBR, or zeroed if end of chain
///
/// Returns all logical partitions found in the chain.
fn parse_ebr_chain<R: Read + Seek>(
    reader: &mut R,
    first_ebr_lba: u32,
    next_number: &mut usize,
) -> std::io::Result<Vec<PartitionEntry>> {
    let mut logical = Vec::new();
    let mut current_ebr = first_ebr_lba;
    let mut visited = std::collections::HashSet::new();
    let max_iterations = 256; // safety limit — realistic chains are < 50

    for _ in 0..max_iterations {
        if current_ebr == 0 || !visited.insert(current_ebr) {
            break;
        }

        reader.seek(SeekFrom::Start(current_ebr as u64 * SECTOR_SIZE))?;
        let mut ebr = [0u8; 512];
        reader.read_exact(&mut ebr)?;

        if ebr[510] != 0x55 || ebr[511] != 0xAA {
            break;
        }

        // First entry (bytes 446-461): logical volume
        let vol_type = ebr[450];
        if vol_type != 0 {
            let vol_lba = u32::from_le_bytes([ebr[454], ebr[455], ebr[456], ebr[457]]);
            let vol_count = u32::from_le_bytes([ebr[458], ebr[459], ebr[460], ebr[461]]);
            if vol_lba > 0 && vol_count > 0 {
                let abs_lba = current_ebr + vol_lba;
                logical.push(PartitionEntry {
                    partition_number: *next_number,
                    is_logical: true,
                    bootable: false,
                    partition_type: vol_type,
                    lba_start: abs_lba,
                    sector_count: vol_count,
                    ebr_lba: Some(current_ebr),
                });
                *next_number += 1;
            }
        }

        // Second entry (bytes 462-477): next EBR pointer
        let next_type = ebr[466];
        let next_lba = u32::from_le_bytes([ebr[470], ebr[471], ebr[472], ebr[473]]);
        if next_type == 0 || next_lba == 0 {
            break;
        }
        current_ebr = first_ebr_lba + next_lba;
    }

    Ok(logical)
}

pub fn find_first_ntfs(entries: &[PartitionEntry]) -> Option<&PartitionEntry> {
    entries
        .iter()
        .find(|e| e.partition_type == 0x07 && e.lba_start > 0)
}

/// Return all NTFS partition entries (primary + logical).
pub fn all_ntfs(entries: &[PartitionEntry]) -> Vec<&PartitionEntry> {
    entries
        .iter()
        .filter(|e| e.partition_type == 0x07 && e.lba_start > 0)
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/volume/mbr.rs"]
mod tests;
