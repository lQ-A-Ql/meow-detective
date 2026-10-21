/// Windows DRIVE_LAYOUT_INFORMATION_EX (48-byte header) followed by native
/// PARTITION_INFORMATION_EX entries (144 bytes). Unknown layouts fail closed.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct CachedPartition {
    pub(super) offset: u64,
    pub(super) mounted_id: Vec<u8>,
}

pub(super) fn partitions(bytes: &[u8]) -> Vec<CachedPartition> {
    let Some(header) = bytes.get(..48) else {
        return Vec::new();
    };
    let style = u32::from_le_bytes(header[..4].try_into().unwrap());
    let count = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
    if style > 1 || count > 4096 || bytes.len() != 48 + count * 144 {
        return Vec::new();
    }
    bytes[48..]
        .chunks_exact(144)
        .filter_map(|entry| {
            let offset = u64::from_le_bytes(entry[8..16].try_into().ok()?);
            let length = u64::from_le_bytes(entry[16..24].try_into().ok()?);
            if length == 0 || offset == 0 || offset.checked_add(length).is_none() {
                return None;
            }
            let mounted_id = if style == 0 {
                if header[8..12] == [0; 4] {
                    return None;
                }
                [header[8..12].to_vec(), offset.to_le_bytes().to_vec()].concat()
            } else {
                if entry[56..72] == [0; 16] {
                    return None;
                }
                [b"DMIO:ID:".to_vec(), entry[56..72].to_vec()].concat()
            };
            Some(CachedPartition { offset, mounted_id })
        })
        .collect()
}
