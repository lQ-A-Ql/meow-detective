//! The BitLocker Elephant Diffuser.
//!
//! Vendored from `elephant-diffuser` at commit `09f02950` (see `../NOTICE`).
//! Vendored rather than depended on: the crate has ~600 total downloads from a
//! single author, and at this size reviewing it costs less than trusting an
//! unreviewed low-adoption crate on an evidence-decryption path.
//!
//! The diffuser is **not** a cipher and holds no secret. It is a keyed,
//! invertible byte-mixing transform applied to a sector *after* AES-CBC
//! decryption, spreading each bit across the whole sector. Every other primitive
//! in BitLocker's cipher has an audited RustCrypto crate; this one does not, so
//! it is the single documented exception to "never hand-roll crypto".
//!
//! The rotation constants and cycle order follow the `dislocker` (`diffuser.c`)
//! and `libbde` reference. Correctness is not provable by a self-authored
//! round-trip — that only shows the two directions agree with each other. The
//! real proof is the public `bdetogo.raw` case exercised by
//! `tests/bitlocker_oracle.rs`; the regression vector in the unit tests was
//! captured from the oracle-validated upstream and pins the transform against
//! accidental edits.

/// Diffuser A rotation amounts, indexed by word position modulo 4.
const ROTATIONS_A: [u32; 4] = [9, 0, 13, 0];
/// Diffuser B rotation amounts, indexed by word position modulo 4.
const ROTATIONS_B: [u32; 4] = [0, 10, 0, 25];

/// Diffuser A cycle count.
const CYCLES_A: usize = 5;
/// Diffuser B cycle count.
const CYCLES_B: usize = 3;

/// Splits a sector into little-endian 32-bit words.
///
/// Trailing bytes that do not fill a word are dropped from the diffused words;
/// the per-sector-key XOR still covers them. Real BitLocker sectors are
/// word-aligned, so a sub-word remainder only arises for out-of-spec input.
pub(crate) fn to_words(sector: &[u8]) -> Vec<u32> {
    sector
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Writes words back over the sector they came from.
pub(crate) fn from_words(words: &[u32], out: &mut [u8]) {
    for (index, word) in words.iter().enumerate() {
        // `words` came from `to_words(out)`, so every slot exists; the guard keeps
        // this panic-free regardless.
        if let Some(slot) = out.get_mut(index * 4..index * 4 + 4) {
            slot.copy_from_slice(&word.to_le_bytes());
        }
    }
}

/// `(index - back) mod count`, without unsigned underflow for any input.
///
/// For real sectors (`count >= 5`) this equals the reference `(i + n - k) % n`.
/// The modulo on `back` additionally keeps it defined for the 1..=4 word counts
/// an out-of-spec caller or a fuzzer can present.
#[inline]
pub(crate) fn wrapping_back(index: usize, back: usize, count: usize) -> usize {
    (index + count - (back % count)) % count
}

/// Diffuser A, decrypt direction: `d[i] += d[i-2] ^ rol(d[i-5], Ra[i%4])`.
fn diffuser_a_decrypt(sector: &mut [u8]) {
    let mut words = to_words(sector);
    let count = words.len();
    if count == 0 {
        return;
    }
    for _ in 0..CYCLES_A {
        for index in 0..count {
            let near = words[wrapping_back(index, 2, count)];
            let far = words[wrapping_back(index, 5, count)].rotate_left(ROTATIONS_A[index % 4]);
            words[index] = words[index].wrapping_add(near ^ far);
        }
    }
    from_words(&words, sector);
}

/// Diffuser B, decrypt direction: `d[i] += d[i+2] ^ rol(d[i+5], Rb[i%4])`.
fn diffuser_b_decrypt(sector: &mut [u8]) {
    let mut words = to_words(sector);
    let count = words.len();
    if count == 0 {
        return;
    }
    for _ in 0..CYCLES_B {
        for index in 0..count {
            let near = words[(index + 2) % count];
            let far = words[(index + 5) % count].rotate_left(ROTATIONS_B[index % 4]);
            words[index] = words[index].wrapping_add(near ^ far);
        }
    }
    from_words(&words, sector);
}

/// Removes the diffuser stage from one sector, in place.
///
/// Order is Diffuser B, then Diffuser A, then the sector-key XOR. Reversing any
/// two of those three still produces 512 plausible-looking bytes with no error,
/// which is why the oracle rather than a round-trip is the real check.
pub(crate) fn decrypt(sector: &mut [u8], sector_key: &[u8; 32]) {
    diffuser_b_decrypt(sector);
    diffuser_a_decrypt(sector);
    for (index, byte) in sector.iter_mut().enumerate() {
        *byte ^= sector_key[index % 32];
    }
}

// The encrypt direction lives in the test tree, not here. Production is
// read-only and never applies the diffuser, so an encrypt path in `src` would be
// dead code — and `src` may not carry `#[cfg(test)]` items to hide it. See
// `tests/unit/shared/volume.rs`.

#[cfg(test)]
#[path = "../tests/unit/diffuser.rs"]
mod tests;
