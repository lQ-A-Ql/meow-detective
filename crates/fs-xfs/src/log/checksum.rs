use super::{XLOG_BASIC_BLOCK_SIZE, XLOG_HEADER_CYCLE_SIZE};

const CRC32C_POLYNOMIAL: u32 = 0x82F6_3B78;
const XLOG_CRC_OFFSET: usize = 32;
const XLOG_REC_SIZE: usize = 328;
const XLOG_REC_SIZE_OTHER: usize = 324;
const XLOG_REC_EXT_SIZE: usize = 260;
const CRC32C_TABLE: [u32; 256] = build_crc32c_table();

pub(crate) fn xlog_checksum_matches(header: &[u8], packed_body: &[u8], expected: u32) -> bool {
    [XLOG_REC_SIZE, XLOG_REC_SIZE_OTHER]
        .into_iter()
        .filter_map(|header_size| xlog_checksum(header, packed_body, header_size))
        .any(|actual| actual == expected)
}

pub(crate) fn xlog_checksum(header: &[u8], packed_body: &[u8], header_size: usize) -> Option<u32> {
    if header_size < XLOG_CRC_OFFSET + 4 || header.len() < header_size {
        return None;
    }
    let mut crc = crc32c(u32::MAX, &header[..XLOG_CRC_OFFSET]);
    crc = crc32c(crc, &[0u8; 4]);
    crc = crc32c(crc, &header[XLOG_CRC_OFFSET + 4..header_size]);

    let extension_count = packed_body
        .len()
        .div_ceil(XLOG_HEADER_CYCLE_SIZE)
        .saturating_sub(1);
    for extension in 0..extension_count {
        let start = (extension + 1).checked_mul(XLOG_BASIC_BLOCK_SIZE)?;
        let end = start.checked_add(XLOG_REC_EXT_SIZE)?;
        crc = crc32c(crc, header.get(start..end)?);
    }
    crc = crc32c(crc, packed_body);
    Some(!crc)
}

pub(crate) fn crc32c(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        let index = ((crc ^ u32::from(*byte)) & 0xFF) as usize;
        crc = CRC32C_TABLE[index] ^ (crc >> 8);
    }
    crc
}

const fn build_crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0usize;
    while index < table.len() {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 != 0 {
                (value >> 1) ^ CRC32C_POLYNOMIAL
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}
