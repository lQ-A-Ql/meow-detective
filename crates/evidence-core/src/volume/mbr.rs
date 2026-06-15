/// MBR partition table parser.
/// Reads 4 primary partition entries from a 512-byte MBR sector (bytes 446-509).
/// For extended/logical partitions, use `parse_mbr_full` which walks the EBR chain.
use std::io::{Read, Seek, SeekFrom};

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
mod tests {
    use super::*;

    #[test]
    fn parses_four_primary_partitions() {
        let mut mbr = vec![0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        // Entry 0: NTFS at LBA 2048, 100000 sectors
        let base0 = 446;
        mbr[base0] = 0x80; // bootable
        mbr[base0 + 4] = 0x07; // NTFS
        mbr[base0 + 8..base0 + 12].copy_from_slice(&2048u32.to_le_bytes());
        mbr[base0 + 12..base0 + 16].copy_from_slice(&100000u32.to_le_bytes());
        // Entry 1: empty
        // Entry 2: extended (0x0F) at LBA 200000
        let base2 = 446 + 2 * 16;
        mbr[base2 + 4] = 0x0F;
        mbr[base2 + 8..base2 + 12].copy_from_slice(&200000u32.to_le_bytes());
        mbr[base2 + 12..base2 + 16].copy_from_slice(&50000u32.to_le_bytes());

        let entries = parse_partition_table(&mbr);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].partition_number, 0);
        assert!(!entries[0].is_logical);
        assert!(entries[0].bootable);
        assert_eq!(entries[0].partition_type, 0x07);
        assert_eq!(entries[0].lba_start, 2048);
        assert!(entries[2].is_extended());
        assert_eq!(entries[2].ebr_lba, Some(200000));
    }

    #[test]
    fn parses_ebr_chain() {
        use std::io::Cursor;
        let sectors = 220_066; // EBR at LBA 220063 + 2 sectors padding
        let mut disk = vec![0u8; 512 * sectors];

        // MBR at sector 0
        disk[510] = 0x55;
        disk[511] = 0xAA;
        let base0 = 446;
        disk[base0 + 4] = 0x07; // NTFS primary
        disk[base0 + 8..base0 + 12].copy_from_slice(&2048u32.to_le_bytes());
        disk[base0 + 12..base0 + 16].copy_from_slice(&10000u32.to_le_bytes());
        // Entry 1: extended at LBA 200000
        let base1 = 446 + 16;
        disk[base1 + 4] = 0x0F;
        disk[base1 + 8..base1 + 12].copy_from_slice(&200000u32.to_le_bytes());
        disk[base1 + 12..base1 + 16].copy_from_slice(&50000u32.to_le_bytes());

        // EBR at sector 200000: logical volume at relative LBA 63
        let ebr1_off = 200000 * 512;
        disk[ebr1_off + 510] = 0x55;
        disk[ebr1_off + 511] = 0xAA;
        disk[ebr1_off + 446 + 4] = 0x07; // NTFS logical
        disk[ebr1_off + 446 + 8..ebr1_off + 446 + 12].copy_from_slice(&63u32.to_le_bytes());
        disk[ebr1_off + 446 + 12..ebr1_off + 446 + 16].copy_from_slice(&20000u32.to_le_bytes());
        // Second entry: next EBR at relative LBA 20063
        disk[ebr1_off + 462 + 4] = 0x05;
        disk[ebr1_off + 462 + 8..ebr1_off + 462 + 12].copy_from_slice(&20063u32.to_le_bytes());

        // EBR at sector 200000+20063 = 220063: logical volume at relative LBA 63
        let ebr2_off = (200000 + 20063) * 512;
        disk[ebr2_off + 510] = 0x55;
        disk[ebr2_off + 511] = 0xAA;
        disk[ebr2_off + 446 + 4] = 0x07;
        disk[ebr2_off + 446 + 8..ebr2_off + 446 + 12].copy_from_slice(&63u32.to_le_bytes());
        disk[ebr2_off + 446 + 12..ebr2_off + 446 + 16].copy_from_slice(&10000u32.to_le_bytes());
        // Second entry: zeroed (end of chain)
        // (already zeros)

        let mut cursor = Cursor::new(&disk);
        let entries = parse_mbr_full(&mut cursor).unwrap();

        assert_eq!(
            entries.len(),
            4,
            "1 primary NTFS + 1 extended + 2 logical = 4 entries"
        );
        // Entry 0: primary NTFS
        assert_eq!(entries[0].partition_number, 0);
        assert!(!entries[0].is_logical);
        assert_eq!(entries[0].lba_start, 2048);
        // Entry 1: extended (not a data partition)
        assert!(entries[1].is_extended());
        // Entry 2: first logical
        assert_eq!(entries[2].partition_number, 2);
        assert!(entries[2].is_logical);
        assert_eq!(entries[2].lba_start, 200000 + 63);
        assert_eq!(entries[2].sector_count, 20000);
        // Entry 3: second logical
        assert_eq!(entries[3].partition_number, 3);
        assert!(entries[3].is_logical);
        assert_eq!(entries[3].lba_start, 200000 + 20063 + 63);
    }

    #[test]
    fn all_ntfs_returns_primary_and_logical() {
        let mut entries = Vec::new();
        entries.push(PartitionEntry {
            partition_number: 0,
            is_logical: false,
            bootable: true,
            partition_type: 0x07,
            lba_start: 2048,
            sector_count: 1000,
            ebr_lba: None,
        });
        entries.push(PartitionEntry {
            partition_number: 1,
            is_logical: true,
            bootable: false,
            partition_type: 0x07,
            lba_start: 200063,
            sector_count: 2000,
            ebr_lba: Some(200000),
        });
        entries.push(PartitionEntry {
            partition_number: 2,
            is_logical: false,
            bootable: false,
            partition_type: 0x83,
            lba_start: 400000,
            sector_count: 5000,
            ebr_lba: None,
        });
        let ntfs = all_ntfs(&entries);
        assert_eq!(ntfs.len(), 2);
        assert_eq!(ntfs[0].partition_number, 0);
        assert!(ntfs[1].is_logical);
    }
}
