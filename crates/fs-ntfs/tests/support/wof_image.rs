pub fn resident(
    record: &mut [u8],
    position: usize,
    kind: u32,
    name: &str,
    content: &[u8],
) -> usize {
    let name: Vec<_> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let content_offset = (24 + name.len() + 7) & !7;
    let length = (content_offset + content.len() + 7).max(content_offset + 8) & !7;
    record[position..position + 4].copy_from_slice(&kind.to_le_bytes());
    record[position + 4..position + 8].copy_from_slice(&(length as u32).to_le_bytes());
    record[position + 9] = (name.len() / 2) as u8;
    record[position + 10..position + 12].copy_from_slice(&24u16.to_le_bytes());
    record[position + 16..position + 20].copy_from_slice(&(content.len() as u32).to_le_bytes());
    record[position + 20..position + 22].copy_from_slice(&(content_offset as u16).to_le_bytes());
    record[position + 24..position + 24 + name.len()].copy_from_slice(&name);
    record[position + content_offset..position + content_offset + content.len()]
        .copy_from_slice(content);
    position + length
}

fn nonresident(record: &mut [u8], position: usize, name: &str, size: u64, sparse: bool) -> usize {
    let name: Vec<_> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let run_offset = (64 + name.len() + 7) & !7;
    let length = (run_offset + 8 + 7) & !7;
    record[position..position + 4].copy_from_slice(&0x80u32.to_le_bytes());
    record[position + 4..position + 8].copy_from_slice(&(length as u32).to_le_bytes());
    record[position + 8] = 1;
    record[position + 9] = (name.len() / 2) as u8;
    record[position + 10..position + 12].copy_from_slice(&64u16.to_le_bytes());
    record[position + 12..position + 14]
        .copy_from_slice(&(if sparse { 0x8000u16 } else { 0 }).to_le_bytes());
    let clusters = size.div_ceil(512).max(1);
    record[position + 24..position + 32].copy_from_slice(&(clusters - 1).to_le_bytes());
    record[position + 32..position + 34].copy_from_slice(&(run_offset as u16).to_le_bytes());
    record[position + 40..position + 48]
        .copy_from_slice(&(if sparse { 0 } else { clusters * 512 }).to_le_bytes());
    record[position + 48..position + 56].copy_from_slice(&size.to_le_bytes());
    record[position + 56..position + 64].copy_from_slice(&size.to_le_bytes());
    record[position + 64..position + 64 + name.len()].copy_from_slice(&name);
    let run = position + run_offset;
    record[run] = if sparse { 0x04 } else { 0x14 };
    record[run + 1..run + 5].copy_from_slice(&(clusters as u32).to_le_bytes());
    if !sparse {
        record[run + 5] = 64;
    }
    position + length
}

pub fn bytes(algorithm: u32, logical_size: u64, compressed: &[u8]) -> Vec<u8> {
    let mut image = vec![0; 32768 + compressed.len().div_ceil(512) * 512];
    image[3..11].copy_from_slice(b"NTFS    ");
    image[11..13].copy_from_slice(&512u16.to_le_bytes());
    image[13] = 1;
    image[48..56].copy_from_slice(&2u64.to_le_bytes());
    image[64] = (-10i8) as u8;
    let record = &mut image[7168..8192];
    record[..4].copy_from_slice(b"FILE");
    record[20..22].copy_from_slice(&56u16.to_le_bytes());
    let mut position = nonresident(record, 56, "", logical_size, true);
    position = nonresident(
        record,
        position,
        "WofCompressedData",
        compressed.len() as u64,
        false,
    );
    let mut reparse = Vec::new();
    reparse.extend_from_slice(&0x80000017u32.to_le_bytes());
    reparse.extend_from_slice(&16u16.to_le_bytes());
    reparse.extend_from_slice(&0u16.to_le_bytes());
    for word in [1, 2, 1, algorithm] {
        reparse.extend_from_slice(&word.to_le_bytes());
    }
    position = resident(record, position, 0xc0, "", &reparse);
    record[position..position + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    image[32768..32768 + compressed.len()].copy_from_slice(compressed);
    image
}
