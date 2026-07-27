use super::*;

#[test]
fn readers_yield_zero_when_out_of_range() {
    // A lying length field in untrusted metadata must not panic the reader.
    let bytes = [1u8, 2, 3];
    assert_eq!(le_u16(&bytes, 0), 0x0201);
    assert_eq!(le_u16(&bytes, 10), 0);
    assert_eq!(le_u32(&bytes, 0), 0, "only 3 bytes available");
    assert_eq!(le_u64(&bytes, 0), 0);
    assert_eq!(read_guid(&bytes, 0), [0u8; 16]);
}

#[test]
fn readers_decode_little_endian_in_range() {
    let bytes = [0x01u8, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    assert_eq!(le_u16(&bytes, 0), 0x0201);
    assert_eq!(le_u32(&bytes, 0), 0x0403_0201);
    assert_eq!(le_u64(&bytes, 0), 0x0807_0605_0403_0201);
}

#[test]
fn readers_do_not_overflow_on_a_huge_offset() {
    // usize::MAX + 2 must saturate rather than wrap into a valid range.
    let bytes = [1u8; 32];
    assert_eq!(le_u16(&bytes, usize::MAX), 0);
    assert_eq!(le_u32(&bytes, usize::MAX), 0);
    assert_eq!(le_u64(&bytes, usize::MAX), 0);
    assert_eq!(read_guid(&bytes, usize::MAX), [0u8; 16]);
    assert!(slice_owned(&bytes, usize::MAX, 16).is_empty());
}

#[test]
fn read_guid_copies_all_sixteen_bytes() {
    let mut bytes = [0u8; 20];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let guid = read_guid(&bytes, 2);
    assert_eq!(guid[0], 2);
    assert_eq!(guid[15], 17);
}

#[test]
fn slice_owned_truncates_to_what_is_present() {
    let bytes = [1u8, 2, 3, 4];
    assert_eq!(slice_owned(&bytes, 1, 2), vec![2, 3]);
    assert_eq!(
        slice_owned(&bytes, 2, 100),
        vec![3, 4],
        "an oversized length yields the available tail, not an error"
    );
    assert!(slice_owned(&bytes, 100, 4).is_empty());
}
