const POLYNOMIAL: u32 = 0x82f6_3b78;

pub(crate) fn checksum(bytes: &[u8]) -> u32 {
    checksum_parts(&[bytes])
}

pub(crate) fn checksum_parts(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for bytes in parts {
        for byte in *bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = 0u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (POLYNOMIAL & mask);
            }
        }
    }
    !crc
}
