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
    pub index: usize,
    pub type_guid: [u8; 16],
    pub start_lba: u64,
    pub end_lba: u64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GptPartitionType {
    EfiSystem,
    MicrosoftReserved,
    MicrosoftBasicData,
    WindowsRecovery,
    Unknown,
}

/// Read GPT header from sector 1 (offset 512)
pub fn parse_gpt_header(data: &[u8]) -> Option<GptHeader> {
    if data.len() < 92 {
        return None;
    }
    if &data[0..8] != b"EFI PART" {
        return None;
    }
    let header_size = u32::from_le_bytes(data[12..16].try_into().ok()?);
    if header_size < 92 {
        return None;
    }
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
    // Guard against malformed GPT headers: the minimum useful entry is 128 bytes
    // (enough to hold type GUID, unique GUID, start/end LBA, and name fields).
    if (entry_size as usize) < 128 {
        return Vec::new();
    }
    let mut parts = Vec::new();
    for i in 0..count {
        let off = i as usize * entry_size as usize;
        if off + entry_size as usize > data.len() {
            break;
        }
        let entry = &data[off..off + entry_size as usize];
        let mut type_guid = [0u8; 16];
        type_guid.copy_from_slice(&entry[0..16]);
        let start = u64::from_le_bytes(entry[32..40].try_into().unwrap_or([0; 8]));
        let end = u64::from_le_bytes(entry[40..48].try_into().unwrap_or([0; 8]));
        if start == 0 && end == 0 {
            continue;
        }
        // Safe name extraction — ensure we don't read past entry boundary
        let name_end = 128.min(entry.len());
        let name_raw = if name_end > 56 {
            &entry[56..name_end]
        } else {
            &[]
        };
        let chars: Vec<u16> = name_raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        let name = String::from_utf16_lossy(&chars);
        parts.push(GptPartition {
            index: i as usize + 1,
            type_guid,
            start_lba: start,
            end_lba: end,
            name,
        });
    }
    parts
}

/// Known partition type GUIDs
const MS_BASIC_DATA: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
];
const EFI_SYSTEM: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];
const MS_RESERVED: [u8; 16] = [
    0x16, 0xE3, 0xC9, 0xE3, 0x5C, 0x0B, 0xB8, 0x4D, 0x81, 0x7D, 0xF9, 0x2D, 0xF0, 0x02, 0x15, 0xAE,
];
const WINDOWS_RECOVERY: [u8; 16] = [
    0xA4, 0xBB, 0x94, 0xDE, 0xD1, 0x06, 0x40, 0x4D, 0xA1, 0x6A, 0xBF, 0xD5, 0x01, 0x79, 0xD6, 0xAC,
];

pub fn classify_partition_type(type_guid: &[u8; 16]) -> GptPartitionType {
    match *type_guid {
        EFI_SYSTEM => GptPartitionType::EfiSystem,
        MS_RESERVED => GptPartitionType::MicrosoftReserved,
        MS_BASIC_DATA => GptPartitionType::MicrosoftBasicData,
        WINDOWS_RECOVERY => GptPartitionType::WindowsRecovery,
        _ => GptPartitionType::Unknown,
    }
}

pub fn partition_type_name(partition_type: GptPartitionType) -> &'static str {
    match partition_type {
        GptPartitionType::EfiSystem => "EFI system",
        GptPartitionType::MicrosoftReserved => "Microsoft reserved",
        GptPartitionType::MicrosoftBasicData => "Microsoft basic data",
        GptPartitionType::WindowsRecovery => "Windows recovery",
        GptPartitionType::Unknown => "Unknown",
    }
}

pub fn format_guid(type_guid: &[u8; 16]) -> String {
    let d1 = u32::from_le_bytes([type_guid[0], type_guid[1], type_guid[2], type_guid[3]]);
    let d2 = u16::from_le_bytes([type_guid[4], type_guid[5]]);
    let d3 = u16::from_le_bytes([type_guid[6], type_guid[7]]);
    format!(
        "{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        type_guid[8],
        type_guid[9],
        type_guid[10],
        type_guid[11],
        type_guid[12],
        type_guid[13],
        type_guid[14],
        type_guid[15]
    )
}

/// Find first partition that looks like a Windows data partition (NTFS/FAT)
pub fn find_first_data_partition(parts: &[GptPartition]) -> Option<&GptPartition> {
    parts
        .iter()
        .find(|p| p.type_guid == MS_BASIC_DATA && p.start_lba > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gpt_entries_with_minimum_entry_size() {
        // Create a minimal GPT entry (128 bytes)
        let mut entry = vec![0u8; 128];
        // Set type GUID (first 16 bytes)
        entry[0..16].copy_from_slice(&MS_BASIC_DATA);
        // Set start LBA at offset 32
        entry[32..40].copy_from_slice(&100u64.to_le_bytes());
        // Set end LBA at offset 40
        entry[40..48].copy_from_slice(&200u64.to_le_bytes());
        // Set name "Test" in UTF-16LE at offset 56
        entry[56] = b'T';
        entry[57] = 0;
        entry[58] = b'e';
        entry[59] = 0;
        entry[60] = b's';
        entry[61] = 0;
        entry[62] = b't';
        entry[63] = 0;

        let parts = parse_gpt_entries(&entry, 128, 1);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].name, "Test");
        assert_eq!(parts[0].start_lba, 100);
        assert_eq!(parts[0].end_lba, 200);
    }

    #[test]
    fn parse_gpt_entries_with_larger_entry_size() {
        // Entry size > 128 should still work
        let mut entry = vec![0u8; 256];
        entry[0..16].copy_from_slice(&MS_BASIC_DATA);
        entry[32..40].copy_from_slice(&100u64.to_le_bytes());
        entry[40..48].copy_from_slice(&200u64.to_le_bytes());

        let parts = parse_gpt_entries(&entry, 256, 1);
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn parse_gpt_entries_rejects_small_entry_size() {
        let entry = vec![0u8; 64];
        let parts = parse_gpt_entries(&entry, 64, 1);
        assert!(parts.is_empty());
    }

    #[test]
    fn parse_gpt_entries_skips_empty_partitions() {
        let entry = vec![0u8; 128];
        // start=0, end=0 means empty partition
        let parts = parse_gpt_entries(&entry, 128, 1);
        assert!(parts.is_empty());
    }

    #[test]
    fn format_guid_basic() {
        let guid = MS_BASIC_DATA;
        let formatted = format_guid(&guid);
        // Should contain dashes
        assert!(formatted.contains('-'));
        assert_eq!(formatted.len(), 36); // XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX
    }
}
