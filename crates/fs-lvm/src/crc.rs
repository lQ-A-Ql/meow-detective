/// LVM2 uses a non-standard CRC-32 variant:
/// - Polynomial: 0xEDB88320 (reflected form, same as standard CRC-32)
/// - Initial value: 0xF597A6CF (non-standard — standard uses 0xFFFFFFFF)
/// - No final XOR (standard CRC-32/IEEE XORs with 0xFFFFFFFF)
///
/// This "weak" CRC is used for:
/// - PV label sector (bytes 20..512)
/// - MDA header (bytes 4..512)
/// - Metadata text blocks
const CRC_POLY: u32 = 0xEDB8_8320;
const CRC_INIT: u32 = 0xF597_A6CF;

/// Compute the LVM2 weak CRC-32 over `data`.
pub fn lvm_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = CRC_INIT;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ CRC_POLY;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// Verify the CRC-32 of a 512-byte PV label sector.
///
/// The stored CRC at bytes 16..20 covers bytes 20..512 of the sector.
pub fn verify_label_crc(sector: &[u8; 512]) -> bool {
    let stored = u32::from_le_bytes([sector[16], sector[17], sector[18], sector[19]]);
    let computed = lvm_crc32(&sector[20..512]);
    stored == computed
}

/// Verify the CRC-32 of a 512-byte MDA header.
///
/// The stored CRC at bytes 0..4 covers bytes 4..512 of the header.
pub fn verify_mda_header_crc(header: &[u8; 512]) -> bool {
    let stored = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let computed = lvm_crc32(&header[4..512]);
    stored == computed
}

#[cfg(test)]
#[path = "../tests/unit/crc.rs"]
mod tests;
