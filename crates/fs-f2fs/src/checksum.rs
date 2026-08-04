const CRC32_POLYNOMIAL: u32 = 0xedb8_8320;

pub(crate) fn f2fs_crc32(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (CRC32_POLYNOMIAL & (0u32.wrapping_sub(crc & 1)));
        }
    }
    crc
}
