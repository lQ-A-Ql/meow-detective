/// MBR partition table parser.
/// Reads 4 partition entries from a 512-byte MBR sector (bytes 446-509).

#[derive(Debug, Clone)]
pub struct PartitionEntry {
    pub bootable: bool,
    pub partition_type: u8,
    pub lba_start: u32,
    pub sector_count: u32,
}

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
            bootable,
            partition_type,
            lba_start,
            sector_count,
        });
    }
    entries
}

pub fn find_first_ntfs(entries: &[PartitionEntry]) -> Option<&PartitionEntry> {
    entries
        .iter()
        .find(|e| e.partition_type == 0x07 && e.lba_start > 0)
}
