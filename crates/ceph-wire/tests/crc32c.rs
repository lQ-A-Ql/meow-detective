use ceph_wire::crc32c::{ceph_crc32c, crc32c, CEPH_CRC32C_INITIAL};

#[test]
fn ceph_crc32c_uses_running_remainder_without_final_xor() {
    assert_eq!(ceph_crc32c(b""), u32::MAX);
    assert_eq!(ceph_crc32c(b"123456789"), 0x1cf9_6d7c);
    assert_eq!(crc32c(0, b"123456789"), 0x58e3_fa20);
    assert_eq!(CEPH_CRC32C_INITIAL, u32::MAX);
}

#[test]
fn crc32c_can_be_continued_across_fragments() {
    let first = crc32c(CEPH_CRC32C_INITIAL, b"blue");
    assert_eq!(crc32c(first, b"store"), ceph_crc32c(b"bluestore"));
}
