/// Ceph uses the reflected CRC32C polynomial with the caller-provided
/// running remainder and no final XOR.
pub const CEPH_CRC32C_INITIAL: u32 = u32::MAX;

const POLYNOMIAL: u32 = 0x82f6_3b78;

pub fn crc32c(initial: u32, data: &[u8]) -> u32 {
    let mut crc = initial;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (POLYNOMIAL & mask);
        }
    }
    crc
}

pub fn ceph_crc32c(data: &[u8]) -> u32 {
    crc32c(CEPH_CRC32C_INITIAL, data)
}
