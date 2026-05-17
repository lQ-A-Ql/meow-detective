/// GPT (GUID Partition Table) parser.
/// Reads GPT header at LBA 1 + partition entries at LBA 2.
///
/// GPT Header (92 bytes): "EFI PART" magic, revision, header size,
/// first/last usable LBA, partition entry LBA, count, size.
#[derive(Debug, Clone)]
pub struct GptHeader {
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub partition_entry_lba: u64,
    pub partition_count: u32,
    pub entry_size: u32,
}

/// GPT Partition Entry (128 bytes): type GUID, unique GUID, start/end LBA, name.
#[derive(Debug, Clone)]
pub struct GptPartition {
    pub type_guid: [u8; 16],
    pub start_lba: u64,
    pub end_lba: u64,
    pub name: String,
}

/// Read GPT header from sector 1 (offset 512)
pub fn parse_gpt_header(data: &[u8]) -> Option<GptHeader> {
    if data.len() < 92 { return None; }
    if &data[0..8] != b"EFI PART" { return None; }
    let header_size = u32::from_le_bytes(data[12..16].try_into().ok()?);
    if header_size < 92 { return None; }
    Some(GptHeader {
        first_usable_lba: u64::from_le_bytes(data[40..48].try_into().ok()?),
        last_usable_lba: u64::from_le_bytes(data[48..56].try_into().ok()?),
        partition_entry_lba: u64::from_le_bytes(data[72..80].try_into().ok()?),
        partition_count: u32::from_le_bytes(data[80..84].try_into().ok()?),
        entry_size: u32::from_le_bytes(data[84..88].try_into().ok()?),
    })
}

/// Read partition entries from a buffer containing all entries.
/// `data` should be entry_count * entry_size bytes.
pub fn parse_gpt_entries(data: &[u8], entry_size: u32, count: u32) -> Vec<GptPartition> {
    let mut parts = Vec::new();
    for i in 0..count {
        let off = i as usize * entry_size as usize;
        if off + entry_size as usize > data.len() { break; }
        let entry = &data[off..off + entry_size as usize];
        let mut type_guid = [0u8; 16];
        type_guid.copy_from_slice(&entry[0..16]);
        let start = u64::from_le_bytes(entry[32..40].try_into().unwrap_or([0;8]));
        let end = u64::from_le_bytes(entry[40..48].try_into().unwrap_or([0;8]));
        if start == 0 && end == 0 { continue; }
        let name_raw = &entry[56..128];
        let chars: Vec<u16> = name_raw.chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        let name = String::from_utf16_lossy(&chars);
        parts.push(GptPartition { type_guid, start_lba: start, end_lba: end, name });
    }
    parts
}

/// Known partition type GUIDs
const MS_BASIC_DATA: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44,
    0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
];

/// Find first partition that looks like a Windows data partition (NTFS/FAT)
pub fn find_first_data_partition(parts: &[GptPartition]) -> Option<&GptPartition> {
    parts.iter().find(|p| p.type_guid == MS_BASIC_DATA && p.start_lba > 0)
}
