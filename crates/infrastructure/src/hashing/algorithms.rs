use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::io::{self, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    Md5,
    Sha1,
    Sha256,
    Sm3,
}

impl HashAlgorithm {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
            Self::Sm3 => "sm3",
        }
    }
}

pub fn digest_bytes(data: &[u8], algorithm: HashAlgorithm) -> String {
    match algorithm {
        HashAlgorithm::Md5 => hex::encode(Md5::digest(data)),
        HashAlgorithm::Sha1 => hex::encode(Sha1::digest(data)),
        HashAlgorithm::Sha256 => hex::encode(Sha256::digest(data)),
        HashAlgorithm::Sm3 => hex::encode(Sm3::digest(data)),
    }
}

pub fn digest_reader_with_cancel(
    reader: &mut dyn Read,
    algorithm: HashAlgorithm,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u64),
) -> io::Result<Option<String>> {
    let mut hasher = Hasher::new(algorithm);
    let mut buffer = [0u8; 1024 * 1024];
    let mut processed = 0u64;
    loop {
        if cancelled() {
            return Ok(None);
        }
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        processed = processed.saturating_add(count as u64);
        on_progress(processed);
    }
    Ok(Some(hasher.finalize()))
}

enum Hasher {
    Md5(Md5),
    Sha1(Sha1),
    Sha256(Sha256),
    Sm3(Sm3),
}

impl Hasher {
    fn new(algorithm: HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Md5 => Self::Md5(Md5::new()),
            HashAlgorithm::Sha1 => Self::Sha1(Sha1::new()),
            HashAlgorithm::Sha256 => Self::Sha256(Sha256::new()),
            HashAlgorithm::Sm3 => Self::Sm3(Sm3::new()),
        }
    }
    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Md5(h) => h.update(bytes),
            Self::Sha1(h) => h.update(bytes),
            Self::Sha256(h) => h.update(bytes),
            Self::Sm3(h) => h.update(bytes),
        }
    }
    fn finalize(self) -> String {
        match self {
            Self::Md5(h) => hex::encode(h.finalize()),
            Self::Sha1(h) => hex::encode(h.finalize()),
            Self::Sha256(h) => hex::encode(h.finalize()),
            Self::Sm3(h) => hex::encode(h.finalize()),
        }
    }
}

struct Sm3 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffered: usize,
    length: u64,
}

impl Sm3 {
    fn new() -> Self {
        Self {
            state: [
                0x7380_166f,
                0x4914_b2b9,
                0x1724_42d7,
                0xda8a_0600,
                0xa96f_30bc,
                0x1631_38aa,
                0xe38d_ee4d,
                0xb0fb_0e4e,
            ],
            buffer: [0; 64],
            buffered: 0,
            length: 0,
        }
    }
    fn update(&mut self, mut input: &[u8]) {
        self.length = self.length.saturating_add(input.len() as u64);
        if self.buffered != 0 {
            let take = (64 - self.buffered).min(input.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&input[..take]);
            self.buffered += take;
            input = &input[take..];
            if self.buffered == 64 {
                compress(&mut self.state, &self.buffer);
                self.buffered = 0;
            }
        }
        while input.len() >= 64 {
            compress(&mut self.state, &input[..64]);
            input = &input[64..];
        }
        self.buffer[..input.len()].copy_from_slice(input);
        self.buffered = input.len();
    }
    fn finalize(mut self) -> [u8; 32] {
        let bits = self.length.saturating_mul(8);
        self.buffer[self.buffered] = 0x80;
        self.buffered += 1;
        if self.buffered > 56 {
            self.buffer[self.buffered..].fill(0);
            compress(&mut self.state, &self.buffer);
            self.buffered = 0;
        }
        self.buffer[self.buffered..56].fill(0);
        self.buffer[56..].copy_from_slice(&bits.to_be_bytes());
        compress(&mut self.state, &self.buffer);
        let mut out = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
    fn digest(input: &[u8]) -> [u8; 32] {
        let mut h = Self::new();
        h.update(input);
        h.finalize()
    }
}

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 68];
    for (i, bytes) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes(bytes.try_into().expect("four byte word"));
    }
    for i in 16..68 {
        let x = w[i - 16] ^ w[i - 9] ^ w[i - 3].rotate_left(15);
        // W[i] = P1(W[i-16] xor W[i-9] xor (W[i-3] <<< 15)) xor W[i-13] xor W[i-6].
        w[i] = x ^ x.rotate_left(15) ^ x.rotate_left(23) ^ w[i - 13].rotate_left(7) ^ w[i - 6];
    }
    let mut v = *state;
    for i in 0..64 {
        let t: u32 = if i < 16 { 0x79cc_4519 } else { 0x7a87_9d8a };
        let ss1 = (v[0]
            .rotate_left(12)
            .wrapping_add(v[4])
            .wrapping_add(t.rotate_left(i as u32)))
        .rotate_left(7);
        let ss2 = ss1 ^ v[0].rotate_left(12);
        let ff = if i < 16 {
            v[0] ^ v[1] ^ v[2]
        } else {
            (v[0] & v[1]) | (v[0] & v[2]) | (v[1] & v[2])
        };
        let gg = if i < 16 {
            v[4] ^ v[5] ^ v[6]
        } else {
            (v[4] & v[5]) | (!v[4] & v[6])
        };
        let tt1 = ff
            .wrapping_add(v[3])
            .wrapping_add(ss2)
            .wrapping_add(w[i + 4] ^ w[i]);
        let tt2 = gg.wrapping_add(v[7]).wrapping_add(ss1).wrapping_add(w[i]);
        v[3] = v[2];
        v[2] = v[1].rotate_left(9);
        v[1] = v[0];
        v[0] = tt1;
        v[7] = v[6];
        v[6] = v[5].rotate_left(19);
        v[5] = v[4];
        v[4] = tt2 ^ tt2.rotate_left(9) ^ tt2.rotate_left(17);
    }
    for i in 0..8 {
        state[i] ^= v[i];
    }
}

#[cfg(test)]
#[path = "../../tests/unit/hashing_algorithms.rs"]
mod tests;
