use super::parse;

#[test]
fn parses_090_superblock_and_role_table() {
    let mut bytes = [0u8; 4096];
    fn put(bytes: &mut [u8], word: usize, value: u32) {
        bytes[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    put(&mut bytes, 0, 0xa92b4efc);
    put(&mut bytes, 1, 0);
    put(&mut bytes, 2, 90);
    put(&mut bytes, 7, 5);
    put(&mut bytes, 8, 16);
    put(&mut bytes, 10, 3);
    put(&mut bytes, 65, 512);
    put(&mut bytes, 64, 2);
    put(&mut bytes, 128, 0);
    put(&mut bytes, 131, 0);
    put(&mut bytes, 160, 1);
    put(&mut bytes, 163, 2);
    put(&mut bytes, 992, 1);
    let mut sum = 0u64;
    for (index, word) in bytes.as_chunks::<4>().0.iter().enumerate() {
        if index != 38 {
            sum += u64::from(u32::from_le_bytes(*word));
        }
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    put(&mut bytes, 38, sum as u32);
    let parsed = parse(&bytes, 100).unwrap();
    assert_eq!(parsed.level, 5);
    assert_eq!(parsed.slot_index(), Some(2));
    assert!(parsed.checksum_valid);
}
