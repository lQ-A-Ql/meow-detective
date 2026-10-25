const BASE: usize = 4096;

/// Initialize a valid single-bin hive for explicitly addressed fixture cells.
pub fn initialize(data: &mut [u8], root: u32) {
    assert!(data.len() >= 8192 && data.len().is_multiple_of(4096));
    let length = data.len() - BASE;
    data[..BASE].fill(0);
    data[..4].copy_from_slice(b"regf");
    for (at, value) in [
        (4, 1u32),
        (8, 1),
        (20, 1),
        (24, 5),
        (32, 1),
        (36, root),
        (40, length as u32),
        (44, 1),
    ] {
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    data[BASE..BASE + 32].fill(0);
    data[BASE..BASE + 4].copy_from_slice(b"hbin");
    data[BASE + 8..BASE + 12].copy_from_slice(&(length as u32).to_le_bytes());
    for cell in data[BASE + 32..].as_chunks_mut::<8>().0 {
        cell.fill(0);
        cell[..4].copy_from_slice(&8i32.to_le_bytes());
    }
    let checksum = data[..508]
        .as_chunks::<4>()
        .0
        .iter()
        .fold(0u32, |sum, word| sum ^ u32::from_le_bytes(*word));
    let checksum = match checksum {
        0 => 1,
        u32::MAX => u32::MAX - 1,
        value => value,
    };
    data[508..512].copy_from_slice(&checksum.to_le_bytes());
}

/// Reserve an aligned allocated cell, keeping the remaining space as free cells.
pub fn allocate(data: &mut [u8], offset: u32, total_length: usize) {
    assert!(offset >= 32 && offset.is_multiple_of(8));
    let at = BASE + offset as usize;
    let length = total_length.max(8).next_multiple_of(8);
    let old = i32::from_le_bytes(data[at..at + 4].try_into().unwrap());
    let old_length = if old < 0 {
        old.checked_neg().unwrap() as usize
    } else {
        8
    };
    assert!(old_length.is_multiple_of(8));
    for cell in data[at..at + length.max(old_length)].as_chunks_mut::<8>().0 {
        cell.fill(0);
        cell[..4].copy_from_slice(&8i32.to_le_bytes());
    }
    data[at..at + length].fill(0);
    data[at..at + 4].copy_from_slice(&(-(length as i32)).to_le_bytes());
}
