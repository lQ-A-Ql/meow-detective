use app_services::gpt;

#[test]
fn parse_header_valid() {
    let mut data = vec![0u8; 512];
    data[0..8].copy_from_slice(b"EFI PART");
    data[12..16].copy_from_slice(&92u32.to_le_bytes());
    data[40..48].copy_from_slice(&34u64.to_le_bytes());
    let last = (1024u64 * 1024).to_le_bytes();
    data[48..56].copy_from_slice(&last);
    data[72..80].copy_from_slice(&2u64.to_le_bytes());
    data[80..84].copy_from_slice(&128u32.to_le_bytes());
    data[84..88].copy_from_slice(&128u32.to_le_bytes());

    let hdr = gpt::parse_gpt_header(&data).unwrap();
    assert_eq!(hdr.first_usable_lba, 34);
    assert_eq!(hdr.partition_count, 128);
}

#[test]
fn parse_entries_finds_partition() {
    let count = 4u32;
    let esz = 128u32;
    let mut data = vec![0u8; (count * esz) as usize];

    let e1 = 1 * 128;
    // MS Basic Data GUID
    data[e1..e1+16].copy_from_slice(&[
        0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44,
        0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
    ]);
    data[e1+32..e1+40].copy_from_slice(&2048u64.to_le_bytes());
    data[e1+40..e1+48].copy_from_slice(&100000u64.to_le_bytes());
    for (i, c) in "Windows".encode_utf16().enumerate() {
        data[e1+56+i*2..e1+56+i*2+2].copy_from_slice(&c.to_le_bytes());
    }

    let parts = gpt::parse_gpt_entries(&data, esz, count);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].start_lba, 2048);

    let found = gpt::find_first_data_partition(&parts);
    assert!(found.is_some());
}
