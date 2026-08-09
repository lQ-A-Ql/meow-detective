use super::*;

#[test]
fn crc32c_matches_the_standard_check_vector() {
    assert_eq!(checksum(b"123456789"), 0xE306_9283);
}

#[test]
fn crc32c_checksum_parts_matches_the_contiguous_checksum() {
    let data: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    assert_eq!(
        checksum_parts(&[&data[..333], &data[333..]]),
        checksum(&data)
    );
}
