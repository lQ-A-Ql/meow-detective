const HEADER_SIZE: u64 = 0x1000;
const MAGIC_HVLE: &[u8; 4] = b"HvLE";
const MAGIC_DIRT: &[u8; 4] = b"DIRT";

#[derive(Debug, Clone)]
pub struct SyntheticEntry {
    pub operation: u16,
    pub sequence_number: u32,
    pub timestamp: Option<u64>,
    pub key_path: String,
    pub value_name: Option<String>,
    pub data_before: Option<Vec<u8>>,
    pub data_after: Option<Vec<u8>>,
}

pub fn build_synthetic_log1(entries: &[SyntheticEntry]) -> Vec<u8> {
    build_log(true, entries)
}

pub fn build_synthetic_log2(entries: &[SyntheticEntry]) -> Vec<u8> {
    build_log(false, entries)
}

fn build_log(primary: bool, entries: &[SyntheticEntry]) -> Vec<u8> {
    let mut data = vec![0u8; HEADER_SIZE as usize];
    data[0..4].copy_from_slice(if primary { MAGIC_HVLE } else { MAGIC_DIRT });
    data[4..8].copy_from_slice(
        &entries
            .first()
            .map(|entry| entry.sequence_number)
            .unwrap_or(0)
            .to_le_bytes(),
    );
    data[8..12].copy_from_slice(
        &entries
            .last()
            .map(|entry| entry.sequence_number)
            .unwrap_or(0)
            .to_le_bytes(),
    );
    data[12..16].copy_from_slice(&(if primary { 0u32 } else { 1u32 }).to_le_bytes());
    for entry in entries {
        append_entry(&mut data, entry);
    }
    data
}

fn append_entry(data: &mut Vec<u8>, entry: &SyntheticEntry) {
    let timestamp = entry.timestamp.unwrap_or(0x01db_a000_0000_0000);
    let key_path = entry.key_path.encode_utf16().collect::<Vec<_>>();
    let value_name = entry
        .value_name
        .as_deref()
        .unwrap_or("")
        .encode_utf16()
        .collect::<Vec<_>>();
    let before = entry.data_before.as_deref().unwrap_or_default();
    let after = entry.data_after.as_deref().unwrap_or_default();
    let entry_size = 4
        + 4
        + 2
        + 2
        + 8
        + 2
        + key_path.len() * 2
        + 2
        + value_name.len() * 2
        + 4
        + before.len()
        + 4
        + after.len();
    data.extend_from_slice(&(entry_size as u32).to_le_bytes());
    data.extend_from_slice(&entry.sequence_number.to_le_bytes());
    data.extend_from_slice(&entry.operation.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&timestamp.to_le_bytes());
    append_utf16(data, &key_path);
    append_utf16(data, &value_name);
    data.extend_from_slice(&(before.len() as u32).to_le_bytes());
    data.extend_from_slice(before);
    data.extend_from_slice(&(after.len() as u32).to_le_bytes());
    data.extend_from_slice(after);
}

fn append_utf16(data: &mut Vec<u8>, value: &[u16]) {
    data.extend_from_slice(&(value.len() as u16).to_le_bytes());
    for unit in value {
        data.extend_from_slice(&unit.to_le_bytes());
    }
}
