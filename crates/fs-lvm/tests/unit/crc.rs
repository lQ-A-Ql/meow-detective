use super::*;

#[test]
fn crc_empty() {
    // Known LVM2 CRC of zero-length input = initial value unchanged
    // after processing zero bytes, the init value remains.
    assert_eq!(lvm_crc32(&[]), CRC_INIT);
}

#[test]
fn crc_known_vector() {
    // Test vector: the string "LABELONE" followed by zeroes
    // LVM label header bytes 0..31: "LABELONE" + 8B sector + 4B CRC + 4B offset + "LVM2 001"
    let mut data = [0u8; 32];
    data[0..8].copy_from_slice(b"LABELONE");
    // sector_number = 1
    data[8..16].copy_from_slice(&1u64.to_le_bytes());
    // crc placeholder (bytes 16..20 are zero for computation)
    // data_offset = 32
    data[20..24].copy_from_slice(&32u32.to_le_bytes());
    data[24..32].copy_from_slice(b"LVM2 001");

    // The CRC is computed over bytes 20..32 (offset to end of these 32 bytes).
    // In a real label sector it would be bytes 20..512. For a minimal test,
    // compute over what we have.
    let crc = lvm_crc32(&data[20..]);
    assert_ne!(crc, 0, "CRC should be non-zero for non-empty data");
    assert_ne!(
        crc, CRC_INIT,
        "CRC should differ from initial value after processing data"
    );
}

#[test]
fn crc_deterministic() {
    let data = b"test LVM2 metadata";
    assert_eq!(lvm_crc32(data), lvm_crc32(data));
}

#[test]
fn crc_differs_on_change() {
    let a = b"identical";
    let b = b"identicaL"; // one bit flipped
    assert_ne!(lvm_crc32(a), lvm_crc32(b));
}
