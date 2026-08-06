const POLYNOMIAL: u32 = 0x82f6_3b78;

const fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < table.len() {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (POLYNOMIAL & mask);
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
}

const TABLE: [u32; 256] = build_table();

pub(crate) fn checksum(bytes: &[u8]) -> u32 {
    checksum_parts(&[bytes])
}

pub(crate) fn checksum_parts(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for bytes in parts {
        for byte in *bytes {
            let index = ((crc ^ u32::from(*byte)) & 0xff) as usize;
            crc = TABLE[index] ^ (crc >> 8);
        }
    }
    !crc
}
