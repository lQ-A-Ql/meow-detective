/// REGF log integrity uses the fixed 64-bit Marvin state; this is not an
/// authenticity signature. All log hash regions have four-byte alignment.
pub(super) fn marvin64(data: &[u8]) -> u64 {
    let mut lo = 0x7a4e55c5u32;
    let mut hi = 0x82ef4d88u32;
    let (words, tail) = data.as_chunks::<4>();
    for word in words {
        mix(&mut lo, &mut hi, u32::from_le_bytes(*word));
    }
    let mut end = 0x80u32 << (tail.len() * 8);
    for (i, byte) in tail.iter().enumerate() {
        end |= (*byte as u32) << (i * 8);
    }
    mix(&mut lo, &mut hi, end);
    mix(&mut lo, &mut hi, 0);
    (hi as u64) << 32 | lo as u64
}
fn mix(lo: &mut u32, hi: &mut u32, word: u32) {
    *lo = lo.wrapping_add(word);
    *hi ^= *lo;
    *lo = lo.rotate_left(20).wrapping_add(*hi);
    *hi = hi.rotate_left(9) ^ *lo;
    *lo = lo.rotate_left(27).wrapping_add(*hi);
    *hi = hi.rotate_left(19);
}

#[cfg(test)]
#[path = "../../../tests/unit/registry/log_replay/hash.rs"]
mod tests;
