pub fn initialize(image: &mut [u8]) {
    let bps = usize::from(u16::from_le_bytes(image[11..13].try_into().unwrap()));
    let cluster_size = bps * usize::from(image[13]);
    let lcn = u64::from_le_bytes(image[48..56].try_into().unwrap()) as usize;
    let offset = lcn * cluster_size;
    let record_size = 1usize << (-(image[64] as i8) as usize);
    assert_eq!(record_size, 1024);
    let count = ((image.len() - offset) / record_size).min(10);
    let real_size = count * record_size;
    let clusters = real_size.div_ceil(cluster_size);
    assert!(count > 0 && clusters <= u16::MAX as usize && lcn <= u16::MAX as usize);
    let record = &mut image[offset..offset + record_size];
    record.fill(0);
    record[..4].copy_from_slice(b"FILE");
    record[4..6].copy_from_slice(&48u16.to_le_bytes());
    record[6..8].copy_from_slice(&((record_size / 512 + 1) as u16).to_le_bytes());
    record[16..18].copy_from_slice(&1u16.to_le_bytes());
    record[20..22].copy_from_slice(&56u16.to_le_bytes());
    record[22..24].copy_from_slice(&1u16.to_le_bytes());
    record[24..28].copy_from_slice(&132u32.to_le_bytes());
    record[28..32].copy_from_slice(&(record_size as u32).to_le_bytes());
    let attribute = &mut record[56..128];
    attribute[..4].copy_from_slice(&0x80u32.to_le_bytes());
    attribute[4..8].copy_from_slice(&72u32.to_le_bytes());
    attribute[8] = 1;
    attribute[24..32].copy_from_slice(&((clusters - 1) as u64).to_le_bytes());
    attribute[32..34].copy_from_slice(&64u16.to_le_bytes());
    attribute[40..48].copy_from_slice(&((clusters * cluster_size) as u64).to_le_bytes());
    for position in [48, 56] {
        attribute[position..position + 8].copy_from_slice(&(real_size as u64).to_le_bytes());
    }
    attribute[64] = 0x22;
    attribute[65..67].copy_from_slice(&(clusters as u16).to_le_bytes());
    attribute[67..69].copy_from_slice(&(lcn as u16).to_le_bytes());
    record[128..132].copy_from_slice(&u32::MAX.to_le_bytes());
    record[48..50].copy_from_slice(&[0x55, 0xaa]);
    for index in 1..=record_size / 512 {
        let tail = index * 512 - 2;
        let original = [record[tail], record[tail + 1]];
        record[48 + index * 2..50 + index * 2].copy_from_slice(&original);
        record[tail..tail + 2].copy_from_slice(&[0x55, 0xaa]);
    }
}
