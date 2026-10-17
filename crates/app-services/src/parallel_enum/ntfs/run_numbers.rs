//! Little-endian numbers used by NTFS run-list decoding.

pub(super) fn read_sized_le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .enumerate()
        .take(8)
        .fold(0, |value, (index, byte)| {
            value | (*byte as u64) << (index * 8)
        })
}

pub(super) fn read_sized_le_signed(bytes: &[u8]) -> i64 {
    let length = bytes.len().min(8);
    if length == 0 {
        return 0;
    }
    let mut value = read_sized_le(&bytes[..length]);
    if bytes[length - 1] & 0x80 != 0 {
        for index in length..8 {
            value |= 0xFFu64 << (index * 8);
        }
    }
    value as i64
}
