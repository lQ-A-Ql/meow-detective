pub fn checksum(header: &mut [u8]) {
    let value = header[..508]
        .as_chunks::<4>()
        .0
        .iter()
        .fold(0u32, |x, b| x ^ u32::from_le_bytes(*b));
    header[508..512].copy_from_slice(
        &match value {
            0 => 1,
            u32::MAX => u32::MAX - 1,
            _ => value,
        }
        .to_le_bytes(),
    );
}
pub fn seal(hive: &mut [u8], primary: u32, secondary: u32) {
    hive[4..8].copy_from_slice(&primary.to_le_bytes());
    hive[8..12].copy_from_slice(&secondary.to_le_bytes());
    hive[44..48].copy_from_slice(&1u32.to_le_bytes());
    hive[112..128].fill(1);
    hive[164..168].copy_from_slice(b"rmtm");
    checksum(hive);
}
pub fn log(base: &[u8], sequence: u32, pages: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut data = base[..512].to_vec();
    seal(&mut data, sequence, sequence);
    data[28..32].copy_from_slice(&6u32.to_le_bytes());
    checksum(&mut data);
    append(
        &mut data,
        sequence,
        u32::from_le_bytes(base[40..44].try_into().unwrap()),
        pages,
    );
    data
}
pub fn append(data: &mut Vec<u8>, sequence: u32, bins: u32, pages: &[(u32, Vec<u8>)]) {
    let size = (40 + pages.len() * 8 + pages.iter().map(|(_, p)| p.len()).sum::<usize>())
        .div_ceil(512)
        * 512;
    let mut entry = vec![0; size];
    entry[..4].copy_from_slice(b"HvLE");
    entry[4..8].copy_from_slice(&(size as u32).to_le_bytes());
    entry[12..16].copy_from_slice(&sequence.to_le_bytes());
    entry[16..20].copy_from_slice(&bins.to_le_bytes());
    entry[20..24].copy_from_slice(&(pages.len() as u32).to_le_bytes());
    let mut cursor = 40 + pages.len() * 8;
    for (index, (offset, bytes)) in pages.iter().enumerate() {
        entry[40 + index * 8..44 + index * 8].copy_from_slice(&offset.to_le_bytes());
        entry[44 + index * 8..48 + index * 8].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        entry[cursor..cursor + bytes.len()].copy_from_slice(bytes);
        cursor += bytes.len();
    }
    let h1 = hash(&entry[40..]);
    entry[24..32].copy_from_slice(&h1.to_le_bytes());
    let h2 = hash(&entry[..32]);
    entry[32..40].copy_from_slice(&h2.to_le_bytes());
    data.extend(entry);
}
fn hash(bytes: &[u8]) -> u64 {
    let (mut a, mut b) = (0x7a4e55c5u32, 0x82ef4d88u32);
    let mix = |a: &mut u32, b: &mut u32, n: u32| {
        *a = a.wrapping_add(n);
        *b ^= *a;
        *a = a.rotate_left(20).wrapping_add(*b);
        *b = b.rotate_left(9) ^ *a;
        *a = a.rotate_left(27).wrapping_add(*b);
        *b = b.rotate_left(19);
    };
    for word in bytes.as_chunks::<4>().0.iter() {
        mix(&mut a, &mut b, u32::from_le_bytes(*word));
    }
    mix(&mut a, &mut b, 0x80);
    mix(&mut a, &mut b, 0);
    ((b as u64) << 32) | (a as u64)
}
