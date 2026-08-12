//! 64-bit hash functions used by the systemd journal on-disk format.
//!
//! Files without `HEADER_INCOMPATIBLE_KEYED_HASH` (the pre-v246 layout, e.g.
//! RHEL7/CentOS7 era journals) hash DATA and FIELD payloads with an unkeyed
//! Jenkins lookup3 variant: `jenkins_hashlittle2()` with zero initvals,
//! combined as `(pc << 32) | pb`. Since v246 new files default to
//! `HEADER_INCOMPATIBLE_KEYED_HASH`, where the payload hash is SipHash-2-4
//! keyed by the header `file_id`.
//!
//! Ported from systemd's `src/libsystemd/sd-journal/lookup3.c` (public
//! domain, Bob Jenkins) and `src/basic/siphash24.c` (CC0, Aumasson/DJB).

/// Jenkins lookup3 64-bit hash (`hashlittle2` with zero initvals), matching
/// systemd's `jenkins_hash64()`.
pub fn jenkins_hash64(data: &[u8]) -> u64 {
    let (pc, pb) = hashlittle2(data, 0, 0);
    ((pc as u64) << 32) | u64::from(pb)
}

/// SipHash-2-4 with a 16-byte key, matching systemd's `siphash24()`.
pub fn siphash24(data: &[u8], key: &[u8; 16]) -> u64 {
    let k0 = u64::from_le_bytes(key[..8].try_into().unwrap_or([0; 8]));
    let k1 = u64::from_le_bytes(key[8..].try_into().unwrap_or([0; 8]));

    let mut v0 = 0x736f_6d65_7073_6575u64 ^ k0;
    let mut v1 = 0x646f_7261_6e64_6f6du64 ^ k1;
    let mut v2 = 0x6c79_6765_6e65_7261u64 ^ k0;
    let mut v3 = 0x7465_6462_7974_6573u64 ^ k1;

    let mut chunks = data.chunks_exact(8);
    for block in &mut chunks {
        let m = u64::from_le_bytes(block.try_into().unwrap_or([0; 8]));
        v3 ^= m;
        sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        v0 ^= m;
    }

    let mut last = (data.len() as u64) << 56;
    for (index, byte) in chunks.remainder().iter().enumerate() {
        last |= u64::from(*byte) << (8 * index);
    }

    v3 ^= last;
    sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    v0 ^= last;
    v2 ^= 0xff;
    for _ in 0..4 {
        sipround(&mut v0, &mut v1, &mut v2, &mut v3);
    }

    v0 ^ v1 ^ v2 ^ v3
}

fn sipround(v0: &mut u64, v1: &mut u64, v2: &mut u64, v3: &mut u64) {
    *v0 = v0.wrapping_add(*v1);
    *v1 = v1.rotate_left(13);
    *v1 ^= *v0;
    *v0 = v0.rotate_left(32);
    *v2 = v2.wrapping_add(*v3);
    *v3 = v3.rotate_left(16);
    *v3 ^= *v2;
    *v0 = v0.wrapping_add(*v3);
    *v3 = v3.rotate_left(21);
    *v3 ^= *v0;
    *v2 = v2.wrapping_add(*v1);
    *v1 = v1.rotate_left(17);
    *v1 ^= *v2;
    *v2 = v2.rotate_left(32);
}

/// Bob Jenkins' lookup3 `hashlittle2`, byte-wise little-endian variant
/// (identical output to the aligned variants on little-endian writers).
/// Returns `(pc, pb)`; the 64-bit form is `(pc << 32) | pb`.
fn hashlittle2(key: &[u8], pc: u32, pb: u32) -> (u32, u32) {
    let mut a = 0xdead_beefu32
        .wrapping_add(key.len() as u32)
        .wrapping_add(pc);
    let mut b = a;
    let mut c = a.wrapping_add(pb);

    let mut rest = key;
    while rest.len() > 12 {
        a = a.wrapping_add(le_u32(&rest[0..4]));
        b = b.wrapping_add(le_u32(&rest[4..8]));
        c = c.wrapping_add(le_u32(&rest[8..12]));
        lookup3_mix(&mut a, &mut b, &mut c);
        rest = &rest[12..];
    }

    // The final block (1..=12 bytes) affects all of `c` and is followed by
    // `final()`; a zero-length key returns the initial state untouched.
    if rest.is_empty() {
        return (c, b);
    }
    for (index, byte) in rest.iter().enumerate() {
        let value = u32::from(*byte) << (8 * (index % 4));
        match index / 4 {
            0 => a = a.wrapping_add(value),
            1 => b = b.wrapping_add(value),
            _ => c = c.wrapping_add(value),
        }
    }
    lookup3_final(&mut a, &mut b, &mut c);
    (c, b)
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().unwrap_or([0; 4]))
}

fn lookup3_mix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *a = a.wrapping_sub(*c);
    *a ^= c.rotate_left(4);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= a.rotate_left(6);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= b.rotate_left(8);
    *b = b.wrapping_add(*a);
    *a = a.wrapping_sub(*c);
    *a ^= c.rotate_left(16);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= a.rotate_left(19);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= b.rotate_left(4);
    *b = b.wrapping_add(*a);
}

fn lookup3_final(a: &mut u32, b: &mut u32, c: &mut u32) {
    *c ^= *b;
    *c = c.wrapping_sub(b.rotate_left(14));
    *a ^= *c;
    *a = a.wrapping_sub(c.rotate_left(11));
    *b ^= *a;
    *b = b.wrapping_sub(a.rotate_left(25));
    *c ^= *b;
    *c = c.wrapping_sub(b.rotate_left(16));
    *a ^= *c;
    *a = a.wrapping_sub(c.rotate_left(4));
    *b ^= *a;
    *b = b.wrapping_sub(a.rotate_left(14));
    *c ^= *b;
    *c = c.wrapping_sub(b.rotate_left(24));
}

/// Payload hash function selected by the file header: keyed SipHash-2-4 for
/// `HEADER_INCOMPATIBLE_KEYED_HASH` files, unkeyed Jenkins otherwise.
#[derive(Debug, Clone, Copy)]
pub(super) enum JournalHash {
    Jenkins,
    SipHash([u8; 16]),
}

impl JournalHash {
    pub(super) fn hash(&self, data: &[u8]) -> u64 {
        match self {
            Self::Jenkins => jenkins_hash64(data),
            Self::SipHash(key) => siphash24(data, key),
        }
    }
}
