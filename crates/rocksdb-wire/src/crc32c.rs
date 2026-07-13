const POLYNOMIAL: u32 = 0x82f6_3b78;
const MASK_DELTA: u32 = 0xa282_ead8;

/// Extends a RocksDB-compatible CRC32C value with additional bytes.
pub fn extend_crc32c(initial: u32, data: &[u8]) -> u32 {
    let mut crc = !initial;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (POLYNOMIAL & mask);
        }
    }
    !crc
}

/// Computes the unmasked CRC32C value used by RocksDB.
pub fn crc32c(data: &[u8]) -> u32 {
    extend_crc32c(0, data)
}

/// Masks a CRC32C value for on-disk storage.
pub fn mask_crc32c(crc: u32) -> u32 {
    crc.rotate_right(15).wrapping_add(MASK_DELTA)
}

/// Reverses RocksDB's on-disk CRC32C mask.
pub fn unmask_crc32c(masked_crc: u32) -> u32 {
    masked_crc.wrapping_sub(MASK_DELTA).rotate_right(17)
}
