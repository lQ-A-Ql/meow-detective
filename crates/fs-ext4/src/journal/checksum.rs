use std::ops::Range;

use sha2::{Digest, Sha256};

const CRC32C_POLYNOMIAL: u32 = 0x82F6_3B78;

pub(crate) fn crc32c(mut crc: u32, data: &[u8]) -> u32 {
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (CRC32C_POLYNOMIAL & mask);
        }
    }
    crc
}

pub(crate) fn journal_checksum_seed(uuid: &[u8; 16]) -> u32 {
    crc32c(u32::MAX, uuid)
}

pub(crate) fn crc32c_with_zeroed_range(
    seed: u32,
    data: &[u8],
    zeroed: Range<usize>,
) -> Option<u32> {
    if zeroed.start > zeroed.end || zeroed.end > data.len() {
        return None;
    }
    let mut crc = crc32c(seed, &data[..zeroed.start]);
    let zeroes = [0u8; 32];
    let mut remaining = zeroed.len();
    while remaining > 0 {
        let count = remaining.min(zeroes.len());
        crc = crc32c(crc, &zeroes[..count]);
        remaining -= count;
    }
    Some(crc32c(crc, &data[zeroed.end..]))
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
